//! Terminal interface, built on The Elm Architecture via `tears`.
//!
//! # Why TEA
//!
//! The previous implementation interleaved event reading, state mutation and
//! drawing inside one `loop`. Under TEA those are three separate things:
//!
//! - **Message** — every transition the app can undergo (`message.rs`)
//! - **update** — a pure function of `(state, Message)`; no I/O, no drawing
//! - **view** — a pure function of state; no mutation
//!
//! The payoff is that input handling and state transitions are testable
//! without a tty, which is what the unit tests below exercise.
//!
//! # A note on `hojicha`
//!
//! Hojicha was evaluated as the runtime alongside `tears` and could not be
//! used with either. Its 0.2.1 line pins ratatui 0.29 while `tears` requires
//! 0.30 — two semver-incompatible ratatui crates in one binary, so a Hojicha
//! widget cannot be rendered into a `tears` frame. Its 0.2.2 line drops
//! ratatui entirely (`Model::view(&self) -> String`), which is incompatible
//! with keeping ratatui at all. The component decomposition below follows
//! Hojicha's Bubble Tea-style shape; only the crate is absent.

mod components;
mod keymap;
mod message;
mod theme;

use std::num::NonZeroU32;

use anyhow::{Result, anyhow};
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use tears::prelude::*;
use tears::subscription::terminal::TerminalEvents;

use crate::model::AppEntry;
use components::detail::DetailPanel;
use components::header::{self, HeaderBar};
use components::help::HelpOverlay;
use components::search::SearchBox;
use components::statusbar::StatusBar;
use components::table::{self, DataGrid};
use message::{Message, Mode};
use theme::Theme;

/// Below this the layout cannot show a useful number of rows, so we say so
/// rather than rendering something broken (tui-design §2, minimum size gate).
const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 10;

pub struct App {
    entries: Vec<AppEntry>,
    /// Indices into `entries`, after filtering and sorting. The single source
    /// of truth for what the grid displays and in what order.
    rows: Vec<usize>,
    mode: Mode,
    /// Where `?` returns to, so help can be opened from any mode.
    resume_mode: Mode,
    grid: DataGrid,
    search: SearchBox,
    theme: Theme,
}

impl App {
    fn selected_entry(&self) -> Option<&AppEntry> {
        let cursor = self.grid.selected()?;
        self.rows.get(cursor).map(|&i| &self.entries[i])
    }

    /// Recompute `rows` from the current filter and sort, and park the cursor
    /// on the first row.
    ///
    /// Re-sorting deliberately returns to the top rather than tracking the
    /// previously selected entry: the point of pressing "sort by usage" is to
    /// see the most-used units, and following the old selection strands the
    /// viewport somewhere in the middle of a list the user just reordered.
    fn rebuild(&mut self) {
        let mut rows: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.search.matches(e))
            .map(|(i, _)| i)
            .collect();

        let (col, asc) = (self.grid.sort_col, self.grid.sort_asc);
        rows.sort_by(|&a, &b| {
            let ord = table::compare(&self.entries[a], &self.entries[b], col);
            if asc { ord } else { ord.reverse() }
        });
        self.rows = rows;
        self.grid.select(if self.rows.is_empty() { None } else { Some(0) });
    }

    fn render_too_small(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(Span::styled(" [ VIEWPORT UNDERSIZED ] ", self.theme.status_band())),
            Line::from(Span::styled(
                format!(
                    " NEED {MIN_WIDTH}x{MIN_HEIGHT} — HAVE {}x{} ",
                    area.width, area.height
                ),
                self.theme.base(),
            )),
        ];
        frame.render_widget(Paragraph::new(lines).style(self.theme.base()), area);
    }
}

impl Application for App {
    type Message = Message;
    type Flags = Vec<AppEntry>;

    fn new(entries: Vec<AppEntry>) -> (Self, Command<Self::Message>) {
        let mut app = Self {
            entries,
            rows: Vec::new(),
            mode: Mode::Browse,
            resume_mode: Mode::Browse,
            grid: DataGrid::default(),
            search: SearchBox::default(),
            theme: Theme::detect(),
        };
        app.rebuild();
        (app, Command::none())
    }

    fn update(&mut self, msg: Self::Message) -> Command<Self::Message> {
        match msg {
            // Raw events are translated, then re-entered as semantic messages.
            // Everything below this line is independent of crossterm.
            Message::Terminal(Event::Key(key)) => {
                return match keymap::translate(self.mode, key) {
                    Some(m) => self.update(m),
                    None => Command::none(),
                };
            }
            Message::Terminal(_) => {}
            Message::TerminalError(e) => {
                // A dead input stream means the session is over; leaving the
                // loop running would hang with no way to quit.
                eprintln!("terminal input failed: {e}");
                return Command::quit();
            }

            Message::Quit => return Command::quit(),

            Message::Move(delta) => self.grid.move_by(delta, self.rows.len()),
            Message::JumpTop => self.grid.move_by(i32::MIN / 2, self.rows.len()),
            Message::JumpBottom => self.grid.move_by(i32::MAX / 2, self.rows.len()),

            Message::SortBy(col) => {
                self.grid.toggle_sort(col);
                self.rebuild();
            }

            Message::SearchOpen => self.mode = Mode::Search,
            Message::SearchPush(c) => {
                self.search.push(c);
                self.rebuild();
            }
            Message::SearchPop => {
                self.search.pop();
                self.rebuild();
            }
            Message::SearchCommit => self.mode = Mode::Browse,
            Message::SearchCancel => {
                self.search.clear();
                self.rebuild();
                self.mode = Mode::Browse;
            }

            Message::DetailOpen => {
                // Nothing to inspect on an empty list — stay put rather than
                // opening a blank record.
                if self.selected_entry().is_some() {
                    self.mode = Mode::Detail;
                }
            }
            Message::DetailClose => self.mode = Mode::Browse,

            Message::HelpToggle => {
                if self.mode == Mode::Help {
                    self.mode = self.resume_mode;
                } else {
                    self.resume_mode = self.mode;
                    self.mode = Mode::Help;
                }
            }
        }
        Command::none()
    }

    fn view(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            self.render_too_small(frame, area);
            return;
        }

        let [head, body, foot] = Layout::vertical([
            Constraint::Length(header::HEIGHT),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

        HeaderBar {
            entries: &self.entries,
            shown: self.rows.len(),
            sort_col: self.grid.sort_col,
            sort_asc: self.grid.sort_asc,
            query: self.search.query(),
        }
        .render(frame, head, &self.theme);

        // The grid always draws; overlays compose on top of it so the user
        // keeps their spatial context.
        self.grid
            .render(frame, body, &self.entries, &self.rows, &self.theme);

        match self.mode {
            Mode::Detail => {
                if let Some(entry) = self.selected_entry() {
                    DetailPanel { entry }.render(frame, body, &self.theme);
                }
            }
            Mode::Help => HelpOverlay.render(frame, body, &self.theme),
            _ => {}
        }

        if self.mode == Mode::Search {
            self.search
                .render(frame, foot, self.rows.len(), &self.theme);
        } else {
            StatusBar {
                mode: self.mode,
                position: self.grid.selected().filter(|_| !self.rows.is_empty()),
                total: self.rows.len(),
            }
            .render(frame, foot, &self.theme);
        }
    }

    fn subscriptions(&self) -> Vec<Subscription<Self::Message>> {
        // Terminal input only. The inventory is a static snapshot, so there is
        // no timer subscription and the process stays idle between keystrokes.
        vec![
            Subscription::new(TerminalEvents::new()).map(|result| match result {
                Ok(event) => Message::Terminal(event),
                Err(e) => Message::TerminalError(e.to_string()),
            }),
        ]
    }
}

pub async fn run(entries: Vec<AppEntry>) -> Result<()> {
    // `ratatui::init` enters the alternate screen, enables raw mode, and
    // installs a panic hook that restores both — so a panic cannot leave the
    // user with a wedged terminal.
    let mut terminal = ratatui::init();

    let frame_rate = FrameRate::new(NonZeroU32::new(30).expect("30 is non-zero"))
        .map_err(|e| anyhow!("invalid frame rate: {e}"))?;

    let result = Runtime::<App>::new(entries, frame_rate).run(&mut terminal).await;

    ratatui::restore();
    result.map_err(|e| anyhow!("tui runtime failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Language, Source};
    use message::Column;

    fn entry(name: &str, usage: u32, source: Source) -> AppEntry {
        AppEntry {
            name: name.into(),
            version: None,
            source,
            language: Some(Language::Rust),
            install_date: None,
            last_used: None,
            usage_count: usage,
            path: None,
            description: None,
        }
    }

    fn app() -> App {
        let entries = vec![
            entry("charlie", 5, Source::Npm),
            entry("alpha", 100, Source::Homebrew),
            entry("bravo", 50, Source::Cargo),
        ];
        App::new(entries).0
    }

    /// `update` hands back a `Command` for the runtime to execute. These tests
    /// exercise state transitions only, so the command is deliberately dropped.
    fn send(app: &mut App, msg: Message) {
        let _ = app.update(msg);
    }

    fn names(a: &App) -> Vec<&str> {
        a.rows.iter().map(|&i| a.entries[i].name.as_str()).collect()
    }

    #[test]
    fn starts_sorted_by_name_ascending() {
        assert_eq!(names(&app()), ["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn sorting_by_usage_puts_most_used_first_then_reverses() {
        let mut a = app();
        // First press adopts the column's default direction: most-used first.
        send(&mut a, Message::SortBy(Column::Usage));
        assert_eq!(names(&a), ["alpha", "bravo", "charlie"]);
        assert!(!a.grid.sort_asc, "usage must start descending");
        // Second press reverses it.
        send(&mut a, Message::SortBy(Column::Usage));
        assert_eq!(names(&a), ["charlie", "bravo", "alpha"]);
    }

    /// Re-sorting returns the cursor to the top so the user sees the new
    /// ordering rather than being stranded mid-list.
    #[test]
    fn sorting_returns_the_cursor_to_the_top() {
        let mut a = app();
        send(&mut a, Message::Move(2));
        assert_eq!(a.grid.selected(), Some(2));
        send(&mut a, Message::SortBy(Column::Usage));
        assert_eq!(a.grid.selected(), Some(0));
        assert_eq!(a.selected_entry().unwrap().name, "alpha", "most-used on top");
    }

    #[test]
    fn filtering_narrows_rows_and_cancel_restores_them() {
        let mut a = app();
        send(&mut a, Message::SearchOpen);
        for c in "alp".chars() {
            send(&mut a, Message::SearchPush(c));
        }
        assert_eq!(names(&a), ["alpha"]);
        send(&mut a, Message::SearchCancel);
        assert_eq!(names(&a), ["alpha", "bravo", "charlie"]);
        assert_eq!(a.mode, Mode::Browse);
    }

    /// A filter that matches nothing must leave no cursor, and the detail
    /// overlay must refuse to open on it.
    #[test]
    fn empty_result_set_has_no_cursor_and_no_record() {
        let mut a = app();
        send(&mut a, Message::SearchOpen);
        for c in "zzzz".chars() {
            send(&mut a, Message::SearchPush(c));
        }
        assert!(a.rows.is_empty());
        assert_eq!(a.grid.selected(), None);
        send(&mut a, Message::DetailOpen);
        assert_eq!(a.mode, Mode::Search, "detail must not open with no selection");
    }

    /// `?` is reachable from any mode and returns to where it was opened.
    #[test]
    fn help_returns_to_the_mode_it_was_opened_from() {
        let mut a = app();
        send(&mut a, Message::DetailOpen);
        assert_eq!(a.mode, Mode::Detail);
        send(&mut a, Message::HelpToggle);
        assert_eq!(a.mode, Mode::Help);
        send(&mut a, Message::HelpToggle);
        assert_eq!(a.mode, Mode::Detail);
    }

    #[test]
    fn jump_messages_reach_both_ends() {
        let mut a = app();
        send(&mut a, Message::JumpBottom);
        assert_eq!(a.grid.selected(), Some(2));
        send(&mut a, Message::JumpTop);
        assert_eq!(a.grid.selected(), Some(0));
    }
}

/// Render-to-buffer harness. Not a unit test of logic — it draws real frames
/// through ratatui's `TestBackend` so layout regressions surface without a tty.
///
/// `cargo test render -- --nocapture` prints the frames.
#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::model::{Language, Source};
    use chrono::{Local, TimeZone};
    use message::Column;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample() -> Vec<AppEntry> {
        let d = |y, m, day| Local.with_ymd_and_hms(y, m, day, 9, 0, 0).single();
        let mk = |name: &str, ver: &str, src, lang, usage, path: &str, day| AppEntry {
            name: name.into(),
            version: Some(ver.into()),
            source: src,
            language: Some(lang),
            install_date: d(2025, 3, day),
            last_used: d(2026, 7, 20),
            usage_count: usage,
            path: Some(path.into()),
            description: None,
        };
        vec![
            mk("ripgrep", "14.1.1", Source::Homebrew, Language::Rust, 842, "/opt/homebrew/bin/rg", 4),
            mk("fd", "10.2.0", Source::Homebrew, Language::Rust, 310, "/opt/homebrew/bin/fd", 4),
            mk("Visual Studio Code", "1.99", Source::Applications, Language::Electron, 0, "/Applications/Visual Studio Code.app", 11),
            mk("typescript", "5.7.2", Source::Npm, Language::JavaScript, 27, "/usr/local/lib/node_modules/typescript", 18),
            mk("httpie", "3.2.4", Source::Pip, Language::Python, 6, "/usr/local/bin/http", 21),
            mk("bat", "0.25.0", Source::Cargo, Language::Rust, 96, "~/.cargo/bin/bat", 2),
            mk("gopls", "0.17.1", Source::Go, Language::Go, 3, "~/go/bin/gopls", 9),
            mk("Docker", "27.4", Source::HomebrewCask, Language::Go, 0, "/Applications/Docker.app", 14),
        ]
    }

    fn draw(app: &App, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).expect("test backend");
        term.draw(|f| app.view(f)).expect("draw");
        let buf = term.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn banner(title: &str, body: &str) {
        println!("\n┌── {title} {}\n{body}", "─".repeat(60usize.saturating_sub(title.len())));
    }

    #[test]
    fn render_browse_120x24() {
        let app = App::new(sample()).0;
        let out = draw(&app, 120, 24);
        banner("BROWSE 120x24", &out);
        assert!(out.contains("SYSAPP"), "identity plate missing");
        assert!(out.contains("[ INVENTORY ]"), "counters missing");
        assert!(out.contains("NAME"), "column headers missing");
        assert!(out.contains("BROWSE"), "mode label missing from footer");
    }

    #[test]
    fn render_sorted_by_usage_120x24() {
        let mut app = App::new(sample()).0;
        let _ = app.update(Message::SortBy(Column::Usage));
        let out = draw(&app, 120, 24);
        banner("SORT BY USAGE 120x24", &out);
        // Anchoring on a line index would break whenever the header grows, so
        // assert the ordering relation instead.
        let pos = |needle: &str| out.find(needle).unwrap_or_else(|| panic!("{needle} missing"));
        assert!(
            pos("ripgrep") < pos("bat") && pos("bat") < pos("httpie"),
            "rows must descend by invocation count"
        );
    }

    #[test]
    fn render_search_120x24() {
        let mut app = App::new(sample()).0;
        let _ = app.update(Message::SearchOpen);
        for c in "rust".chars() {
            let _ = app.update(Message::SearchPush(c));
        }
        let out = draw(&app, 120, 24);
        banner("SEARCH \"rust\" 120x24", &out);
        assert!(out.contains("SEARCH"), "search band missing");
        assert!(out.contains("MATCH"), "hit count missing");
        assert!(!out.contains("httpie"), "python entry should be filtered out");
    }

    #[test]
    fn render_detail_120x24() {
        let mut app = App::new(sample()).0;
        let _ = app.update(Message::DetailOpen);
        let out = draw(&app, 120, 24);
        banner("DETAIL OVERLAY 120x24", &out);
        assert!(out.contains("UNIT RECORD"), "overlay title missing");
        assert!(out.contains("INVOCATIONS"), "overlay fields missing");
    }

    #[test]
    fn render_help_120x30() {
        let mut app = App::new(sample()).0;
        let _ = app.update(Message::HelpToggle);
        let out = draw(&app, 120, 30);
        banner("HELP OVERLAY 120x30", &out);
        assert!(out.contains("KEY REFERENCE"), "help title missing");
    }

    #[test]
    fn render_narrow_80x24() {
        let app = App::new(sample()).0;
        let out = draw(&app, 80, 24);
        banner("BROWSE 80x24", &out);
        assert!(out.contains("SYSAPP"), "must still render at the 80x24 floor");
    }

    /// Below the floor we must say so, not draw a mangled layout.
    #[test]
    fn render_undersized_40x8() {
        let app = App::new(sample()).0;
        let out = draw(&app, 40, 8);
        banner("UNDERSIZED 40x8", &out);
        assert!(out.contains("VIEWPORT UNDERSIZED"));
    }

    /// Every frame must survive an empty result set without panicking.
    #[test]
    fn render_no_matches() {
        let mut app = App::new(sample()).0;
        let _ = app.update(Message::SearchOpen);
        for c in "zzzz".chars() {
            let _ = app.update(Message::SearchPush(c));
        }
        let out = draw(&app, 120, 24);
        banner("NO MATCHES 120x24", &out);
        assert!(out.contains("NO UNITS MATCH FILTER"));
    }
}

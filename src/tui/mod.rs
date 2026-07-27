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

use std::num::{NonZeroU32, NonZeroU64};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Local};
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use tears::prelude::*;
use tears::subscription::terminal::TerminalEvents;
use tears::subscription::time::{Timer, TimerEvent};

use crate::model::AppEntry;
use components::detail::DetailPanel;
use components::header::{self, HeaderBar};
use components::help::HelpOverlay;
use components::scanning::{ScanningScreen, SourceState};
use components::search::SearchBox;
use components::statusbar::StatusBar;
use components::table::{self, DataGrid};
use message::{Message, Mode};
use theme::Theme;

/// Below this the layout cannot show a useful number of rows, so we say so
/// rather than rendering something broken (tui-design §2, minimum size gate).
const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 10;

/// Braille spinner cadence (tui-design §5): fast enough to read as motion,
/// slow enough not to burn a wake-up budget.
const SPINNER_INTERVAL_MS: u64 = 120;

/// Below this the grid and the record panel cannot both be legible, so the
/// record falls back to a modal.
const SIDE_PANEL_MIN_WIDTH: u16 = 116;

/// Breathing room between the interface and the terminal edge. Content that
/// runs to the very edge of the window reads as cramped and makes the panel
/// borders fight the terminal's own frame.
const MARGIN_X: u16 = 2;
const MARGIN_Y: u16 = 1;

/// Gap between the inventory and record panels, so their borders do not touch.
const PANEL_GAP: u16 = 1;

/// Fixed width of the record panel. The grid takes whatever remains, so this
/// and `SIDE_PANEL_MIN_WIDTH` together set the grid's minimum content budget —
/// see the column widths in `components::table`.
const RECORD_PANEL_WIDTH: u16 = 36;

/// How long without use before a unit counts as idle. Six months is long
/// enough to survive a quarter of neglect but short enough to still flag
/// things worth uninstalling.
const IDLE_MONTHS: i64 = 6;

fn idle_cutoff() -> DateTime<Local> {
    Local::now() - chrono::Duration::days(IDLE_MONTHS * 30)
}

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
    /// When the displayed inventory was scanned. `None` means it was scanned
    /// during this launch, so it is live rather than restored from cache.
    generated_at: Option<DateTime<Local>>,
    /// A background rescan is in flight. The UI stays fully interactive; only
    /// the status band changes.
    refreshing: bool,
    /// Spinner frame, advanced by `Tick` while refreshing.
    tick: usize,
    /// Transient one-line feedback (rescan finished, rescan failed).
    notice: Option<String>,
    /// Hide packaging noise. On by default: 115 of 906 entries on a typical
    /// machine are pkgutil receipts that dilute every sort and search.
    hide_noise: bool,
    /// Show only units with no evidence of use.
    idle_only: bool,
    /// Cold-start scan progress. `None` once the inventory is ready.
    scan: Option<ScanProgress>,
    /// Per-source totals for the record panel. Recomputed only when the
    /// inventory itself changes — not on every keystroke.
    source_counts: Vec<(String, usize)>,
    /// Terminal width, so `update` can tell whether the record is already on
    /// screen. Seeded from the real terminal and kept current by resize events;
    /// without it, Enter would flip the mode label to DETAIL on a wide terminal
    /// where the record panel is already visible and nothing would change.
    width: u16,
}

/// Counts per source, descending. Sources with zero entries are dropped.
fn source_counts(entries: &[AppEntry]) -> Vec<(String, usize)> {
    use crate::model::Source;
    const ORDER: [Source; 9] = [
        Source::Homebrew,
        Source::HomebrewCask,
        Source::Applications,
        Source::Cargo,
        Source::Go,
        Source::Npm,
        Source::Pip,
        Source::Gem,
        Source::Pkgutil,
    ];
    let mut out: Vec<(String, usize)> = ORDER
        .iter()
        .map(|s| {
            let n = entries.iter().filter(|e| &e.source == s).count();
            (s.to_string().to_uppercase(), n)
        })
        .filter(|(_, n)| *n > 0)
        .collect();
    out.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    out
}

/// Per-source progress during a cold scan.
struct ScanProgress {
    sources: Vec<(&'static str, SourceState)>,
    collected: Vec<AppEntry>,
    enriching: bool,
}

impl ScanProgress {
    fn new() -> Self {
        Self {
            sources: crate::scanner::SOURCES
                .iter()
                .map(|s| (*s, SourceState::Pending))
                .collect(),
            collected: Vec::new(),
            enriching: false,
        }
    }

    fn all_reported(&self) -> bool {
        self.sources
            .iter()
            .all(|(_, st)| !matches!(st, SourceState::Pending))
    }
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
        self.rebuild_anchored(None);
    }

    /// Rebuild, then put the cursor back on the entry with `anchor` as its
    /// name. Used after a background rescan: the list contents changed
    /// underneath the user, but the unit they were looking at should not move.
    fn rebuild_anchored(&mut self, anchor: Option<String>) {
        let mut rows: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.visible(e))
            .map(|(i, _)| i)
            .collect();

        let (col, asc) = (self.grid.sort_col, self.grid.sort_asc);
        rows.sort_by(|&a, &b| {
            let ord = table::compare(&self.entries[a], &self.entries[b], col);
            if asc { ord } else { ord.reverse() }
        });
        self.rows = rows;

        let cursor = match anchor {
            Some(name) => self
                .rows
                .iter()
                .position(|&i| self.entries[i].name == name)
                .or(if self.rows.is_empty() { None } else { Some(0) }),
            None if self.rows.is_empty() => None,
            None => Some(0),
        };
        self.grid.select(cursor);
    }

    /// Every filter that decides whether an entry reaches the grid.
    fn visible(&self, e: &AppEntry) -> bool {
        if self.hide_noise && e.is_system_noise() {
            return false;
        }
        if self.idle_only && !e.is_idle(idle_cutoff()) {
            return false;
        }
        self.search.matches(e)
    }

    /// How many entries the noise filter is currently withholding. Shown in
    /// the header so the totals can never look inexplicably wrong.
    fn hidden_noise(&self) -> usize {
        if !self.hide_noise {
            return 0;
        }
        self.entries.iter().filter(|e| e.is_system_noise()).count()
    }

    /// Filter and sort summary, shown as the grid panel's first line.
    fn stats_line(&self) -> Line<'_> {
        let mut spans = vec![
            Span::styled(format!(" {}", self.rows.len()), self.theme.heading()),
            Span::styled(" shown", self.theme.muted()),
        ];
        let hidden = self.hidden_noise();
        if hidden > 0 {
            spans.push(Span::styled(format!("   {hidden}"), self.theme.base()));
            spans.push(Span::styled(" noise hidden", self.theme.muted()));
        }
        if self.idle_only {
            spans.push(Span::styled("   IDLE ONLY", self.theme.accented()));
        }
        if !self.search.query().is_empty() {
            spans.push(Span::styled(
                format!("   /{}", self.search.query()),
                self.theme.accented(),
            ));
        }
        spans.push(Span::styled(
            format!(
                "   sorted by {} {}",
                self.grid.sort_col.label(),
                if self.grid.sort_asc { "▲" } else { "▼" }
            ),
            self.theme.muted(),
        ));
        Line::from(spans)
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
    /// `None` means "no cache — scan from scratch", and the app opens on the
    /// progress screen instead of the grid.
    type Flags = Option<(Vec<AppEntry>, Option<DateTime<Local>>)>;

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let (entries, generated_at) = flags.clone().unwrap_or_default();
        let mut app = Self {
            entries,
            rows: Vec::new(),
            mode: Mode::Browse,
            resume_mode: Mode::Browse,
            grid: DataGrid::default(),
            search: SearchBox::default(),
            theme: Theme::detect(),
            generated_at,
            refreshing: false,
            tick: 0,
            notice: None,
            hide_noise: true,
            idle_only: false,
            scan: None,
            source_counts: Vec::new(),
            width: crossterm::terminal::size().map(|(w, _)| w).unwrap_or(120),
        };
        app.source_counts = source_counts(&app.entries);
        app.rebuild();

        // With no cache, open the UI immediately on a progress screen and run
        // every source concurrently behind it. Nothing blocks before first
        // paint — the terminal is never blank.
        if flags.is_none() {
            app.scan = Some(ScanProgress::new());
            let commands = crate::scanner::SOURCES.map(|name| {
                Command::perform(crate::scanner::scan_one(name), move |result| {
                    Message::SourceScanned(name, result.map_err(|e| format!("{e:#}")))
                })
            });
            return (app, Command::batch(commands));
        }
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
            Message::Terminal(Event::Resize(w, _)) => self.width = w,
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
                // On a wide terminal the record panel is already on screen, so
                // there is nothing to open — flipping the mode would only make
                // the footer claim a state change that never happened.
                let has_side_panel = self.width >= SIDE_PANEL_MIN_WIDTH;
                if !has_side_panel && self.selected_entry().is_some() {
                    self.mode = Mode::Detail;
                }
            }
            Message::DetailClose => self.mode = Mode::Browse,

            Message::RefreshStart => {
                // Ignore a second `r` while one rescan is already running —
                // two concurrent brew queries would be slower than one.
                if self.refreshing || self.scan.is_some() {
                    return Command::none();
                }
                self.refreshing = true;
                self.tick = 0;
                self.notice = None;
                return Command::perform(rescan(), |result| match result {
                    Ok(entries) => Message::RefreshDone(entries),
                    Err(e) => Message::RefreshFailed(e),
                });
            }
            Message::RefreshDone(entries) if entries.is_empty() => {
                // Same rule on the rescan path: keep the inventory we have.
                self.refreshing = false;
                self.notice = Some("RESCAN FOUND NOTHING — KEEPING PREVIOUS DATA".into());
            }
            Message::RefreshDone(entries) => {
                let anchor = self.selected_entry().map(|e| e.name.clone());
                let count = entries.len();
                self.entries = entries;
                self.source_counts = source_counts(&self.entries);
                self.generated_at = None; // freshly scanned — this is live data
                self.refreshing = false;
                self.notice = Some(format!("RESCAN COMPLETE — {count} UNITS"));
                // Filter and sort are preserved; only the underlying data moved.
                self.rebuild_anchored(anchor);
            }
            Message::RefreshFailed(e) => {
                // Keep the old inventory. Stale data beats an empty screen.
                self.refreshing = false;
                self.notice = Some(format!("RESCAN FAILED — {e}"));
            }
            Message::Tick => self.tick = self.tick.wrapping_add(1),

            Message::SourceScanned(name, result) => {
                let Some(scan) = self.scan.as_mut() else {
                    return Command::none();
                };
                let state = match result {
                    Ok(entries) => {
                        let n = entries.len();
                        scan.collected.extend(entries);
                        SourceState::Done(n)
                    }
                    // One dead source must not abort the whole scan — a machine
                    // without Go or gem installed is normal, not an error.
                    Err(e) => SourceState::Skipped(e),
                };
                if let Some(slot) = scan.sources.iter_mut().find(|(n, _)| *n == name) {
                    slot.1 = state;
                }

                if scan.all_reported() {
                    scan.enriching = true;
                    let merged = crate::scanner::merge(std::mem::take(&mut scan.collected));
                    return Command::perform(
                        crate::enricher::enrich_owned(merged),
                        Message::EnrichDone,
                    );
                }
                return Command::none();
            }
            Message::EnrichDone(entries) => {
                // A scan that yielded nothing means every source failed, not
                // that the machine has no packages. Surface it and leave the
                // cache alone rather than persisting the failure.
                if entries.is_empty() {
                    self.notice = Some("SCAN FOUND NOTHING — press r to retry".into());
                } else if let Err(e) = crate::cache::save(&entries) {
                    self.notice = Some(format!("CACHE WRITE FAILED — {e}"));
                }
                self.entries = entries;
                self.source_counts = source_counts(&self.entries);
                self.generated_at = None;
                self.scan = None;
                self.rebuild();
            }

            Message::ToggleNoise => {
                self.hide_noise = !self.hide_noise;
                self.notice = Some(if self.hide_noise {
                    "PACKAGING NOISE HIDDEN".into()
                } else {
                    "SHOWING ALL SOURCES".into()
                });
                self.rebuild();
            }
            Message::ToggleIdleOnly => {
                self.idle_only = !self.idle_only;
                self.notice = Some(if self.idle_only {
                    format!("IDLE ONLY — NO USE IN {IDLE_MONTHS} MONTHS")
                } else {
                    "SHOWING ALL UNITS".into()
                });
                self.rebuild();
            }

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

        if let Some(scan) = &self.scan {
            ScanningScreen {
                sources: &scan.sources,
                tick: self.tick,
                enriching: scan.enriching,
            }
            .render(frame, area, &self.theme);
            return;
        }

        // Paint the whole frame before drawing anything into it. Widget styles
        // cover only the cells they write; column gaps, panel padding and short
        // rows would otherwise expose the user's terminal background — which on
        // a themed terminal shows up as vertical stripes through the grid.
        // Painting the full area (not the inset one) also fills the margin.
        frame.render_widget(Block::default().style(self.theme.base()), area);

        // Inset everything from the terminal edge.
        let area = Rect {
            x: area.x + MARGIN_X,
            y: area.y + MARGIN_Y,
            width: area.width.saturating_sub(MARGIN_X * 2),
            height: area.height.saturating_sub(MARGIN_Y * 2),
        };

        let [head, body, foot] = Layout::vertical([
            Constraint::Length(header::HEIGHT),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

        HeaderBar {
            total: self.entries.len(),
            generated_at: self.generated_at,
            title: "MACOS PACKAGE INVENTORY",
        }
        .render(frame, head, &self.theme);

        // Master-detail when there is room for both; the record falls back to a
        // modal on narrow terminals rather than squeezing the grid to nothing.
        let side_by_side = frame.area().width >= SIDE_PANEL_MIN_WIDTH;
        let (grid_area, record_area) = if side_by_side {
            let [g, _gap, r] = Layout::horizontal([
                Constraint::Min(48),
                Constraint::Length(PANEL_GAP),
                Constraint::Length(RECORD_PANEL_WIDTH),
            ])
            .areas(body);
            (g, Some(r))
        } else {
            (body, None)
        };

        self.grid.render(
            frame,
            grid_area,
            &self.entries,
            &self.rows,
            self.stats_line(),
            &self.theme,
        );

        let record = DetailPanel {
            entry: self.selected_entry(),
            sources: &self.source_counts,
        };
        if let Some(r) = record_area {
            record.render_side(frame, r, &self.theme);
        }

        match self.mode {
            // With a side panel the record is already on screen, so Enter is a
            // no-op there; on a narrow terminal it still opens the modal.
            Mode::Detail if !side_by_side => record.render_modal(frame, body, &self.theme),
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
                refreshing: self.refreshing,
                tick: self.tick,
                notice: self.notice.as_deref(),
            }
            .render(frame, foot, &self.theme);
        }
    }

    fn subscriptions(&self) -> Vec<Subscription<Self::Message>> {
        // `subscriptions` is a pure function of state, so the spinner timer
        // exists only while a rescan is running. Idle sessions subscribe to
        // terminal input alone and the process stays asleep between keystrokes.
        let mut subs = vec![
            Subscription::new(TerminalEvents::new()).map(|result| match result {
                Ok(event) => Message::Terminal(event),
                Err(e) => Message::TerminalError(e.to_string()),
            }),
        ];
        if self.refreshing || self.scan.is_some() {
            subs.push(
                Subscription::new(Timer::new(
                    NonZeroU64::new(SPINNER_INTERVAL_MS).expect("non-zero"),
                ))
                .map(|TimerEvent::Tick| Message::Tick),
            );
        }
        subs
    }
}

/// Full rescan, run off the UI thread by `Command::perform`.
///
/// Errors come back as a `String` rather than propagating: a failed rescan
/// must not take down a session that is still showing perfectly usable data.
async fn rescan() -> Result<Vec<AppEntry>, String> {
    let mut entries = crate::scanner::scan_all()
        .await
        .map_err(|e| format!("{e:#}"))?;
    crate::enricher::enrich(&mut entries).await;
    if let Err(e) = crate::cache::save(&entries) {
        // The user still gets fresh data; only the next launch pays for this.
        return Err(format!("scanned but cache write failed: {e:#}"));
    }
    Ok(entries)
}

/// `snapshot` is `None` on a cold start: the UI opens on the progress screen
/// and scans behind it.
pub async fn run(snapshot: Option<(Vec<AppEntry>, Option<DateTime<Local>>)>) -> Result<()> {
    // `ratatui::init` enters the alternate screen, enables raw mode, and
    // installs a panic hook that restores both — so a panic cannot leave the
    // user with a wedged terminal.
    let mut terminal = ratatui::init();

    let frame_rate = FrameRate::new(NonZeroU32::new(30).expect("30 is non-zero"))
        .map_err(|e| anyhow!("invalid frame rate: {e}"))?;

    let result = Runtime::<App>::new(snapshot, frame_rate)
        .run(&mut terminal)
        .await;

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

    /// `App::new` seeds `width` from the real terminal, so state tests must
    /// pin it or their behaviour depends on whoever runs the suite — CI has no
    /// tty and took the fallback, which silently changed what Enter does.
    fn app() -> App {
        let entries = vec![
            entry("charlie", 5, Source::Npm),
            entry("alpha", 100, Source::Homebrew),
            entry("bravo", 50, Source::Cargo),
        ];
        let mut a = App::new(Some((entries, None))).0;
        a.width = 80; // narrow: the record is a modal, so Enter is meaningful
        a
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

    /// A rescan must not block the UI, and a second `r` while one is running
    /// must be ignored rather than launching a duplicate brew query.
    #[test]
    fn refresh_is_single_flight_and_non_blocking() {
        let mut a = app();
        assert!(!a.refreshing);
        send(&mut a, Message::RefreshStart);
        assert!(a.refreshing);

        // Navigation, sorting and filtering all still work mid-rescan.
        send(&mut a, Message::Move(1));
        assert_eq!(a.grid.selected(), Some(1));
        send(&mut a, Message::SortBy(Column::Usage));
        assert_eq!(a.grid.sort_col, Column::Usage);

        // A second refresh while one is in flight changes nothing.
        send(&mut a, Message::RefreshStart);
        assert!(a.refreshing);
    }

    /// After a rescan the cursor stays on the same *unit*, and any active
    /// filter survives.
    #[test]
    fn refresh_preserves_cursor_and_filter() {
        let mut a = app();
        send(&mut a, Message::SearchOpen);
        for c in "l".chars() {
            send(&mut a, Message::SearchPush(c));
        }
        // Only "alpha" and "charlie" contain an 'l' — "bravo" does not, and
        // neither do any of the source/language/path fields.
        assert_eq!(names(&a), ["alpha", "charlie"]);
        send(&mut a, Message::Move(1));
        assert_eq!(a.selected_entry().unwrap().name, "charlie");

        // Fresh scan returns the same units in a different order, plus a new one.
        let fresh = vec![
            // "echo" deliberately has no 'l', so the active filter still
            // yields exactly the two units the test tracks.
            entry("echo", 1, Source::Npm),
            entry("charlie", 5, Source::Npm),
            entry("alpha", 100, Source::Homebrew),
            entry("bravo", 50, Source::Cargo),
        ];
        send(&mut a, Message::RefreshStart);
        send(&mut a, Message::RefreshDone(fresh));

        assert!(!a.refreshing);
        assert_eq!(a.entries.len(), 4, "new inventory adopted");
        assert_eq!(names(&a), ["alpha", "charlie"], "filter still applied");
        assert_eq!(
            a.selected_entry().unwrap().name,
            "charlie",
            "cursor stayed on the same unit"
        );
        assert!(a.generated_at.is_none(), "rescanned data is live, not a snapshot");
    }

    /// A rescan that returns nothing means every source failed; the previous
    /// inventory must survive and the empty result must not be cached.
    #[test]
    fn empty_rescan_keeps_previous_inventory() {
        let mut a = app();
        send(&mut a, Message::RefreshStart);
        send(&mut a, Message::RefreshDone(vec![]));
        assert!(!a.refreshing);
        assert_eq!(a.entries.len(), 3, "previous inventory retained");
        assert!(a.notice.as_deref().unwrap_or_default().contains("NOTHING"));
    }

    /// A failed rescan must keep the existing inventory on screen.
    #[test]
    fn failed_refresh_keeps_old_data() {
        let mut a = app();
        send(&mut a, Message::RefreshStart);
        send(&mut a, Message::RefreshFailed("brew exploded".into()));

        assert!(!a.refreshing);
        assert_eq!(a.entries.len(), 3, "old inventory retained");
        assert!(a.notice.as_deref().unwrap_or_default().contains("FAILED"));
    }

    /// Packaging noise is hidden by default, and the header is told how much
    /// is being withheld so the totals stay explicable.
    #[test]
    fn noise_is_hidden_by_default_and_toggles() {
        let mut entries = vec![entry("ripgrep", 5, Source::Homebrew)];
        entries.push(entry("com.apple.pkg.Foo", 0, Source::Pkgutil));
        let mut a = App::new(Some((entries, None))).0;

        assert_eq!(names(&a), ["ripgrep"], "pkgutil hidden by default");
        assert_eq!(a.hidden_noise(), 1);

        send(&mut a, Message::ToggleNoise);
        assert_eq!(names(&a), ["com.apple.pkg.Foo", "ripgrep"]);
        assert_eq!(a.hidden_noise(), 0, "nothing withheld once shown");
    }

    /// The idle view keeps only units with no evidence of use, and composes
    /// with the search filter rather than replacing it.
    #[test]
    fn idle_view_filters_and_composes_with_search() {
        let mut used = entry("ripgrep", 500, Source::Homebrew);
        used.name = "ripgrep".into();
        let unused_a = entry("abandoned-tool", 0, Source::Homebrew);
        let unused_b = entry("another-dead-app", 0, Source::Cargo);
        let mut a = App::new(Some((vec![used, unused_a, unused_b], None))).0;

        assert_eq!(names(&a).len(), 3);
        send(&mut a, Message::ToggleIdleOnly);
        assert_eq!(names(&a), ["abandoned-tool", "another-dead-app"]);

        // Search still applies on top of the idle filter.
        send(&mut a, Message::SearchOpen);
        for c in "another".chars() {
            send(&mut a, Message::SearchPush(c));
        }
        assert_eq!(names(&a), ["another-dead-app"]);

        send(&mut a, Message::ToggleIdleOnly);
        assert_eq!(names(&a), ["another-dead-app"], "search survives the toggle");
    }

    /// A recently-opened GUI app is not idle even with zero shell invocations.
    #[test]
    fn recently_opened_app_is_not_idle() {
        let mut recent = entry("Zed", 0, Source::Applications);
        recent.last_used = Some(chrono::Local::now() - chrono::Duration::days(3));
        let mut a = App::new(Some((vec![recent], None))).0;

        send(&mut a, Message::ToggleIdleOnly);
        assert!(a.rows.is_empty(), "recently used app must not be listed as idle");
    }


    /// Enter opens the record only when it is not already on screen. This is
    /// width-dependent, so both branches are pinned explicitly.
    #[test]
    fn enter_opens_the_record_only_when_it_is_hidden() {
        let mut narrow = app();
        narrow.width = SIDE_PANEL_MIN_WIDTH - 1;
        send(&mut narrow, Message::DetailOpen);
        assert_eq!(narrow.mode, Mode::Detail, "narrow: Enter must open the modal");

        let mut wide = app();
        wide.width = SIDE_PANEL_MIN_WIDTH;
        send(&mut wide, Message::DetailOpen);
        assert_eq!(
            wide.mode,
            Mode::Browse,
            "wide: the record panel is already visible, so Enter must be inert"
        );
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
        let app = App::new(Some((sample(), None))).0;
        let out = draw(&app, 120, 24);
        banner("BROWSE 120x24", &out);
        assert!(out.contains("SYSAPP"), "identity band missing");
        assert!(out.contains("INVENTORY"), "grid panel title missing");
        assert!(out.contains("RECORD"), "record panel missing at this width");
        assert!(out.contains("shown"), "stats line missing");
        assert!(out.contains("NAME"), "column headers missing");
        assert!(out.contains("BROWSE"), "mode label missing from footer");
    }

    #[test]
    fn render_sorted_by_usage_120x24() {
        let mut app = App::new(Some((sample(), None))).0;
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
        let mut app = App::new(Some((sample(), None))).0;
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

    /// Wide terminals carry the record as a persistent side panel — no key
    /// press required, and Enter must not replace it with a modal.
    #[test]
    fn render_record_side_panel_120x24() {
        let mut app = App::new(Some((sample(), None))).0;
        app.width = 120;
        let out = draw(&app, 120, 24);
        banner("RECORD SIDE PANEL 120x24", &out);
        assert!(out.contains("RECORD"), "record panel title missing");
        assert!(out.contains("INVOCATIONS"), "record fields missing");
        // SOURCES is inventory-wide context, not part of the record, so it
        // yields to the record itself on a short terminal. Checked at a
        // realistic height below.
        let tall = draw(&app, 120, 34);
        assert!(tall.contains("SOURCES"), "source breakdown missing on a tall terminal");

        let _ = app.update(Message::DetailOpen);
        let after = draw(&app, 120, 24);
        assert_eq!(after, out, "Enter must be inert while the side panel is up");
    }

    /// Narrow terminals drop the side panel; Enter still opens the modal.
    #[test]
    fn render_record_modal_when_narrow() {
        let mut app = App::new(Some((sample(), None))).0;
        app.width = 80;
        let plain = draw(&app, 80, 24);
        assert!(!plain.contains("INVOCATIONS"), "no side panel at 80 cols");

        let _ = app.update(Message::DetailOpen);
        let out = draw(&app, 80, 24);
        banner("RECORD MODAL 80x24", &out);
        assert!(out.contains("RECORD"), "modal title missing");
        assert!(out.contains("INVOCATIONS"), "modal fields missing");
    }

    #[test]
    fn render_help_120x30() {
        let mut app = App::new(Some((sample(), None))).0;
        let _ = app.update(Message::HelpToggle);
        let out = draw(&app, 120, 30);
        banner("HELP OVERLAY 120x30", &out);
        assert!(out.contains("KEY REFERENCE"), "help title missing");
    }

    #[test]
    fn render_narrow_80x24() {
        let app = App::new(Some((sample(), None))).0;
        let out = draw(&app, 80, 24);
        banner("BROWSE 80x24", &out);
        assert!(out.contains("SYSAPP"), "must still render at the 80x24 floor");
    }

    /// Below the floor we must say so, not draw a mangled layout.
    #[test]
    fn render_undersized_40x8() {
        let app = App::new(Some((sample(), None))).0;
        let out = draw(&app, 40, 8);
        banner("UNDERSIZED 40x8", &out);
        assert!(out.contains("VIEWPORT UNDERSIZED"));
    }

    /// Reproduces the reported artefact: with a non-black terminal background,
    /// vertical stripes appear between columns. Cell styles paint cells; the
    /// column gaps and any unwritten area belong to whatever is underneath.
    #[test]
    fn probe_unpainted_cells() {
        let app = App::new(Some((sample(), None))).0;
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).unwrap();
        term.draw(|f| app.view(f)).unwrap();
        let buf = term.backend().buffer().clone();
        let mut unpainted = 0;
        let mut total = 0;
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                total += 1;
                if buf[(x, y)].bg == ratatui::style::Color::Reset { unpainted += 1; }
            }
        }
        println!("UNPAINTED CELLS: {unpainted}/{total} ({:.0}%)", 100.0 * unpainted as f64 / total as f64);
        // Row 7 is inside the data grid — sample the column-gap columns.
        let row: String = (0..60).map(|x| if buf[(x,7)].bg == ratatui::style::Color::Reset {'.'} else {'#'}).collect();
        println!("ROW 7 PAINT MAP (. = shows terminal bg): {row}");
    }


    /// Regression guard for the background bleed: unpainted cells expose the
    /// user's terminal background, which on a themed terminal appeared as
    /// vertical stripes through the grid. This measured 44% before the fix.
    #[test]
    fn every_cell_is_painted() {
        let mut app = App::new(Some((sample(), None))).0;
        app.width = 130;
        let mut term =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(130, 30)).unwrap();
        term.draw(|f| app.view(f)).unwrap();
        let buf = term.backend().buffer().clone();
        let mut unpainted = 0;
        let total = (buf.area.width as usize) * (buf.area.height as usize);
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                if buf[(x, y)].bg == ratatui::style::Color::Reset {
                    unpainted += 1;
                }
            }
        }
        println!("UNPAINTED: {unpainted}/{total}");
        assert_eq!(unpainted, 0, "{unpainted} cells would leak the terminal background");
    }

    #[test]
    fn render_cold_start_scan_screen() {
        // `None` flags = no cache, so the app opens on the progress screen.
        let (mut app, _cmd) = App::new(None);
        let out = draw(&app, 120, 24);
        banner("COLD START — ALL PENDING", &out);
        assert!(out.contains("NO SNAPSHOT"), "must explain why it is scanning");
        assert!(out.contains("BREW"), "sources must be listed");
        assert!(out.contains("slowest source"), "brew must be flagged as slow");

        // Partial progress: brew done, one source unavailable.
        let _ = app.update(Message::SourceScanned("brew", Ok(vec![])));
        let _ = app.update(Message::SourceScanned("go", Err("go not installed".into())));
        let out = draw(&app, 120, 24);
        banner("COLD START — PARTIAL", &out);
        assert!(out.contains("0 UNITS"), "completed source shows its count");
        assert!(out.contains("SKIPPED"), "unavailable source is marked, not fatal");
    }

    /// A source that fails must not abort the scan or lose the others.
    #[test]
    fn failed_source_does_not_abort_the_scan() {
        let (mut app, _) = App::new(None);
        for name in crate::scanner::SOURCES {
            let _ = app.update(Message::SourceScanned(name, Err("boom".into())));
        }
        // All reported (all failed) → enrichment still runs and completes.
        let _ = app.update(Message::EnrichDone(vec![]));
        assert!(app.scan.is_none(), "scan screen must clear even if every source failed");
    }

    /// Column widths must fit the grid panel at the narrowest side-by-side
    /// layout. Overflow makes ratatui clip the rightmost headers silently —
    /// which shipped twice as `5·INSTALLE`.
    #[test]
    fn columns_fit_at_minimum_width() {
        let mut app = App::new(Some((sample(), None))).0;
        app.width = SIDE_PANEL_MIN_WIDTH;
        let out = draw(&app, SIDE_PANEL_MIN_WIDTH, 26);
        banner("MINIMUM SIDE-BY-SIDE WIDTH", &out);
        for header in ["1·NAME", "2·SRC", "3·LANG", "4·VER", "5·INSTALLED", "6·USAGE"] {
            assert!(out.contains(header), "header {header:?} was clipped");
        }
    }

    /// Every frame must survive an empty result set without panicking.
    #[test]
    fn render_no_matches() {
        let mut app = App::new(Some((sample(), None))).0;
        let _ = app.update(Message::SearchOpen);
        for c in "zzzz".chars() {
            let _ = app.update(Message::SearchPush(c));
        }
        let out = draw(&app, 120, 24);
        banner("NO MATCHES 120x24", &out);
        assert!(out.contains("NO UNITS MATCH FILTER"));
    }
}

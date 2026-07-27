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
pub mod i18n;
mod keymap;
mod message;
mod theme;

use std::num::{NonZeroU32, NonZeroU64};
use std::time::{Duration, Instant};

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

use crate::enricher::interface::PathIndex;
use crate::exec::{ExecPlan, Unrunnable};
use crate::model::{AppEntry, Category};
use components::category::CategoryInput;
use components::detail::DetailPanel;
use components::header::{self, HeaderBar};
use components::help::HelpOverlay;
use components::scanning::{ScanningScreen, SourceState};
use components::search::SearchBox;
use components::sourcecards::{self, SourceCards};
use components::statusbar::StatusBar;
use components::table::{self, DataGrid};
use i18n::Lang;
use message::{Message, Mode};
use theme::Theme;

/// Below this the layout cannot show a useful number of rows, so we say so
/// rather than rendering something broken (tui-design §2, minimum size gate).
const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 10;

/// Reserve one row and one column of every card's area for its drop shadow,
/// paint the shadow, and return the rect the card itself should occupy.
///
/// The shadow is an L along the right and bottom edges, offset by one cell so
/// it reads as cast rather than as a border. It is taken out of the card's own
/// allocation, so nothing else in the layout has to know about it.
pub(super) fn cast_shadow(frame: &mut Frame, area: Rect, theme: &Theme) -> Rect {
    if area.width < 3 || area.height < 3 {
        return area; // no room to spend on decoration
    }
    let card = Rect {
        width: area.width - 1,
        height: area.height - 1,
        ..area
    };
    let right = Rect {
        x: card.x + card.width,
        y: card.y + 1,
        width: 1,
        height: card.height,
    };
    let bottom = Rect {
        x: card.x + 1,
        y: card.y + card.height,
        width: card.width,
        height: 1,
    };
    frame.render_widget(Block::default().style(theme.shadow_style()), right);
    frame.render_widget(Block::default().style(theme.shadow_style()), bottom);
    card
}

/// How long a footer notice stays up before the key hints return.
const NOTICE_TTL: Duration = Duration::from_secs(4);

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
    /// The name being typed while `Mode::Category` holds focus. Separate from
    /// `search` because it targets one entry rather than filtering the list.
    category: CategoryInput,
    /// Show only this category. `None` means every category is shown — it is
    /// not the same as filtering on `Uncategorized`, which is a real bucket.
    category_filter: Option<Category>,
    /// Where `C` persists an assignment. Resolved once at construction rather
    /// than per keypress, and held rather than re-derived so tests can point
    /// it at a scratch file instead of the developer's own config. `None` on a
    /// platform with no home directory: the assignment still applies on
    /// screen, but the failure to persist it has to be said out loud.
    config_path: Option<std::path::PathBuf>,
    /// Executable index, built once at construction.
    ///
    /// One sweep of every directory on `PATH` costs a few milliseconds against
    /// a warm cache and buys O(1) lookups afterwards, which is what lets Enter
    /// decide whether a unit is launchable inside `update` without touching
    /// the filesystem per keypress.
    path_index: PathIndex,
    /// The launch waiting on `y`. `Some` exactly while `Mode::Confirm` is
    /// active — the plan is built before the question is asked so the answer
    /// cannot be about something that turns out to be unrunnable.
    pending_exec: Option<PendingExec>,
    /// Where a foreground launch is left for `run` to pick up after the
    /// runtime has exited. Shared rather than owned because `Runtime::run`
    /// consumes the `Application` and never gives it back.
    suspend: SuspendSlot,
    theme: Theme,
    /// When the displayed inventory was scanned. `None` means it was scanned
    /// during this launch, so it is live rather than restored from cache.
    generated_at: Option<DateTime<Local>>,
    /// A background rescan is in flight. The UI stays fully interactive; only
    /// the status band changes.
    refreshing: bool,
    /// Spinner frame, advanced by `Tick` while refreshing.
    tick: usize,
    /// Transient one-line feedback (rescan finished, filter toggled, language
    /// switched). It occupies the footer, so it must give the key hints back.
    notice: Option<String>,
    /// When the notice stops being shown. Without this every notice was
    /// permanent: one press of `p`, `s` or `L` replaced the key hints for the
    /// rest of the session.
    notice_until: Option<Instant>,
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
    /// Interface language. Detected from the locale, toggled with `L`.
    lang: Lang,
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

/// A planned launch, held between the question and the answer.
struct PendingExec {
    name: String,
    plan: ExecPlan,
}

/// Everything the relaunched TUI needs to look like it never went away.
///
/// A terminal program takes the tty, so running one means tearing this TUI
/// down and building a new one afterwards. Without this the user would come
/// back to row 0 of an unfiltered, unsorted, English list — which reads as
/// having lost their place rather than as having run something.
#[derive(Debug, Clone)]
pub struct ResumeState {
    /// Anchored by name rather than by index: the inventory is re-read from
    /// cache on the way back, and a row number would point at whatever had
    /// moved into that slot.
    cursor: Option<String>,
    query: String,
    sort_col: message::Column,
    sort_asc: bool,
    hide_noise: bool,
    idle_only: bool,
    category_filter: Option<Category>,
    lang: Lang,
}

/// A terminal program to run once the TUI is out of the way.
pub struct Suspend {
    pub program: std::path::PathBuf,
    pub args: Vec<String>,
    pub name: String,
    pub resume: ResumeState,
}

/// How the request escapes the runtime.
///
/// `tears::Runtime::run` consumes the `Runtime` and never hands the
/// `Application` back, so a field on `App` cannot be read after the program
/// exits. The slot is handed in through `Flags` and read by `run` once the
/// runtime has returned — `update` still only *records*, and the process still
/// only ever has one of these.
type SuspendSlot = std::sync::Arc<std::sync::Mutex<Option<Suspend>>>;

/// What a launch of the interface starts from.
#[derive(Default)]
pub struct Flags {
    /// `None` means "no cache — scan from scratch", and the app opens on the
    /// progress screen instead of the grid.
    snapshot: Option<(Vec<AppEntry>, Option<DateTime<Local>>)>,
    /// Set when this launch is the far side of a suspend.
    resume: Option<ResumeState>,
    suspend: SuspendSlot,
}

/// Lets the tests keep saying `App::new(Some((entries, None)).into())` — they
/// care about the inventory and nothing else about how the process was started.
impl From<Option<(Vec<AppEntry>, Option<DateTime<Local>>)>> for Flags {
    fn from(snapshot: Option<(Vec<AppEntry>, Option<DateTime<Local>>)>) -> Self {
        Self {
            snapshot,
            ..Self::default()
        }
    }
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
        let cutoff = idle_cutoff();
        let mut rows: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.visible(e, cutoff))
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

    /// Categories the `c` key can cycle through: built-ins in their declared
    /// order, then `Custom` ones by name.
    ///
    /// Only categories that actually hold entries are listed. Most of the
    /// eleven built-ins are empty on any given machine, and cycling through a
    /// filter that yields nothing is a dead keypress the user has to press
    /// their way out of.
    ///
    /// Presence is read from `entries` rather than `rows`, so the cycle keeps
    /// its shape while the search box narrows the list. Deriving it from the
    /// visible rows would make the same key land on a different category
    /// depending on state the user cannot see from the footer.
    fn present_categories(&self) -> Vec<Category> {
        let mut built_in: Vec<Category> = Category::BUILT_IN
            .iter()
            .filter(|c| {
                self.entries
                    .iter()
                    .any(|e| e.category.as_ref() == Some(*c))
            })
            .cloned()
            .collect();

        let mut custom: Vec<String> = self
            .entries
            .iter()
            .filter_map(|e| match e.category.as_ref() {
                Some(Category::Custom(name)) => Some(name.clone()),
                _ => None,
            })
            .collect();
        custom.sort_unstable();
        custom.dedup();

        built_in.extend(custom.into_iter().map(Category::Custom));
        built_in
    }

    /// Everything a suspend has to carry across the process boundary.
    ///
    /// Deliberately not the whole `App`: the inventory is re-read from cache
    /// on the way back, and carrying a stale copy of it across would show the
    /// user data older than the file on disk.
    fn resume_state(&self) -> ResumeState {
        ResumeState {
            cursor: self.selected_entry().map(|e| e.name.clone()),
            query: self.search.query().to_string(),
            sort_col: self.grid.sort_col,
            sort_asc: self.grid.sort_asc,
            hide_noise: self.hide_noise,
            idle_only: self.idle_only,
            category_filter: self.category_filter.clone(),
            lang: self.lang,
        }
    }

    /// Reinstate a `ResumeState`, returning the name to anchor the cursor on.
    ///
    /// The caller rebuilds afterwards rather than this doing it, because the
    /// filters have to be set before the rows are built or the anchor is
    /// searched for in a list that has not been filtered yet.
    fn restore(&mut self, state: ResumeState) -> Option<String> {
        self.search.set_query(&state.query);
        self.grid.sort_col = state.sort_col;
        self.grid.sort_asc = state.sort_asc;
        self.hide_noise = state.hide_noise;
        self.idle_only = state.idle_only;
        self.category_filter = state.category_filter;
        self.lang = state.lang;
        state.cursor
    }

    /// Show a transient message in the footer.
    ///
    /// Always use this rather than assigning `notice` directly — a notice with
    /// no deadline never clears, and the footer never returns to showing the
    /// keys (tui-design §3: toasts auto-dismiss).
    fn notify(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
        self.notice_until = Some(Instant::now() + NOTICE_TTL);
    }

    /// Whether the record is rendered as a persistent side panel.
    ///
    /// The single source of truth for this question. `update` and `view` both
    /// consult it, so a mode that only makes sense in one layout cannot
    /// survive into the other.
    fn side_by_side(&self) -> bool {
        self.width >= SIDE_PANEL_MIN_WIDTH
    }

    /// Every filter that decides whether an entry reaches the grid.
    ///
    /// `cutoff` is passed in rather than read from the clock here: this runs
    /// once per entry per rebuild, so calling `Local::now()` inside would mean
    /// ~900 timezone resolutions per keystroke while filtering, and would make
    /// the filter time-dependent mid-pass.
    fn visible(&self, e: &AppEntry, cutoff: DateTime<Local>) -> bool {
        if self.hide_noise && e.is_system_noise() {
            return false;
        }
        if self.idle_only && !e.is_idle(cutoff) {
            return false;
        }
        if let Some(wanted) = &self.category_filter
            && e.category.as_ref() != Some(wanted)
        {
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
        let t = self.lang.strings();
        let mut spans = vec![
            Span::styled(format!(" {}", self.rows.len()), self.theme.heading()),
            Span::styled(format!(" {}", t.shown), self.theme.muted()),
        ];
        let hidden = self.hidden_noise();
        if hidden > 0 {
            spans.push(Span::styled(format!("   {hidden}"), self.theme.base()));
            spans.push(Span::styled(format!(" {}", t.noise_hidden), self.theme.muted()));
        }
        if self.idle_only {
            spans.push(Span::styled(format!("   {}", t.idle_only), self.theme.accented()));
        }
        if let Some(cat) = &self.category_filter {
            spans.push(Span::styled(
                format!("   {}{}", t.cat_filter, i18n::category_label(cat, self.lang)),
                self.theme.accented(),
            ));
        }
        if !self.search.query().is_empty() {
            spans.push(Span::styled(
                format!("   /{}", self.search.query()),
                self.theme.accented(),
            ));
        }
        spans.push(Span::styled(
            format!(
                "   {}{} {}",
                t.sorted_by,
                self.grid.sort_col.label(self.lang),
                if self.grid.sort_asc { "▲" } else { "▼" }
            ),
            self.theme.muted(),
        ));
        Line::from(spans)
    }

    fn render_too_small(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(Span::styled(
                format!(" {} ", self.lang.strings().viewport_undersized),
                self.theme.masthead(),
            )),
            Line::from(Span::styled(
                format!(
                    " {} {MIN_WIDTH}x{MIN_HEIGHT} — {} {}x{} ",
                    self.lang.strings().need,
                    self.lang.strings().have,
                    area.width,
                    area.height
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
    type Flags = Flags;

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let snapshot = flags.snapshot;
        let (entries, generated_at) = snapshot.clone().unwrap_or_default();
        let mut app = Self {
            entries,
            rows: Vec::new(),
            mode: Mode::Browse,
            resume_mode: Mode::Browse,
            grid: DataGrid::default(),
            search: SearchBox::default(),
            category: CategoryInput::default(),
            category_filter: None,
            config_path: crate::config::path(),
            path_index: PathIndex::build(),
            pending_exec: None,
            suspend: flags.suspend,
            theme: Theme::detect(),
            generated_at,
            refreshing: false,
            tick: 0,
            notice: None,
            notice_until: None,
            hide_noise: true,
            idle_only: false,
            scan: None,
            source_counts: Vec::new(),
            lang: Lang::detect(),
            width: crossterm::terminal::size().map(|(w, _)| w).unwrap_or(120),
        };
        app.source_counts = source_counts(&app.entries);

        // Restore before the first `rebuild`, so the filters are already in
        // place when the rows are built and the cursor is anchored against the
        // list the user will actually see.
        let anchor = flags.resume.and_then(|r| app.restore(r));
        app.rebuild_anchored(anchor);

        // With no cache, open the UI immediately on a progress screen and run
        // every source concurrently behind it. Nothing blocks before first
        // paint — the terminal is never blank.
        if snapshot.is_none() {
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
            Message::Terminal(Event::Resize(w, _)) => {
                self.width = w;
                // Detail only exists as a fallback for terminals too narrow to
                // carry the side panel. Widening past the threshold makes the
                // modal stop rendering, so holding the mode would leave the
                // user in a browse-looking screen where the Detail keymap
                // binds almost nothing and navigation silently dies.
                if self.mode == Mode::Detail && self.side_by_side() {
                    self.mode = Mode::Browse;
                }
                if self.resume_mode == Mode::Detail && self.side_by_side() {
                    self.resume_mode = Mode::Browse;
                }
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

            Message::CategoryFilterCycle => {
                let present = self.present_categories();
                if present.is_empty() {
                    self.notify(self.lang.strings().category_filter_none);
                    return Command::none();
                }
                // `None` → first → … → last → `None`. The wrap back through
                // "no filter" is what makes one key both enter and leave the
                // filtered view; without it the user would need a second key
                // just to get the full list back.
                let next = match &self.category_filter {
                    None => Some(present[0].clone()),
                    Some(current) => present
                        .iter()
                        .position(|c| c == current)
                        .and_then(|i| present.get(i + 1))
                        .cloned(),
                };
                self.notify(match &next {
                    Some(cat) => i18n::category_label(cat, self.lang).into_owned(),
                    None => self.lang.strings().category_filter_all.to_string(),
                });
                self.category_filter = next;
                self.rebuild();
            }

            Message::CategoryOpen => {
                // Nothing selected means nothing to label, and entering the
                // mode would trap the user in an edit that can never commit.
                if self.selected_entry().is_some() {
                    self.category.clear();
                    self.mode = Mode::Category;
                }
            }
            // No rebuild on either of these: the text names a category to
            // write rather than a query, so refiltering per keystroke would
            // only move the cursor off the entry being labelled.
            Message::CategoryPush(c) => self.category.push(c),
            Message::CategoryPop => self.category.pop(),
            Message::CategoryCommit => {
                let Some(name) = self.selected_entry().map(|e| e.name.clone()) else {
                    self.category.clear();
                    self.mode = Mode::Browse;
                    return Command::none();
                };
                let Some(index) = self.entries.iter().position(|e| e.name == name) else {
                    self.category.clear();
                    self.mode = Mode::Browse;
                    return Command::none();
                };

                let typed = self.category.input().trim().to_string();
                let t = self.lang.strings();
                let target = self
                    .config_path
                    .clone()
                    .ok_or_else(|| anyhow!("platform has no home directory"));

                let (written, confirmation) = if typed.is_empty() {
                    // A blank name is not a category. Storing `Custom("")`
                    // would render as an empty cell the user then has no way
                    // to select and remove, so empty input means "undo my
                    // override" and the automatic classification comes back.
                    let written =
                        target.and_then(|p| crate::config::clear_override_in(&p, &name));
                    crate::enricher::category::enrich_categories(
                        std::slice::from_mut(&mut self.entries[index]),
                        &crate::config::load(),
                    );
                    (written, t.category_cleared)
                } else {
                    let cat = Category::parse(&typed);
                    let written =
                        target.and_then(|p| crate::config::set_override_in(&p, &name, &cat));
                    self.entries[index].category = Some(cat);
                    (written, t.category_assigned)
                };

                // The in-memory change stands either way. A failed write is
                // worth saying out loud, but dropping the label as well would
                // make the keypress look ignored and lose the user's intent
                // along with the file.
                match written {
                    Ok(()) => self.notify(confirmation),
                    Err(e) => self.notify(format!("{} — {e:#}", t.category_save_failed)),
                }

                self.category.clear();
                self.mode = Mode::Browse;
                // Anchored, not a plain rebuild: the cursor belongs on the
                // unit just labelled, not back at row 0.
                self.rebuild_anchored(Some(name));
            }
            Message::CategoryCancel => {
                self.category.clear();
                self.mode = Mode::Browse;
            }

            Message::ExecRequest => {
                let Some(entry) = self.selected_entry() else {
                    return Command::none();
                };
                let name = entry.name.clone();
                let t = self.lang.strings();
                // Plan before asking. A question about running something that
                // turns out to have no executable is a question the user
                // cannot usefully answer.
                match crate::exec::plan(entry, &self.path_index) {
                    Ok(plan) => {
                        self.pending_exec = Some(PendingExec { name, plan });
                        self.mode = Mode::Confirm;
                    }
                    Err(Unrunnable::NotRunnable) => self.notify(t.exec_not_runnable),
                    Err(Unrunnable::Unresolved) => {
                        self.notify(format!("{} — {name}", t.exec_cannot_locate))
                    }
                }
            }
            Message::ExecCancel => {
                self.pending_exec = None;
                self.mode = Mode::Browse;
                self.notify(self.lang.strings().exec_cancelled);
            }
            Message::ExecConfirm => {
                let Some(pending) = self.pending_exec.take() else {
                    self.mode = Mode::Browse;
                    return Command::none();
                };
                self.mode = Mode::Browse;
                match pending.plan {
                    // `open` returns as soon as launchd has the bundle, so the
                    // TUI keeps the terminal. Spawned through `Command::perform`
                    // rather than here: `update` must not block the frame on a
                    // process, and the result comes back as a message so a
                    // failed launch cannot be swallowed.
                    ExecPlan::Background { program, args } => {
                        let name = pending.name;
                        return Command::perform(
                            async move { launch_background(program, args, name).await },
                            Message::ExecLaunched,
                        );
                    }
                    // Terminal programs want the tty, which the TUI is holding.
                    // Record the launch and quit: `run` performs it once the
                    // runtime has returned and `ratatui::restore()` has given
                    // the terminal back, then relaunches us. `update` stays
                    // pure — it never spawns anything itself.
                    ExecPlan::Foreground { program, args } => {
                        let handover = Suspend {
                            program,
                            args,
                            name: pending.name,
                            resume: self.resume_state(),
                        };
                        // A poisoned mutex still holds a usable slot; the only
                        // writer is this arm, once per process. Refusing to
                        // launch because another thread panicked would be a
                        // worse answer than proceeding.
                        match self.suspend.lock() {
                            Ok(mut slot) => *slot = Some(handover),
                            Err(poisoned) => *poisoned.into_inner() = Some(handover),
                        }
                        return Command::quit();
                    }
                }
            }
            Message::ExecLaunched(result) => {
                let t = self.lang.strings();
                match result {
                    Ok(name) => self.notify(format!("{} — {name}", t.exec_launched)),
                    Err(e) => self.notify(format!("{} — {e}", t.exec_launch_failed)),
                }
            }

            Message::DetailOpen => {
                // On a wide terminal the record panel is already on screen, so
                // there is nothing to open — flipping the mode would only make
                // the footer claim a state change that never happened.
                if !self.side_by_side() && self.selected_entry().is_some() {
                    self.mode = Mode::Detail;
                }
            }
            Message::DetailClose => self.mode = Mode::Browse,

            Message::RefreshStart => {
                // Ignore a second `r` while one rescan is already running —
                // two concurrent brew queries would be slower than one.
                if self.refreshing || self.scan.is_some() || self.notice.is_some() {
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
                self.notify(self.lang.strings().rescan_empty);
            }
            Message::RefreshDone(entries) => {
                let anchor = self.selected_entry().map(|e| e.name.clone());
                let count = entries.len();
                self.entries = entries;
                self.source_counts = source_counts(&self.entries);
                self.generated_at = None; // freshly scanned — this is live data
                self.refreshing = false;
                self.notify(format!("{} — {count} {}", self.lang.strings().rescan_complete, self.lang.strings().units));
                // Filter and sort are preserved; only the underlying data moved.
                self.rebuild_anchored(anchor);
            }
            Message::RefreshFailed(e) => {
                // Keep the old inventory. Stale data beats an empty screen.
                self.refreshing = false;
                self.notify(format!("{} — {e}", self.lang.strings().rescan_failed));
            }
            Message::Tick => {
                self.tick = self.tick.wrapping_add(1);
                if self.notice_until.is_some_and(|t| Instant::now() >= t) {
                    self.notice = None;
                    self.notice_until = None;
                }
            }

            Message::CacheWriteFailed(e) => {
                self.notify(format!("{} — {e}", self.lang.strings().cache_write_failed));
            }

            Message::ToggleLanguage => {
                self.lang = self.lang.toggled();
                // The notice is written in the language just switched to, so
                // it doubles as immediate confirmation the switch took effect.
                self.notify(self.lang.strings().app_subtitle.to_string());
            }

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
                    self.notify(self.lang.strings().scan_empty);
                } else {
                    // Off the UI thread: `save` serialises ~900 entries and
                    // does a synchronous write, which would stall keystrokes
                    // and the spinner on a slow or full volume. The rescan
                    // path already writes inside its async task; this is the
                    // cold-start path catching up.
                    //
                    // The result comes back as a message rather than being
                    // discarded — a silent cache failure means an unexplained
                    // 90-second launch next time.
                    let to_cache = entries.clone();
                    self.entries = entries;
                    self.source_counts = source_counts(&self.entries);
                    self.generated_at = None;
                    self.scan = None;
                    self.rebuild();
                    return Command::perform(
                        async move {
                            tokio::task::spawn_blocking(move || crate::cache::save(&to_cache))
                                .await
                                .map_err(|e| e.to_string())
                                .and_then(|r| r.map_err(|e| format!("{e:#}")))
                        },
                        |r| match r {
                            Ok(()) => Message::Tick,
                            Err(e) => Message::CacheWriteFailed(e),
                        },
                    );
                }
                self.entries = entries;
                self.source_counts = source_counts(&self.entries);
                self.generated_at = None;
                self.scan = None;
                self.rebuild();
            }

            Message::ToggleNoise => {
                let t = self.lang.strings();
                self.hide_noise = !self.hide_noise;
                self.notify(if self.hide_noise {
                    t.noise_hidden_notice
                } else {
                    t.noise_shown_notice
                });
                self.rebuild();
            }
            Message::ToggleIdleOnly => {
                let t = self.lang.strings();
                self.idle_only = !self.idle_only;
                self.notify(if self.idle_only {
                    t.idle_only_notice
                } else {
                    t.all_units_notice
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
                lang: self.lang,
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

        let show_source_cards = sourcecards::fits(area.height);
        let [head, cards, body, foot] = Layout::vertical([
            Constraint::Length(header::HEIGHT),
            Constraint::Length(if show_source_cards { sourcecards::HEIGHT } else { 0 }),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

        let head = cast_shadow(frame, head, &self.theme);
        HeaderBar {
            total: self.entries.len(),
            generated_at: self.generated_at,
            lang: self.lang,
        }
        .render(frame, head, &self.theme);

        if show_source_cards {
            SourceCards {
                counts: &self.source_counts,
            }
            .render(frame, cards, &self.theme);
        }

        // Master-detail when there is room for both; the record falls back to a
        // modal on narrow terminals rather than squeezing the grid to nothing.
        let side_by_side = self.side_by_side();
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

        let grid_area = cast_shadow(frame, grid_area, &self.theme);
        self.grid.render(
            frame,
            grid_area,
            &self.entries,
            &self.rows,
            self.stats_line(),
            self.lang,
            &self.theme,
        );

        let record = DetailPanel {
            entry: self.selected_entry(),
            lang: self.lang,
        };
        if let Some(r) = record_area {
            let r = cast_shadow(frame, r, &self.theme);
            record.render_side(frame, r, &self.theme);
        }

        // Overlays centre on the whole inset area rather than `body`: they are
        // focus traps drawn over everything, and once the source cards claim
        // their rows `body` is too short to hold a full record on a 24-row
        // terminal — the modal silently lost its trailing fields.
        match self.mode {
            // With a side panel the record is already on screen, so Enter is a
            // no-op there; on a narrow terminal it still opens the modal.
            Mode::Detail if !side_by_side => record.render_modal(frame, area, &self.theme),
            Mode::Help => HelpOverlay { lang: self.lang }.render(frame, area, &self.theme),
            _ => {}
        }

        if self.mode == Mode::Search {
            self.search
                .render(frame, foot, self.rows.len(), self.lang, &self.theme);
        } else if self.mode == Mode::Category {
            self.category.render(frame, foot, self.lang, &self.theme);
        } else {
            StatusBar {
                mode: self.mode,
                position: self.grid.selected().filter(|_| !self.rows.is_empty()),
                total: self.rows.len(),
                refreshing: self.refreshing,
                tick: self.tick,
                notice: self.notice.as_deref(),
                confirm: self.pending_exec.as_ref().map(|p| p.name.as_str()),
                lang: self.lang,
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
        if self.refreshing || self.scan.is_some() || self.notice.is_some() {
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

/// Hand a bundle to `open` and return as soon as it has been accepted.
///
/// `program` and `args` come from an `ExecPlan` and go straight to
/// `Command::new(...).args(...)` — no shell, ever, so a package name carrying
/// `;` or a newline is one argument rather than a second command.
///
/// This waits for `open` itself (which exits immediately once launchd has the
/// bundle), not for the application. Waiting is what turns "the app does not
/// exist" into a reportable failure instead of a silent no-op; not reaping it
/// would leave a zombie for every launch in a long session.
async fn launch_background(
    program: String,
    args: Vec<String>,
    name: String,
) -> Result<String, String> {
    let status = tokio::process::Command::new(&program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| format!("{name}: {e}"))?;

    if status.success() {
        Ok(name)
    } else {
        Err(format!("{name}: {program} exited {status}"))
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
/// Restore the terminal and re-raise, for signals that would otherwise kill
/// the process with raw mode and the alternate screen still in effect.
///
/// `ratatui::init()` installs a panic hook, but panics are the rare death.
/// Closing the terminal window or dropping an SSH session sends SIGHUP, and
/// `kill`/`pkill` sends SIGTERM — both left the shell with no echo, no line
/// editing and no Ctrl-C until `stty sane`. SIGKILL cannot be caught and is
/// out of scope.
///
/// The handler runs on a signal thread and only calls `ratatui::restore()`
/// before re-raising with the default disposition, so the exit status still
/// reports the signal.
fn install_signal_restore() {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
    use signal_hook::iterator::Signals;

    let Ok(mut signals) = Signals::new([SIGHUP, SIGTERM, SIGINT, SIGQUIT]) else {
        // Losing the handler is not worth refusing to start over.
        return;
    };
    std::thread::spawn(move || {
        if let Some(sig) = signals.forever().next() {
            ratatui::restore();
            // Re-raise with the default handler so the parent shell sees the
            // process die from the signal rather than exiting 0.
            unsafe {
                libc::signal(sig, libc::SIG_DFL);
                libc::raise(sig);
            }
        }
    });
}

/// Run the interface once.
///
/// Returns `Some` when the user asked to run a terminal program: the caller
/// owns performing it and relaunching, because by then the terminal has been
/// restored and this function's own state is gone.
pub async fn run(
    snapshot: Option<(Vec<AppEntry>, Option<DateTime<Local>>)>,
    resume: Option<ResumeState>,
) -> Result<Option<Suspend>> {
    // `ratatui::init` enters the alternate screen, enables raw mode, and
    // installs a panic hook that restores both — so a panic cannot leave the
    // user with a wedged terminal.
    // Built before `ratatui::init()`: everything between init and restore must
    // reach restore, and `?` here would return past it with the terminal left
    // in raw mode on the alternate screen.
    let frame_rate = FrameRate::new(NonZeroU32::new(30).expect("30 is non-zero"))
        .map_err(|e| anyhow!("invalid frame rate: {e}"))?;

    // `init` panics rather than returning when stdout is not a terminal, which
    // surfaces as a raw Rust backtrace to anyone piping the binary or running
    // it from cron.
    let mut terminal = ratatui::try_init()
        .map_err(|e| anyhow!("cannot start the interface: {e}. Is this a terminal?"))?;
    install_signal_restore();

    let suspend = SuspendSlot::default();
    let flags = Flags {
        snapshot,
        resume,
        suspend: suspend.clone(),
    };

    let result = Runtime::<App>::new(flags, frame_rate)
        .run(&mut terminal)
        .await;

    // Unconditional, and before the `?`: the terminal must come back whether
    // the runtime returned cleanly, errored, or is handing over to a child.
    ratatui::restore();
    result.map_err(|e| anyhow!("tui runtime failed: {e}"))?;

    let handover = match suspend.lock() {
        Ok(mut slot) => slot.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    };
    Ok(handover)
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
            ui_kind: None,
            category: None,
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
        let mut a = App::new(Some((entries, None)).into()).0;
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
        let mut a = App::new(Some((entries, None)).into()).0;

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
        let mut a = App::new(Some((vec![used, unused_a, unused_b], None)).into()).0;

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
        let mut a = App::new(Some((vec![recent], None)).into()).0;

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

    /// PROOF of the reported defect: widening past the side-panel threshold
    /// while the modal is open leaves the app in Detail mode, where the keymap
    /// binds almost nothing — navigation dies with no on-screen cause.
    #[test]
    fn proof_resize_strands_detail_mode() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut a = app();
        a.width = 80;
        send(&mut a, Message::DetailOpen);
        assert_eq!(a.mode, Mode::Detail);

        // User drags the window wider; the side panel takes over rendering.
        send(&mut a, Message::Terminal(Event::Resize(130, 40)));

        assert_eq!(a.mode, Mode::Browse, "mode must follow the layout");
        let j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        assert!(
            crate::tui::keymap::translate(a.mode, j).is_some(),
            "navigation must still work after widening"
        );
    }

    /// A notice must hand the footer back. Reported symptom: after pressing
    /// `L` the key hints never returned — and the same was true of `p` and
    /// `s`, because no notice had a deadline.
    #[test]
    fn notices_expire_and_give_the_footer_back() {
        for msg in [
            Message::ToggleLanguage,
            Message::ToggleNoise,
            Message::ToggleIdleOnly,
        ] {
            let mut a = app();
            send(&mut a, msg.clone());
            assert!(a.notice.is_some(), "{msg:?} should announce itself");
            assert!(
                a.notice_until.is_some(),
                "{msg:?} set a notice with no deadline — it would never clear"
            );

            // The timer must be running, or nothing will ever deliver the Tick
            // that clears it.
            assert!(
                a.subscriptions().len() > 1,
                "{msg:?}: no timer subscribed, so the notice cannot expire"
            );

            // Once the deadline passes, the next tick restores the footer.
            a.notice_until = Some(std::time::Instant::now());
            send(&mut a, Message::Tick);
            assert!(a.notice.is_none(), "{msg:?}: notice outlived its deadline");
            assert!(a.notice_until.is_none());
        }
    }

    /// An idle session must not subscribe to a timer — that is what keeps the
    /// process asleep between keystrokes.
    #[test]
    fn idle_session_subscribes_to_input_only() {
        let a = app();
        assert_eq!(a.subscriptions().len(), 1);
    }

    /// A categories file in a directory of its own, removed on drop.
    ///
    /// Committing a category writes to disk, so any test that presses Enter in
    /// `Mode::Category` would otherwise edit the developer's real
    /// `~/.config/sysapp-tui/categories.json`. The directory name is
    /// namespaced to this module for the same reason `config.rs`'s helper is:
    /// `Drop` removes the whole parent, and two helpers deriving the same path
    /// delete each other's fixtures under the parallel runner.
    struct ScratchConfig(std::path::PathBuf);

    impl ScratchConfig {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "sysapp-tui-tui-tests-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir.join("categories.json"))
        }

        fn path(&self) -> std::path::PathBuf {
            self.0.clone()
        }

        fn contents(&self) -> String {
            std::fs::read_to_string(&self.0).unwrap_or_default()
        }
    }

    impl Drop for ScratchConfig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
        }
    }

    /// Inventory with a deliberate mix: two built-ins out of order, one
    /// `Custom`, and one entry left uncategorised.
    fn categorised_app() -> App {
        let mut entries = vec![
            entry("alpha", 1, Source::Homebrew),
            entry("bravo", 1, Source::Cargo),
            entry("charlie", 1, Source::Npm),
            entry("delta", 1, Source::Npm),
        ];
        // `Media` precedes `Network` in `BUILT_IN`, but `alpha` carries the
        // later one — so an order derived from the entries rather than from
        // `BUILT_IN` would put them the wrong way round.
        entries[0].category = Some(Category::Network);
        entries[1].category = Some(Category::Media);
        entries[2].category = Some(Category::Custom("Terminal".into()));
        let mut a = App::new(Some((entries, None)).into()).0;
        a.width = 80;
        a
    }

    /// The cycle visits built-ins in `BUILT_IN` order, then `Custom` ones by
    /// name, and wraps back to no filter. Categories with no entries never
    /// appear — cycling onto an empty one is a keypress that shows nothing.
    #[test]
    fn category_cycle_visits_only_present_categories_then_wraps() {
        let mut a = categorised_app();
        assert_eq!(
            a.present_categories(),
            vec![
                Category::Media,
                Category::Network,
                Category::Custom("Terminal".into())
            ]
        );

        send(&mut a, Message::CategoryFilterCycle);
        assert_eq!(a.category_filter, Some(Category::Media));
        assert_eq!(names(&a), ["bravo"]);

        send(&mut a, Message::CategoryFilterCycle);
        assert_eq!(a.category_filter, Some(Category::Network));
        assert_eq!(names(&a), ["alpha"]);

        send(&mut a, Message::CategoryFilterCycle);
        assert_eq!(a.category_filter, Some(Category::Custom("Terminal".into())));
        assert_eq!(names(&a), ["charlie"]);

        send(&mut a, Message::CategoryFilterCycle);
        assert_eq!(a.category_filter, None, "the cycle must wrap to no filter");
        assert_eq!(names(&a), ["alpha", "bravo", "charlie", "delta"]);
    }

    /// A cached inventory scanned before categories existed has none at all.
    /// The key must say so rather than silently doing nothing or looping.
    #[test]
    fn category_cycle_on_an_uncategorised_inventory_says_so_and_stays_off() {
        let mut a = app();
        assert!(a.present_categories().is_empty());
        send(&mut a, Message::CategoryFilterCycle);
        assert_eq!(a.category_filter, None);
        assert!(a.notice.is_some(), "a dead keypress must explain itself");
    }

    /// The category filter is another term in `visible`, not a replacement for
    /// the others — search must still narrow what it returns.
    #[test]
    fn category_filter_composes_with_search() {
        let mut a = categorised_app();
        a.entries[3].category = Some(Category::Media); // delta joins bravo
        a.rebuild();

        send(&mut a, Message::CategoryFilterCycle);
        assert_eq!(a.category_filter, Some(Category::Media));
        assert_eq!(names(&a), ["bravo", "delta"]);

        send(&mut a, Message::SearchOpen);
        for c in "del".chars() {
            send(&mut a, Message::SearchPush(c));
        }
        assert_eq!(names(&a), ["delta"], "both filters must apply");
    }

    #[test]
    fn category_open_is_inert_with_nothing_selected() {
        let mut a = app();
        send(&mut a, Message::SearchOpen);
        for c in "zzzz".chars() {
            send(&mut a, Message::SearchPush(c));
        }
        assert!(a.rows.is_empty());
        send(&mut a, Message::CategoryOpen);
        assert_eq!(a.mode, Mode::Search, "no target means no edit to enter");
    }

    /// Whitespace-only input clears the override instead of storing
    /// `Custom("")`, which would render as a blank cell the user could never
    /// select and remove.
    #[test]
    fn whitespace_only_commit_never_stores_an_empty_custom_category() {
        let scratch = ScratchConfig::new("blank-commit");
        let mut a = categorised_app();
        a.config_path = Some(scratch.path());
        let target = a.selected_entry().unwrap().name.clone();

        send(&mut a, Message::CategoryOpen);
        for c in "   ".chars() {
            send(&mut a, Message::CategoryPush(c));
        }
        send(&mut a, Message::CategoryCommit);

        assert_eq!(a.mode, Mode::Browse);
        assert!(
            !a.entries
                .iter()
                .any(|e| e.category == Some(Category::Custom(String::new()))),
            "empty input must clear, never create a nameless category"
        );
        // The cleared entry falls back to whatever the classifier says, which
        // is always `Some` — leaving it `None` would render as `—` and be
        // indistinguishable from an inventory scanned before categories.
        assert!(
            a.entries
                .iter()
                .find(|e| e.name == target)
                .is_some_and(|e| e.category.is_some()),
            "clearing must restore the automatic category, not leave a hole"
        );
    }

    /// The cursor stays on the unit that was just labelled. A plain rebuild
    /// would send it back to row 0, which on a 900-row inventory means the
    /// user loses their place every time they categorise something.
    #[test]
    fn commit_keeps_the_cursor_on_the_labelled_entry() {
        let scratch = ScratchConfig::new("commit-cursor");
        let mut a = categorised_app();
        a.config_path = Some(scratch.path());
        send(&mut a, Message::Move(2));
        let before = a.selected_entry().unwrap().name.clone();

        send(&mut a, Message::CategoryOpen);
        for c in "Design".chars() {
            send(&mut a, Message::CategoryPush(c));
        }
        send(&mut a, Message::CategoryCommit);

        assert_eq!(a.selected_entry().unwrap().name, before);
        assert_eq!(a.mode, Mode::Browse);
        assert_eq!(a.category.input(), "", "the input must not survive a commit");
        assert_eq!(a.selected_entry().unwrap().category, Some(Category::Design));
        assert!(
            scratch.contents().contains(&before) && scratch.contents().contains("Design"),
            "the assignment must reach disk, or it dies with the session: {}",
            scratch.contents()
        );
    }

    /// A write that fails must not also swallow the label. The user pressed a
    /// key and named a category; showing the name while reporting the failure
    /// keeps their intent on screen and lets them retry. Dropping both would
    /// look exactly like the keypress being ignored.
    #[test]
    fn a_failed_write_still_applies_in_memory_and_says_so() {
        let scratch = ScratchConfig::new("write-fails");
        // A regular file where the config directory should be: `save_to`'s
        // `create_dir_all` cannot succeed against it.
        let blocked = scratch.path().join("categories.json");
        std::fs::write(scratch.path(), b"not a directory").unwrap();

        let mut a = categorised_app();
        a.config_path = Some(blocked);
        send(&mut a, Message::Move(1));
        let target = a.selected_entry().unwrap().name.clone();

        send(&mut a, Message::CategoryOpen);
        for c in "Gaming".chars() {
            send(&mut a, Message::CategoryPush(c));
        }
        send(&mut a, Message::CategoryCommit);

        assert_eq!(
            a.entries
                .iter()
                .find(|e| e.name == target)
                .and_then(|e| e.category.clone()),
            Some(Category::Gaming),
            "the label must survive a failed write"
        );
        // Read the expected text off the app's own language rather than
        // pinning one: `App::new` calls `Lang::detect()`, so hardcoding either
        // language makes the test pass or fail on the runner's locale.
        let notice = a.notice.as_deref().unwrap_or_default();
        assert!(
            notice.contains(a.lang.strings().category_save_failed),
            "a lost write must be reported, got {notice:?}"
        );
    }

    /// Esc leaves the entry exactly as it was.
    #[test]
    fn category_cancel_changes_nothing() {
        let mut a = categorised_app();
        let before: Vec<Option<Category>> =
            a.entries.iter().map(|e| e.category.clone()).collect();

        send(&mut a, Message::CategoryOpen);
        for c in "Gaming".chars() {
            send(&mut a, Message::CategoryPush(c));
        }
        send(&mut a, Message::CategoryCancel);

        assert_eq!(a.mode, Mode::Browse);
        assert_eq!(a.category.input(), "");
        let after: Vec<Option<Category>> =
            a.entries.iter().map(|e| e.category.clone()).collect();
        assert_eq!(after, before);
    }

    fn runnable_app(kind: crate::model::UiKind, path: Option<&str>) -> App {
        let mut e = entry("target", 0, Source::Applications);
        e.ui_kind = Some(kind);
        e.path = path.map(str::to_string);
        let mut a = App::new(Some((vec![e], None)).into()).0;
        a.width = 80;
        a
    }

    /// Enter never runs anything by itself. It plans, then asks — and the
    /// question is a mode, so the grid keys stop working until it is answered.
    #[test]
    fn enter_asks_before_running_and_traps_focus() {
        use crate::model::UiKind;
        let mut a = runnable_app(UiKind::Gui, Some("/Applications/Zed.app"));

        send(&mut a, Message::ExecRequest);
        assert_eq!(a.mode, Mode::Confirm);
        assert!(a.pending_exec.is_some(), "the plan must be built before asking");

        // Every key that is not `y` cancels — including ones that navigate
        // everywhere else, which is the whole point of the trap.
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        for ch in ['n', 'j', 'q'] {
            let mut a = runnable_app(UiKind::Gui, Some("/Applications/Zed.app"));
            send(&mut a, Message::ExecRequest);
            send(
                &mut a,
                Message::Terminal(Event::Key(KeyEvent::new(
                    KeyCode::Char(ch),
                    KeyModifiers::NONE,
                ))),
            );
            assert_eq!(a.mode, Mode::Browse, "{ch:?} must cancel");
            assert!(a.pending_exec.is_none(), "{ch:?} left a plan armed");
        }
    }

    /// A suspend has to come back to the list the user left, not to row 0 of
    /// an unfiltered English list — that reads as having lost their place
    /// rather than as having run something.
    ///
    /// Exercised as a real round trip through `Flags` rather than by calling
    /// `restore` directly, because the ordering is the fragile part: filters
    /// have to be reinstated before the rows are built, or the cursor anchor
    /// is searched for in a list that has not been filtered yet.
    #[test]
    fn a_suspend_returns_to_the_same_view() {
        let fixture = || {
            vec![
                entry("charlie", 5, Source::Npm),
                entry("alpha", 100, Source::Homebrew),
                entry("bravo", 50, Source::Cargo),
            ]
        };

        let mut before = App::new(Some((fixture(), None)).into()).0;
        send(&mut before, Message::ToggleNoise);
        send(&mut before, Message::SortBy(Column::Usage));
        send(&mut before, Message::SearchOpen);
        for c in "a".chars() {
            send(&mut before, Message::SearchPush(c));
        }
        send(&mut before, Message::SearchCommit);
        before.lang = Lang::En;

        let anchor = before.selected_entry().map(|e| e.name.clone());
        assert!(anchor.is_some(), "the fixture must leave something selected");

        let after = App::new(Flags {
            snapshot: Some((fixture(), None)),
            resume: Some(before.resume_state()),
            ..Flags::default()
        })
        .0;

        assert_eq!(after.search.query(), "a");
        assert_eq!(after.grid.sort_col, Column::Usage);
        assert_eq!(after.grid.sort_asc, before.grid.sort_asc);
        assert_eq!(after.hide_noise, before.hide_noise);
        assert_eq!(after.lang, Lang::En);
        assert_eq!(after.rows.len(), before.rows.len());
        assert_eq!(
            after.selected_entry().map(|e| e.name.clone()),
            anchor,
            "the cursor must come back to the same unit, not to row 0"
        );
    }

    /// The handover is what crosses the process boundary, so it has to carry
    /// the argv verbatim. A package name is attacker-adjacent data — it comes
    /// from `brew`, from filenames, from package registries — and it must be
    /// one opaque argv entry rather than anything a shell would re-read.
    #[test]
    fn a_handover_never_reinterprets_the_name() {
        use crate::model::UiKind;

        let mut a = runnable_app(UiKind::Gui, Some("/Applications/Zed.app; rm -rf ~"));
        send(&mut a, Message::ExecRequest);
        let plan = &a.pending_exec.as_ref().expect("plan armed").plan;
        match plan {
            ExecPlan::Background { program, args } => {
                assert_eq!(program, "open");
                assert_eq!(args, &vec!["/Applications/Zed.app; rm -rf ~".to_string()]);
            }
            ExecPlan::Foreground { .. } => panic!("a GUI unit must not take the tty"),
        }
    }

    /// Libraries, daemons and unclassified units are not runnable. Enter says
    /// so and arms nothing — a wrong guess here launches the wrong program.
    #[test]
    fn enter_refuses_things_that_are_not_programs() {
        use crate::model::UiKind;
        for kind in [UiKind::Library, UiKind::Service, UiKind::Unknown] {
            let mut a = runnable_app(kind, None);
            send(&mut a, Message::ExecRequest);
            assert_eq!(a.mode, Mode::Browse, "{kind:?} must not open a prompt");
            assert!(a.pending_exec.is_none());
            assert!(a.notice.is_some(), "{kind:?} refusal must be explained");
        }
    }

    /// A CLI whose binary cannot be found is reported rather than guessed at.
    #[test]
    fn enter_reports_an_unresolvable_binary() {
        use crate::model::UiKind;
        let mut a = runnable_app(UiKind::Cli, None);
        a.entries[0].name = "definitely-not-on-this-machine-xyzzy".into();
        a.path_index = PathIndex::empty();
        send(&mut a, Message::ExecRequest);

        assert_eq!(a.mode, Mode::Browse);
        assert!(a.pending_exec.is_none());
        assert!(
            a.notice
                .as_deref()
                .unwrap_or_default()
                .contains(a.lang.strings().exec_cannot_locate)
        );
    }

    /// Confirming with nothing armed must not panic — the mode and the plan
    /// are set together, but a message can always arrive out of order.
    #[test]
    fn confirming_with_nothing_armed_is_inert() {
        let mut a = app();
        a.mode = Mode::Confirm;
        send(&mut a, Message::ExecConfirm);
        assert_eq!(a.mode, Mode::Browse);
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
            ui_kind: None,
            category: None,
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

    /// Render at `w`x`h`, pinning everything the model reads from the
    /// environment.
    ///
    /// Two things leak in and both have now broken CI: `App::width` is seeded
    /// from `crossterm::terminal::size()`, and `Theme::detect()` reads
    /// COLORTERM/TERM/NO_COLOR — a headless runner gets the 16-colour palette,
    /// so any assertion about a specific colour passes locally and fails
    /// there. Pinning both here covers every render test at once rather than
    /// waiting for each to be caught individually.
    fn draw(app: &mut App, w: u16, h: u16) -> String {
        app.width = w;
        app.theme = Theme::tactical();
        app.lang = Lang::En;
        draw_at(app, w, h)
    }

    fn draw_at(app: &App, w: u16, h: u16) -> String {
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
        let mut app = App::new(Some((sample(), None)).into()).0;
        let out = draw(&mut app, 120, 24);
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
        let mut app = App::new(Some((sample(), None)).into()).0;
        let _ = app.update(Message::SortBy(Column::Usage));
        let out = draw(&mut app, 120, 24);
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
        let mut app = App::new(Some((sample(), None)).into()).0;
        let _ = app.update(Message::SearchOpen);
        for c in "rust".chars() {
            let _ = app.update(Message::SearchPush(c));
        }
        let out = draw(&mut app, 120, 24);
        banner("SEARCH \"rust\" 120x24", &out);
        assert!(out.contains("SEARCH"), "search band missing");
        assert!(out.contains("MATCH"), "hit count missing");
        assert!(!out.contains("httpie"), "python entry should be filtered out");
    }

    /// Wide terminals carry the record as a persistent side panel — no key
    /// press required, and Enter must not replace it with a modal.
    #[test]
    fn render_record_side_panel_120x24() {
        let mut app = App::new(Some((sample(), None)).into()).0;
        app.width = 120;
        let out = draw(&mut app, 120, 24);
        banner("RECORD SIDE PANEL 120x24", &out);
        assert!(out.contains("RECORD"), "record panel title missing");
        assert!(out.contains("INVOCATIONS"), "record fields missing");
        assert!(out.contains("INTERFACE"), "interface field missing");
        assert!(out.contains("CATEGORY"), "category field missing");
        // Per-source totals left the record entirely: they are inventory-wide
        // context, so they belong to the card row under the masthead. The row
        // is optional chrome and only appears once the terminal is tall enough.
        assert!(!out.contains("SOURCES"), "source breakdown must leave the record");
        let tall = draw(&mut app, 120, 34);
        assert!(tall.contains("BREW"), "source cards missing on a tall terminal");

        let _ = app.update(Message::DetailOpen);
        let after = draw(&mut app, 120, 24);
        assert_eq!(after, out, "Enter must be inert while the side panel is up");
    }

    /// Narrow terminals drop the side panel; Enter still opens the modal.
    #[test]
    fn render_record_modal_when_narrow() {
        let mut app = App::new(Some((sample(), None)).into()).0;
        app.width = 80;
        let plain = draw(&mut app, 80, 24);
        assert!(!plain.contains("INVOCATIONS"), "no side panel at 80 cols");

        let _ = app.update(Message::DetailOpen);
        let out = draw(&mut app, 80, 24);
        banner("RECORD MODAL 80x24", &out);
        assert!(out.contains("RECORD"), "modal title missing");
        assert!(out.contains("INVOCATIONS"), "modal fields missing");
    }

    #[test]
    fn render_help_120x30() {
        let mut app = App::new(Some((sample(), None)).into()).0;
        let _ = app.update(Message::HelpToggle);
        let out = draw(&mut app, 120, 30);
        banner("HELP OVERLAY 120x30", &out);
        assert!(out.contains("KEY REFERENCE"), "help title missing");
    }

    #[test]
    fn render_narrow_80x24() {
        let mut app = App::new(Some((sample(), None)).into()).0;
        let out = draw(&mut app, 80, 24);
        banner("BROWSE 80x24", &out);
        assert!(out.contains("SYSAPP"), "must still render at the 80x24 floor");
    }

    /// Below the floor we must say so, not draw a mangled layout.
    #[test]
    fn render_undersized_40x8() {
        let mut app = App::new(Some((sample(), None)).into()).0;
        let out = draw(&mut app, 40, 8);
        banner("UNDERSIZED 40x8", &out);
        assert!(out.contains("VIEWPORT UNDERSIZED"));
    }

    /// Hostile names must not break the layout or reach the terminal as
    /// control sequences. Package names are attacker-influenced in principle —
    /// anyone can publish a cask, gem or npm package.
    #[test]
    fn hostile_names_do_not_corrupt_the_frame() {
        let mk = |name: &str| AppEntry {
            name: name.into(),
            version: Some("1.0".into()),
            source: Source::Homebrew,
            language: Some(Language::Rust),
            install_date: None,
            last_used: None,
            usage_count: 5,
            path: Some("/tmp/x".into()),
            description: None,
            ui_kind: None,
            category: None,
        };
        let hostile = vec![
            mk("\u{1b}[31mRED\u{1b}[0m"),
            mk("\u{1b}[2J\u{1b}[H cleared"),
            mk("\u{1b}]0;PWNED\u{7} title"),
            mk("bell\u{7}here"),
            mk("tab\there"),
            mk("line\nbreak"),
            mk("日本語のとても長いアプリケーション名前です"),
            mk("🎉👨‍👩‍👧‍👦🇹🇼"),
            mk("\u{202e}RTL-override"),
            mk("e\u{301}\u{301}combining"),
            mk(&"A".repeat(10_000)),
            mk(""),
        ];
        let mut app = App::new(Some((hostile, None)).into()).0;
        let out = draw(&mut app, 130, 24);
        banner("HOSTILE NAMES 130x24", &out);

        // No control character may survive into the buffer.
        for (i, line) in out.lines().enumerate() {
            assert!(
                !line.chars().any(|c| c.is_control() && c != '\n'),
                "line {i} carries a control char: {line:?}"
            );
        }
        // Border alignment must be checked against buffer coordinates, not the
        // stringified frame: one cell can hold a multi-char grapheme (a ZWJ
        // emoji) or be a wide-glyph continuation, so neither `chars().count()`
        // nor display width maps back to a column.
        let mut term =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(130, 24)).unwrap();
        term.draw(|f| app.view(f)).unwrap();
        let buf = term.backend().buffer().clone();

        let border_cols = |y: u16| -> Vec<u16> {
            (0..buf.area.width)
                .filter(|&x| buf[(x, y)].symbol() == "│")
                .collect()
        };
        // Locate the panel body rows rather than hardcoding indices — the
        // masthead's height is a layout constant that has already changed once.
        let body_rows: Vec<u16> = (0..buf.area.height)
            .filter(|&y| border_cols(y).len() >= 4)
            .collect();
        assert!(
            body_rows.len() > 8,
            "expected a tall two-panel body, found {} rows with borders",
            body_rows.len()
        );
        let reference = border_cols(body_rows[0]);
        for &y in &body_rows {
            assert_eq!(
                border_cols(y),
                reference,
                "row {y} borders moved — a glyph width miscount shifted the layout"
            );
        }
    }

    /// The masthead is a filled field: deep red, white text, with blank rows
    /// above and below the title. Every cell of it must be painted, padding
    /// rows included — an unpainted row would show the terminal background as
    /// a gap straight through the bar.
    #[test]
    fn masthead_is_a_filled_field() {
        use ratatui::style::Color;
        let mut app = App::new(Some((sample(), None)).into()).0;
        app.width = 130;
        app.theme = Theme::tactical();
        let mut term =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(130, 30)).unwrap();
        term.draw(|f| app.view(f)).unwrap();
        let buf = term.backend().buffer().clone();

        // Read from the theme rather than repeating the literal, so retuning
        // the palette does not silently invalidate the test.
        let band = app.theme.band;
        // The band is inset by the outer margin and gives one further column
        // to its shadow, so it spans neither the full frame nor the full
        // content width.
        let inner = MARGIN_X..(buf.area.width - MARGIN_X - 1);
        let band_rows: Vec<u16> = (0..buf.area.height)
            .filter(|&y| inner.clone().all(|x| buf[(x, y)].bg == band))
            .collect();
        assert_eq!(
            band_rows.len(),
            3,
            "expected 1 padding + 1 title + 1 padding fully filled rows, got {band_rows:?}"
        );

        // The title sits on the middle row.
        let title_row = band_rows[1];
        let text: String = (0..buf.area.width)
            .map(|x| buf[(x, title_row)].symbol())
            .collect();
        assert!(text.contains("SYSAPP"), "title row is {text:?}");
        let title_x = text.find("SYSAPP").unwrap() as u16;
        assert_eq!(
            buf[(title_x, title_row)].fg,
            app.theme.fg_on_band,
            "title must use the band's foreground"
        );
        assert_eq!(
            app.theme.fg_on_band,
            Color::Rgb(0xFF, 0xFF, 0xFF),
            "the band foreground is white"
        );
        assert_eq!(app.theme.band, Color::Rgb(0x6E, 0x0D, 0x10), "deep red field");

        // The card casts a shadow: the column just right of the band, one row
        // down, carries the shadow colour rather than the substrate.
        let shadow_x = buf.area.width - MARGIN_X - 1;
        assert_eq!(
            buf[(shadow_x, band_rows[1])].bg,
            app.theme.shadow,
            "the masthead must cast a shadow on its right edge"
        );
        // Padding rows carry the field but no text.
        for &y in [band_rows[0], band_rows[2]].iter() {
            let row: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            assert!(row.trim().is_empty(), "padding row {y} is not blank: {row:?}");
        }
    }

    #[test]
    fn render_cold_start_scan_screen() {
        // `None` flags = no cache, so the app opens on the progress screen.
        let (mut app, _cmd) = App::new(Flags::default());
        let out = draw(&mut app, 120, 24);
        banner("COLD START — ALL PENDING", &out);
        assert!(out.contains("NO SNAPSHOT"), "must explain why it is scanning");
        assert!(out.contains("BREW"), "sources must be listed");
        assert!(out.contains("slowest source"), "brew must be flagged as slow");

        // Partial progress: brew done, one source unavailable.
        let _ = app.update(Message::SourceScanned("brew", Ok(vec![])));
        let _ = app.update(Message::SourceScanned("go", Err("go not installed".into())));
        let out = draw(&mut app, 120, 24);
        banner("COLD START — PARTIAL", &out);
        assert!(out.contains("0 UNITS"), "completed source shows its count");
        assert!(out.contains("SKIPPED"), "unavailable source is marked, not fatal");
    }

    /// A source that fails must not abort the scan or lose the others.
    #[test]
    fn failed_source_does_not_abort_the_scan() {
        let (mut app, _) = App::new(Flags::default());
        for name in crate::scanner::SOURCES {
            let _ = app.update(Message::SourceScanned(name, Err("boom".into())));
        }
        // All reported (all failed) → enrichment still runs and completes.
        let _ = app.update(Message::EnrichDone(vec![]));
        assert!(app.scan.is_none(), "scan screen must clear even if every source failed");
    }

    /// Every visible header must render INTACT at every tier. Overflow makes
    /// ratatui clip the rightmost headers silently rather than reporting
    /// anything — which shipped twice as `5·INSTALLE`. Expectations come from
    /// `visible_columns` rather than a hardcoded list, so a tier that quietly
    /// widens is caught by the absence half of the assertion instead of
    /// sailing through a stale literal.
    #[test]
    fn every_tier_renders_its_headers_intact() {
        use table::visible_columns;

        // One width per tier, plus the side-by-side floor — where the record
        // panel takes 36 columns back and drops the grid to the compact set,
        // which is the case the old hand-computed budget was written for.
        // Without the 90 the middle tier is never rendered by any test: it
        // needs the record panel gone and still ~81 columns of grid.
        let mut tiers_seen: Vec<usize> = Vec::new();

        for width in [80u16, 90, SIDE_PANEL_MIN_WIDTH, 160] {
            let mut app = App::new(Some((sample(), None)).into()).0;
            app.width = width;
            let out = draw(&mut app, width, 26);
            banner(&format!("HEADERS AT {width} COLS"), &out);

            // The grid's content width is not reachable from here, so derive the
            // visible set from whichever tier actually rendered: exactly one of
            // the three must match the headers on screen.
            let shown: Vec<Column> = Column::ALL
                .iter()
                .copied()
                .filter(|col| {
                    let logical = Column::ALL.iter().position(|c| c == col).unwrap() + 1;
                    out.contains(&format!("{logical}·{}", col.label(Lang::En)))
                })
                .collect();

            let expected = [
                visible_columns(0),
                visible_columns(table::MEDIUM_MIN_CONTENT_WIDTH),
                visible_columns(table::FULL_MIN_CONTENT_WIDTH),
            ]
            .into_iter()
            .find(|tier| {
                let mut a: Vec<Column> = tier.to_vec();
                let mut b = shown.clone();
                a.sort_by_key(|c| Column::ALL.iter().position(|x| x == c));
                b.sort_by_key(|c| Column::ALL.iter().position(|x| x == c));
                a == b
            });

            let expected = expected.unwrap_or_else(|| {
                panic!(
                    "at {width} cols the headers on screen ({shown:?}) match no tier — \
                     a label was clipped or a column leaked"
                )
            });

            // Absence is half the guard: anything outside this tier must not be
            // on screen, or a tier has quietly widened past its budget.
            for col in Column::ALL.iter().filter(|c| !expected.contains(c)) {
                let logical = Column::ALL.iter().position(|c| c == col).unwrap() + 1;
                assert!(
                    !out.contains(&format!("{logical}·{}", col.label(Lang::En))),
                    "{col:?} leaked into the {expected:?} tier at {width} cols"
                );
            }
            tiers_seen.push(expected.len());
        }

        // All three tiers must actually have rendered, or this test would keep
        // passing after a change that collapsed them into one.
        tiers_seen.sort_unstable();
        tiers_seen.dedup();
        assert_eq!(tiers_seen.len(), 3, "widths exercised only {tiers_seen:?}");
    }

    /// Every frame must survive an empty result set without panicking.
    #[test]
    fn render_no_matches() {
        let mut app = App::new(Some((sample(), None)).into()).0;
        let _ = app.update(Message::SearchOpen);
        for c in "zzzz".chars() {
            let _ = app.update(Message::SearchPush(c));
        }
        let out = draw(&mut app, 120, 24);
        banner("NO MATCHES 120x24", &out);
        assert!(out.contains("NO UNITS MATCH FILTER"));
    }
}

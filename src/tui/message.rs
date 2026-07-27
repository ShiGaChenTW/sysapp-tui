//! The TEA vocabulary: every state transition this application can undergo.
//!
//! Nothing here touches the terminal. `update` is a pure function of
//! `(state, Message)`, which is what makes the whole interface testable
//! without a tty.

use crossterm::event::Event;

use crate::model::AppEntry;
use crate::tui::i18n::Lang;

/// Which input context owns the keyboard right now.
///
/// Exactly one mode is active at a time; overlays are focus traps
/// (tui-design §3, focus management).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Browse,
    Search,
    Category,
    /// Waiting for `y` before running something. A real mode rather than a
    /// flag on the footer: running a program is irreversible, so the keyboard
    /// has to belong entirely to the question until it is answered.
    Confirm,
    Detail,
    Help,
}

impl Mode {
    /// Shown verbatim in the status band. Modal confusion is anti-pattern #9;
    /// the current mode is always on screen.
    pub fn label(self, lang: Lang) -> &'static str {
        let t = lang.strings();
        match self {
            Self::Browse => t.mode_browse,
            Self::Search => t.mode_search,
            Self::Category => t.mode_category,
            Self::Confirm => t.mode_confirm,
            Self::Detail => t.mode_detail,
            Self::Help => t.mode_help,
        }
    }
}

/// Sortable columns of the data grid.
///
/// Exactly nine, because `keymap` binds digits '1'..='9' to `from_index` and a
/// tenth column would be unreachable from the keyboard. PATH lost its slot to
/// CATEGORY: it is never drawn in the grid (it is long, always elided, and
/// shown in full in the record panel), and a sort key for a column the user
/// cannot see carries no meaning the ▲/▼ marker can express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Source,
    Lang,
    Version,
    Installed,
    Usage,
    Category,
    UiKind,
    LastUsed,
}

impl Column {
    pub const ALL: [Column; 9] = [
        Column::Name,
        Column::Source,
        Column::Lang,
        Column::Version,
        Column::Installed,
        Column::Usage,
        Column::Category,
        Column::UiKind,
        Column::LastUsed,
    ];

    pub fn label(self, lang: Lang) -> &'static str {
        let t = lang.strings();
        match self {
            Self::Name => t.col_name,
            Self::Source => t.col_source,
            Self::Lang => t.col_lang,
            Self::Version => t.col_version,
            Self::Installed => t.col_installed,
            Self::Usage => t.col_usage,
            Self::Category => t.col_category,
            Self::UiKind => t.col_ui_kind,
            Self::LastUsed => t.col_last_used,
        }
    }

    pub fn from_index(i: usize) -> Option<Self> {
        Self::ALL.get(i).copied()
    }

    /// Direction to use the first time a column is selected.
    ///
    /// For counts and dates the interesting end is the large one — nobody
    /// sorts by usage to find their least-used tool. Comparison itself stays
    /// natural ascending for every column so the ▲/▼ indicator never lies;
    /// only the starting direction differs.
    pub fn default_ascending(self) -> bool {
        !matches!(self, Self::Usage | Self::Installed | Self::LastUsed)
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Raw terminal event, routed through the keymap by `update`.
    Terminal(Event),
    TerminalError(String),

    /// Move the cursor by a signed row delta.
    Move(i32),
    JumpTop,
    JumpBottom,

    /// Sort by a column; repeating the same column flips direction.
    SortBy(Column),

    SearchOpen,
    SearchPush(char),
    SearchPop,
    SearchCommit,
    SearchCancel,

    /// Step the category filter to the next category that actually has
    /// entries, wrapping back to "no filter". Empty categories are skipped
    /// because most of the eleven built-ins are unused on any real machine.
    CategoryFilterCycle,
    /// Begin labelling the selected entry. Inert with nothing selected: the
    /// input has no target and could never commit.
    CategoryOpen,
    /// A character typed into the category input. Unlike `SearchPush` this
    /// does not refilter — the text names a category to write, not a query.
    CategoryPush(char),
    CategoryPop,
    /// Persist the typed name as this entry's category. Empty input clears the
    /// override instead, so a blank name can never become `Custom("")`.
    CategoryCommit,
    CategoryCancel,

    /// Enter on a row: plan the launch and ask before doing anything.
    ExecRequest,
    /// `y` was pressed. This is the only key that runs anything.
    ExecConfirm,
    /// Any other key while the question is up. Cancelling is the default so a
    /// keystroke aimed at the grid cannot launch a program.
    ExecCancel,
    /// A background (`open`) launch finished; carries the failure text, if any.
    ExecLaunched(Result<String, String>),

    DetailOpen,
    DetailClose,
    HelpToggle,

    /// Begin a background rescan. The UI stays fully interactive throughout.
    RefreshStart,
    /// A rescan finished; carries the fresh inventory.
    RefreshDone(Vec<AppEntry>),
    RefreshFailed(String),
    /// Spinner animation frame — only subscribed to while a rescan is running.
    Tick,

    /// One source finished during a cold scan: `(source name, result)`.
    SourceScanned(&'static str, Result<Vec<AppEntry>, String>),
    /// All sources reported; enrichment produced the final inventory.
    EnrichDone(Vec<AppEntry>),

    /// Show/hide system items (pkgutil receipts, `/System/` apps).
    ToggleNoise,
    /// Show only units with no evidence of use.
    ToggleIdleOnly,
    /// Switch between Traditional Chinese and English.
    ToggleLanguage,
    /// The background cache write failed; the data on screen is fine, only
    /// the next launch pays for it.
    CacheWriteFailed(String),

    Quit,
}

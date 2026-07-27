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
            Self::Detail => t.mode_detail,
            Self::Help => t.mode_help,
        }
    }
}

/// Sortable columns of the data grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Source,
    Lang,
    Version,
    Installed,
    Usage,
    Path,
}

impl Column {
    pub const ALL: [Column; 7] = [
        Column::Name,
        Column::Source,
        Column::Lang,
        Column::Version,
        Column::Installed,
        Column::Usage,
        Column::Path,
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
            Self::Path => t.col_path,
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
        !matches!(self, Self::Usage | Self::Installed)
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

    /// Show/hide packaging noise (pkgutil receipts, `/System/` apps).
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

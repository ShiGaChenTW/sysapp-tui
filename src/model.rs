use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    Homebrew,
    HomebrewCask,
    Npm,
    Pip,
    Cargo,
    Go,
    Gem,
    Pkgutil,
    Applications,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Homebrew => write!(f, "brew"),
            Self::HomebrewCask => write!(f, "cask"),
            Self::Npm => write!(f, "npm"),
            Self::Pip => write!(f, "pip"),
            Self::Cargo => write!(f, "cargo"),
            Self::Go => write!(f, "go"),
            Self::Gem => write!(f, "gem"),
            Self::Pkgutil => write!(f, "pkgutil"),
            Self::Applications => write!(f, "app"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Rust,
    Go,
    Python,
    JavaScript,
    Ruby,
    C,
    #[allow(dead_code)]
    Cpp,
    Swift,
    ObjC,
    Java,
    Shell,
    Electron,
    Unknown,
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rust => write!(f, "Rust"),
            Self::Go => write!(f, "Go"),
            Self::Python => write!(f, "Python"),
            Self::JavaScript => write!(f, "JavaScript"),
            Self::Ruby => write!(f, "Ruby"),
            Self::C => write!(f, "C"),
            Self::Cpp => write!(f, "C++"),
            Self::Swift => write!(f, "Swift"),
            Self::ObjC => write!(f, "Obj-C"),
            Self::Java => write!(f, "Java"),
            Self::Shell => write!(f, "Shell"),
            Self::Electron => write!(f, "Electron"),
            Self::Unknown => write!(f, "—"),
        }
    }
}

/// How the unit is driven: a window, a terminal full-screen interface, a
/// one-shot command, a background daemon, or nothing runnable at all.
///
/// This is what decides whether Enter can launch something in the background
/// (`Gui`) or has to hand the terminal over to it (`Tui`, `Cli`), so it is a
/// behavioural classification, not a cosmetic label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiKind {
    Gui,
    Tui,
    Cli,
    Service,
    Library,
    Unknown,
}

impl fmt::Display for UiKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gui => write!(f, "GUI"),
            Self::Tui => write!(f, "TUI"),
            Self::Cli => write!(f, "CLI"),
            Self::Service => write!(f, "SVC"),
            Self::Library => write!(f, "LIB"),
            Self::Unknown => write!(f, "—"),
        }
    }
}

/// What the unit is *for*.
///
/// The built-in set is closed so it can be translated and ordered; `Custom`
/// carries whatever name the user assigned, which is never translated because
/// only they know what it means.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    Development,
    Media,
    Productivity,
    System,
    Network,
    Security,
    Design,
    Data,
    Communication,
    Gaming,
    Uncategorized,
    Custom(String),
}

impl Category {
    /// Built-ins in display order. `Custom` values are appended after these,
    /// sorted by name, wherever a full list is needed.
    pub const BUILT_IN: [Category; 11] = [
        Category::Development,
        Category::Media,
        Category::Productivity,
        Category::System,
        Category::Network,
        Category::Security,
        Category::Design,
        Category::Data,
        Category::Communication,
        Category::Gaming,
        Category::Uncategorized,
    ];

    /// Parse a user-supplied name. Anything that is not a built-in becomes a
    /// `Custom` category rather than an error — the user is allowed to invent
    /// names, and rejecting them would make the feature useless.
    pub fn parse(s: &str) -> Self {
        let key = s.trim();
        Self::BUILT_IN
            .iter()
            .find(|c| c.key().eq_ignore_ascii_case(key))
            .cloned()
            .unwrap_or_else(|| Self::Custom(key.to_string()))
    }

    /// Stable, untranslated identifier — what gets written to the config file.
    /// Display text is looked up per language in `tui::i18n`.
    pub fn key(&self) -> &str {
        match self {
            Self::Development => "Development",
            Self::Media => "Media",
            Self::Productivity => "Productivity",
            Self::System => "System",
            Self::Network => "Network",
            Self::Security => "Security",
            Self::Design => "Design",
            Self::Data => "Data",
            Self::Communication => "Communication",
            Self::Gaming => "Gaming",
            Self::Uncategorized => "Uncategorized",
            Self::Custom(name) => name,
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.key())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEntry {
    pub name: String,
    pub version: Option<String>,
    pub source: Source,
    pub language: Option<Language>,
    pub install_date: Option<DateTime<Local>>,
    pub last_used: Option<DateTime<Local>>,
    pub usage_count: u32,
    pub path: Option<String>,
    #[allow(dead_code)]
    pub description: Option<String>,
    /// How it is driven. `None` until the interface enricher has run.
    #[serde(default)]
    pub ui_kind: Option<UiKind>,
    /// What it is for. `None` until the category enricher has run.
    #[serde(default)]
    pub category: Option<Category>,
}

impl AppEntry {
    /// Packaging artefacts and OS-bundled components rather than software the
    /// user chose to install.
    ///
    /// `pkgutil` reports Apple's installer receipts — reverse-DNS package ids
    /// with no version, language or usage data. On this machine that is 115 of
    /// 906 entries: noise that dilutes every sort and every search. Apps under
    /// `/System/` are the GUI equivalent.
    pub fn is_system_noise(&self) -> bool {
        matches!(self.source, Source::Pkgutil)
            || self
                .path
                .as_deref()
                .is_some_and(|p| p.starts_with("/System/"))
    }

    /// No evidence this has ever been used, or not since `cutoff`.
    ///
    /// Both conditions are required because the two data sources are
    /// asymmetric: `usage_count` comes from shell history and carries no
    /// timestamp, while `last_used` comes from Spotlight and exists only for
    /// GUI apps. An entry is idle when neither source shows recent life.
    pub fn is_idle(&self, cutoff: DateTime<Local>) -> bool {
        self.usage_count == 0 && self.last_used.is_none_or(|used| used < cutoff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn entry(source: Source, usage: u32, path: Option<&str>) -> AppEntry {
        AppEntry {
            name: "x".into(),
            version: None,
            source,
            language: None,
            install_date: None,
            last_used: None,
            usage_count: usage,
            path: path.map(str::to_string),
            description: None,
            ui_kind: None,
            category: None,
        }
    }

    #[test]
    fn pkgutil_and_system_apps_are_noise() {
        assert!(entry(Source::Pkgutil, 0, None).is_system_noise());
        assert!(
            entry(Source::Applications, 0, Some("/System/Applications/Music.app"))
                .is_system_noise()
        );
        assert!(!entry(Source::Homebrew, 0, Some("/opt/homebrew/bin/rg")).is_system_noise());
        assert!(!entry(Source::Applications, 0, Some("/Applications/Zed.app")).is_system_noise());
    }

    #[test]
    fn idle_requires_no_invocations_and_no_recent_use() {
        let cutoff = Local::now() - Duration::days(180);

        // Never invoked, never opened → idle.
        assert!(entry(Source::Homebrew, 0, None).is_idle(cutoff));

        // Invoked at all → not idle, regardless of timestamps.
        assert!(!entry(Source::Homebrew, 3, None).is_idle(cutoff));

        // Opened recently → not idle.
        let mut recent = entry(Source::Applications, 0, None);
        recent.last_used = Some(Local::now() - Duration::days(7));
        assert!(!recent.is_idle(cutoff));

        // Opened, but long ago → idle.
        let mut stale = entry(Source::Applications, 0, None);
        stale.last_used = Some(Local::now() - Duration::days(400));
        assert!(stale.is_idle(cutoff));
    }
}

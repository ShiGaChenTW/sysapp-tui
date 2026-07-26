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
}

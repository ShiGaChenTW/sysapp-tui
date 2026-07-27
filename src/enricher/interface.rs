use std::collections::HashMap;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::model::{AppEntry, Source, UiKind};

/// Curated canonical formula names — the PRIMARY signal.
///
/// Measured against the 8,517-formula homebrew-core corpus, keyword matching
/// alone recovers 15 of 64 known TUIs (23%), and the misses are the popular
/// ones: `fzf` (586,715 installs/yr) describes itself as a "Command-line fuzzy
/// finder", `glow` as "Render markdown on the CLI". Descriptions systematically
/// say "command-line" for programs that are plainly full-screen, so the name
/// list leads and keywords only see names it does not already cover.
///
/// These are `brew info --json` canonical names. Aliases and installed binary
/// names can never appear in scanned data, which silently cost this list four
/// entries: `mc`, `btm`, `nvim` and `hx` are here as `midnight-commander`,
/// `bottom`, `neovim` and `helix`. `known_tuis_holds_no_aliases` guards that.
///
/// `cmatrix`, `asciiquarium`, `pipes-sh` and `sl` are deliberately excluded:
/// they redraw full-screen but are not keyboard-driven, and `UiKind` exists to
/// decide whether Enter must hand the terminal over interactively.
const KNOWN_TUIS: &[&str] = &[
    // Core set.
    "htop",
    "btop",
    "bpytop",
    "bottom",
    "lazygit",
    "lazydocker",
    "vim",
    "neovim",
    "tig",
    "ranger",
    "yazi",
    "k9s",
    "gitui",
    "tmux",
    "less",
    "nano",
    "emacs",
    "midnight-commander",
    "ncdu",
    "glances",
    "helix",
    "zellij",
    // Tier 1 — >10k installs/yr, each at least as common as `tig` or `less`.
    "fzf",
    "glow",
    "mtr",
    "broot",
    "sk",
    "dive",
    "gdu",
    "screen",
    "lnav",
    "fx",
    "micro",
    "lynx",
    "w3m",
    // Tier 2 — 1k-10k installs/yr, still mainstream.
    "moor",
    "mutt",
    "spotify_player",
    "gping",
    "lazysql",
    "superfile",
    "lf",
    "byobu",
    "weechat",
    "nnn",
    "nvtop",
    "podman-tui",
    "neomutt",
    "jjui",
    "jless",
    "peco",
    "iftop",
    "ctop",
    "cava",
    "wtfutil",
    "newsboat",
    "bandwhich",
    "posting",
    "taskwarrior-tui",
    "irssi",
    "links",
    "visidata",
    "sniffnet",
    "harlequin",
    "nethack",
    "trippy",
    "cmus",
    "serie",
    "presenterm",
    "sc-im",
    "nethogs",
    "aerc",
    "gitu",
    "kakoune",
    "ncmpcpp",
    "ov",
    "nload",
    "vifm",
    "bmon",
    "gtop",
    "zenith",
    "pspg",
    "oxker",
    "musikcube",
    "profanity",
    "bastet",
    "calcurse",
    "joshuto",
    "tere",
];

/// Aliases and bare binary names that `brew info --json` never emits.
///
/// A dead entry here produces zero matches forever and looks exactly like a
/// working one, so the list is asserted against rather than merely documented.
#[cfg(test)]
const NEVER_CANONICAL_NAMES: &[&str] = &["mc", "btm", "nvim", "hx"];

/// Phrases with zero measured false positives in the corpus.
///
/// Written in punctuation-normalized form: the haystack is split on
/// non-alphanumerics and rejoined with single spaces, so `terminal-based` and
/// `terminal based` are the same needle. `terminal-based` alone accounts for 19
/// corpus hits, all true — the best ratio in the set.
const TUI_PHRASES: &[&str] = &[
    "terminal based",
    "terminal user interface",
    "terminal ui",
    "terminal multiplexer",
    "terminal file manager",
    "ncurses based",
    "curses based",
    "top like",
    "interactive process viewer",
    "console based",
    "console interface",
    "text mode",
    "in your terminal",
    "for the terminal",
    "lives in your terminal",
    // Framework markers — a program built on these is a TUI by construction.
    "ratatui",
    "bubbletea",
];

/// Bare `tui` as a substring matches "in**tui**tive", which mislabels `sd`,
/// `cli11`, `dust` and `packetry` — all pure CLI. Whole-token matching fixes
/// that; restricting it to descriptions keeps `atuin` and `atuin-server` out.
const TUI_WORDS: &[&str] = &["tui"];

/// `curses` and `ncurses` describe both TUIs and the libraries used to build
/// them. Without the suppression below, `ncurses`, `notcurses`, `cdk` and
/// `libgnt` classify themselves as terminal applications.
const CURSES_WORDS: &[&str] = &["curses", "ncurses"];

/// A description carrying any of these is documenting an API, not a program.
const LIBRARY_MARKERS: &[&str] = &[
    "library",
    "toolkit",
    "widget",
    "development kit",
    "bindings",
    "api for",
];

/// Build this once so every later executable lookup stays O(1) and no caller
/// has to spawn `which` just to classify or launch a package.
pub struct PathIndex {
    executables: HashMap<String, PathBuf>,
}

impl PathIndex {
    pub fn build() -> PathIndex {
        let Some(path) = env::var_os("PATH") else {
            return PathIndex::empty();
        };

        PathIndex::build_from_dirs(env::split_paths(&path))
    }

    /// An index that resolves nothing.
    ///
    /// The fallback when the blocking build task fails to join: classification
    /// degrades to `Unknown` rather than taking the whole inventory down.
    pub fn empty() -> PathIndex {
        PathIndex {
            executables: HashMap::new(),
        }
    }

    /// Consumed by the Enter-to-launch path, which needs to know a program is
    /// runnable before offering to run it. Classification uses `resolve`.
    #[allow(dead_code)]
    pub fn contains(&self, name: &str) -> bool {
        self.executables.contains_key(name)
    }

    pub fn resolve(&self, name: &str) -> Option<PathBuf> {
        self.executables.get(name).cloned()
    }

    /// First of `names` that is on PATH, with its resolved location.
    ///
    /// A formula's own name is frequently not the name of anything it
    /// installs, so both classification and the Enter-to-launch path have to
    /// try the binaries it ships before concluding a package is not runnable.
    pub fn resolve_any<'a>(&self, names: &[&'a str]) -> Option<(&'a str, PathBuf)> {
        names
            .iter()
            .find_map(|name| self.resolve(name).map(|path| (*name, path)))
    }

    fn build_from_dirs<I, P>(dirs: I) -> PathIndex
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut executables = HashMap::new();

        for dir in dirs {
            let dir = dir.as_ref();
            if dir.as_os_str().is_empty() {
                continue;
            }

            // Stale PATH entries are routine on real machines, so missing or unreadable
            // directories are collapsed to "nothing here" instead of failing the scan.
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };

            for entry in entries {
                let Ok(entry) = entry else {
                    continue;
                };
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_dir() {
                    continue;
                }

                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if metadata.is_dir() || metadata.permissions().mode() & 0o111 == 0 {
                    continue;
                }

                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };

                executables.entry(name).or_insert_with(|| entry.path());
            }
        }

        PathIndex { executables }
    }

    #[cfg(test)]
    pub fn from_pairs<I, N, P>(pairs: I) -> PathIndex
    where
        I: IntoIterator<Item = (N, P)>,
        N: Into<String>,
        P: Into<PathBuf>,
    {
        let mut executables = HashMap::new();

        for (name, path) in pairs {
            executables
                .entry(name.into())
                .or_insert_with(|| path.into());
        }

        PathIndex { executables }
    }
}

/// Binary names a formula actually installs, read from its Cellar directory.
///
/// A formula name is frequently not the name of anything it ships: `ripgrep`
/// installs `rg`, `the-silver-searcher` installs `ag`, `fd-find` installs `fd`.
/// Looking up only the package name left 318 of 909 real entries `Unknown` —
/// the second-largest bucket on a live machine.
///
/// Homebrew lays these out at `<Cellar>/<name>/<version>/bin`, which is
/// authoritative, so one `read_dir` per formula replaces any guessing. Casks
/// have no comparable guaranteed shape and are already `Gui`, so they are not
/// probed. Still zero subprocesses.
pub fn binary_aliases(cellar_dir: &Path) -> Vec<String> {
    let Ok(versions) = fs::read_dir(cellar_dir) else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for version in versions.flatten() {
        // A formula can have several versions installed; every one of them
        // ships the same binaries, so the union is what we want and duplicates
        // are dropped below.
        let Ok(binaries) = fs::read_dir(version.path().join("bin")) else {
            continue;
        };
        for binary in binaries.flatten() {
            if let Ok(name) = binary.file_name().into_string()
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
    }

    names
}

pub fn enrich_interface(entries: &mut [AppEntry], path_index: &PathIndex) {
    for entry in entries {
        let aliases = match (&entry.source, entry.path.as_deref()) {
            (Source::Homebrew, Some(path)) => binary_aliases(Path::new(path)),
            _ => Vec::new(),
        };
        let aliases: Vec<&str> = aliases.iter().map(String::as_str).collect();

        entry.ui_kind = Some(classify_ui_kind(
            &entry.name,
            &aliases,
            entry.description.as_deref(),
            entry.path.as_deref(),
            &entry.source,
            path_index,
        ));
    }
}

fn classify_ui_kind(
    name: &str,
    aliases: &[&str],
    description: Option<&str>,
    path: Option<&str>,
    source: &Source,
    path_index: &PathIndex,
) -> UiKind {
    if path
        .map(|path| path.to_ascii_lowercase().ends_with(".app"))
        .unwrap_or(false)
        || is_gui_source(source)
    {
        return UiKind::Gui;
    }

    // Allowlist first, keywords only for names it does not cover. Keywords read
    // the description alone: matching the name is what makes `atuin` a TUI.
    if is_known_tui(name) || description.is_some_and(description_reads_as_tui) {
        return UiKind::Tui;
    }

    // The formula name first, then whatever it actually installs.
    let mut candidates = Vec::with_capacity(aliases.len() + 1);
    candidates.push(name);
    candidates.extend(aliases.iter().copied());

    let resolved = path_index.resolve_any(&candidates);

    // The literal spec put the daemon heuristic after the plain PATH hit, but that
    // makes it unreachable because a service candidate must already resolve on PATH.
    if let Some((resolved_name, resolved_path)) = &resolved
        && resolved_name.ends_with('d')
        && resolved_path.to_string_lossy().contains("/sbin/")
    {
        return UiKind::Service;
    }

    if resolved.is_some() {
        return UiKind::Cli;
    }

    if is_library_source(source) {
        return UiKind::Library;
    }

    UiKind::Unknown
}

fn is_gui_source(source: &Source) -> bool {
    matches!(source, &Source::Applications | &Source::HomebrewCask)
}

fn is_library_source(source: &Source) -> bool {
    matches!(source, &Source::Npm | &Source::Pip | &Source::Gem)
}

fn is_known_tui(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    KNOWN_TUIS.contains(&lower.as_str())
}

/// Punctuation-normalized token view, shared by phrase and whole-word matching.
fn normalize(text: &str) -> (Vec<String>, String) {
    let lower = text.to_ascii_lowercase();
    let tokens: Vec<String> = lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();
    let joined = tokens.join(" ");
    (tokens, joined)
}

fn description_reads_as_tui(description: &str) -> bool {
    let (tokens, normalized) = normalize(description);

    if TUI_PHRASES
        .iter()
        .any(|phrase| normalized.contains(phrase))
    {
        return true;
    }

    if tokens
        .iter()
        .any(|token| TUI_WORDS.contains(&token.as_str()))
    {
        return true;
    }

    let mentions_curses = tokens
        .iter()
        .any(|token| CURSES_WORDS.contains(&token.as_str()));
    let documents_a_library = LIBRARY_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker));

    mentions_curses && !documents_a_library
}

#[cfg(test)]
mod tests {
    use super::{PathIndex, classify_ui_kind, enrich_interface};
    use crate::model::{AppEntry, Source, UiKind};
    use std::fs::{self, File};
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn gui_classifies_from_app_path() {
        let path_index = empty_index();

        assert_eq!(
            classify_ui_kind("Safari", &[], None,
                Some("/Applications/Safari.app"),
                &Source::Homebrew,
                &path_index,
            ),
            UiKind::Gui
        );
    }

    #[test]
    fn tui_keywords_require_boundaries_and_normalize_phrases() {
        let path_index = empty_index();
        let tui = |desc: &str| {
            classify_ui_kind("viewer", &[], Some(desc), None, &Source::Homebrew, &path_index)
        };

        assert_eq!(tui("Terminal user interface for logs"), UiKind::Tui);
        assert_eq!(tui("A terminal-based dashboard"), UiKind::Tui);
        assert_eq!(tui("A terminal based dashboard"), UiKind::Tui);
        assert_eq!(tui("Interactive process viewer"), UiKind::Tui);
        assert_eq!(tui("Built with ratatui"), UiKind::Tui);
        assert_eq!(tui("A TUI for containers"), UiKind::Tui);
    }

    /// Bare substring matching on `tui` mislabels these four real formulae,
    /// which are pure CLI tools whose descriptions contain "intuitive".
    #[test]
    fn intuitive_does_not_read_as_tui() {
        let path_index = empty_index();

        for desc in [
            "an intuitive diff tool",
            "Intuitive find and replace",
            "A more intuitive du",
        ] {
            assert_eq!(
                classify_ui_kind("sd", &[], Some(desc), None, &Source::Homebrew, &path_index),
                UiKind::Unknown,
                "{desc:?} must not read as a TUI"
            );
        }
    }

    /// Matching the name rather than the description is what makes `atuin`
    /// look like a TUI. It is a shell history hook.
    #[test]
    fn tui_keywords_never_match_the_name() {
        let path_index = PathIndex::from_pairs([("atuin", "/opt/homebrew/bin/atuin")]);

        assert_eq!(
            classify_ui_kind("atuin", &[], Some("Improved shell history"),
                None,
                &Source::Homebrew,
                &path_index,
            ),
            UiKind::Cli
        );
    }

    /// A curses *library* is not a terminal application.
    #[test]
    fn curses_libraries_are_not_tuis() {
        let path_index = empty_index();
        let kind = |desc: &str| {
            classify_ui_kind("ncurses", &[], Some(desc), None, &Source::Homebrew, &path_index)
        };

        assert_eq!(kind("Text-based UI library"), UiKind::Unknown);
        assert_eq!(kind("Blingful ncurses widget toolkit"), UiKind::Unknown);
        assert_eq!(kind("Python bindings for ncurses"), UiKind::Unknown);
        // A program that merely says it is curses-driven still counts.
        assert_eq!(kind("Ncurses disk usage analyzer"), UiKind::Tui);
    }

    /// A dead allowlist entry produces zero matches forever and is invisible
    /// in every other test, so assert the four known-broken forms are absent.
    #[test]
    fn known_tuis_holds_no_aliases() {
        for alias in super::NEVER_CANONICAL_NAMES {
            assert!(
                !super::KNOWN_TUIS.contains(alias),
                "{alias:?} is an alias or binary name; brew reports the canonical formula name"
            );
        }
    }

    #[test]
    fn known_tuis_has_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for name in super::KNOWN_TUIS {
            assert!(seen.insert(name), "{name:?} listed twice");
        }
    }

    /// The measured headline: the most-installed TUIs describe themselves as
    /// command-line tools, so only the allowlist can catch them.
    #[test]
    fn allowlist_catches_tuis_whose_description_says_command_line() {
        let path_index = PathIndex::from_pairs([
            ("fzf", "/opt/homebrew/bin/fzf"),
            ("glow", "/opt/homebrew/bin/glow"),
        ]);

        assert_eq!(
            classify_ui_kind("fzf", &[], Some("Command-line fuzzy finder written in Go"),
                None,
                &Source::Homebrew,
                &path_index,
            ),
            UiKind::Tui
        );
        assert_eq!(
            classify_ui_kind("glow", &[], Some("Render markdown on the CLI"),
                None,
                &Source::Homebrew,
                &path_index,
            ),
            UiKind::Tui
        );
    }

    #[test]
    fn known_tui_list_beats_path_lookup() {
        let path_index = PathIndex::from_pairs([("htop", "/usr/bin/htop")]);

        assert_eq!(
            classify_ui_kind("htop", &[], None, None, &Source::Homebrew, &path_index),
            UiKind::Tui
        );
    }

    #[test]
    fn d_suffixed_binary_outside_sbin_stays_cli() {
        let path_index = PathIndex::from_pairs([("zstd", "/usr/bin/zstd")]);

        assert_eq!(
            classify_ui_kind("zstd", &[], None, None, &Source::Homebrew, &path_index),
            UiKind::Cli
        );
    }

    #[test]
    fn d_suffixed_binary_in_sbin_is_service() {
        let path_index = PathIndex::from_pairs([("syslogd", "/usr/sbin/syslogd")]);

        assert_eq!(
            classify_ui_kind("syslogd", &[], None, None, &Source::Homebrew, &path_index),
            UiKind::Service
        );
    }

    #[test]
    fn npm_package_absent_from_path_is_library_but_present_one_is_cli() {
        let absent = empty_index();
        let present = PathIndex::from_pairs([("eslint", "/usr/bin/eslint")]);

        assert_eq!(
            classify_ui_kind("eslint", &[], None, None, &Source::Npm, &absent),
            UiKind::Library
        );
        assert_eq!(
            classify_ui_kind("eslint", &[], None, None, &Source::Npm, &present),
            UiKind::Cli
        );
    }

    #[test]
    fn enrich_interface_sets_ui_kind_for_every_entry() {
        let path_index =
            PathIndex::from_pairs([("syslogd", "/usr/sbin/syslogd"), ("ripgrep", "/usr/bin/rg")]);
        let mut entries = vec![
            test_entry("ripgrep", Source::Homebrew),
            test_entry("syslogd", Source::Homebrew),
            test_entry("crate-only", Source::Cargo),
        ];

        enrich_interface(&mut entries, &path_index);

        assert_eq!(entries[0].ui_kind, Some(UiKind::Cli));
        assert_eq!(entries[1].ui_kind, Some(UiKind::Service));
        assert_eq!(entries[2].ui_kind, Some(UiKind::Unknown));
    }

    #[test]
    fn build_skips_missing_directories_and_non_executables() {
        let temp_dir = unique_temp_dir();
        let exec_path = temp_dir.join("runner");
        let plain_path = temp_dir.join("notes");
        let nested_dir = temp_dir.join("nested");
        let missing_dir = temp_dir.join("missing");

        fs::create_dir_all(&temp_dir).unwrap();
        fs::create_dir_all(&nested_dir).unwrap();
        File::create(&exec_path).unwrap();
        File::create(&plain_path).unwrap();

        let mut exec_permissions = fs::metadata(&exec_path).unwrap().permissions();
        exec_permissions.set_mode(0o755);
        fs::set_permissions(&exec_path, exec_permissions).unwrap();

        let mut plain_permissions = fs::metadata(&plain_path).unwrap().permissions();
        plain_permissions.set_mode(0o644);
        fs::set_permissions(&plain_path, plain_permissions).unwrap();

        let path_index = PathIndex::build_from_dirs([missing_dir.as_path(), temp_dir.as_path()]);

        assert!(path_index.contains("runner"));
        assert_eq!(path_index.resolve("runner"), Some(exec_path));
        assert!(!path_index.contains("notes"));
        assert!(!path_index.contains("nested"));

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    fn test_entry(name: &str, source: Source) -> AppEntry {
        AppEntry {
            name: name.to_string(),
            version: None,
            source,
            language: None,
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: None,
            description: None,
            ui_kind: None,
            category: None,
        }
    }

    fn empty_index() -> PathIndex {
        PathIndex::from_pairs(Vec::<(&str, &str)>::new())
    }

    /// The formula name is not the binary name for a large minority of
    /// packages, and looking up only the name left `ripgrep` as `Unknown`.
    #[test]
    fn a_formula_resolves_through_the_binaries_it_installs() {
        let path_index = PathIndex::from_pairs([("rg", "/opt/homebrew/bin/rg")]);

        // The package name alone finds nothing.
        assert_eq!(
            classify_ui_kind("ripgrep", &[], None, None, &Source::Homebrew, &path_index),
            UiKind::Unknown
        );
        // Its installed binary does.
        assert_eq!(
            classify_ui_kind("ripgrep", &["rg"], None, None, &Source::Homebrew, &path_index),
            UiKind::Cli
        );
    }

    /// The daemon rule must follow the binary that actually resolved, not the
    /// package name, or an alias would be judged by the wrong suffix.
    #[test]
    fn service_detection_follows_the_resolved_binary() {
        let path_index = PathIndex::from_pairs([("syslogd", "/usr/sbin/syslogd")]);

        assert_eq!(
            classify_ui_kind(
                "syslog-package",
                &["syslogd"],
                None,
                None,
                &Source::Homebrew,
                &path_index,
            ),
            UiKind::Service
        );
    }

    /// An npm package whose name is absent from PATH but which installs a
    /// binary that is present is a CLI, not a library.
    #[test]
    fn aliases_are_checked_before_falling_back_to_library() {
        let path_index = PathIndex::from_pairs([("tsc", "/opt/homebrew/bin/tsc")]);

        assert_eq!(
            classify_ui_kind("typescript", &["tsc"], None, None, &Source::Npm, &path_index),
            UiKind::Cli
        );
        assert_eq!(
            classify_ui_kind("typescript", &[], None, None, &Source::Npm, &path_index),
            UiKind::Library
        );
    }

    /// `binary_aliases` reads the real Homebrew Cellar layout,
    /// `<Cellar>/<name>/<version>/bin`, and unions across installed versions.
    #[test]
    fn binary_aliases_reads_the_cellar_layout() {
        let cellar = unique_temp_dir();
        let v1 = cellar.join("14.1.0").join("bin");
        let v2 = cellar.join("14.0.0").join("bin");
        fs::create_dir_all(&v1).unwrap();
        fs::create_dir_all(&v2).unwrap();
        File::create(v1.join("rg")).unwrap();
        File::create(v2.join("rg")).unwrap();
        File::create(v2.join("rg-legacy")).unwrap();

        let mut aliases = super::binary_aliases(&cellar);
        aliases.sort();

        assert_eq!(aliases, vec!["rg".to_string(), "rg-legacy".to_string()]);

        fs::remove_dir_all(&cellar).unwrap();
    }

    /// A formula directory that does not exist, or one with no `bin`, must
    /// yield no aliases rather than panicking — casks and kegs without
    /// executables both hit this path.
    #[test]
    fn binary_aliases_tolerates_a_missing_or_binless_cellar() {
        let missing = unique_temp_dir();
        assert!(super::binary_aliases(&missing).is_empty());

        let binless = unique_temp_dir();
        fs::create_dir_all(binless.join("1.0.0")).unwrap();
        assert!(super::binary_aliases(&binless).is_empty());
        fs::remove_dir_all(&binless).unwrap();
    }

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("sysapp-tui-interface-tests-{nanos}"))
    }
}

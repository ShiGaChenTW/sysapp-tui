use crate::model::{AppEntry, Source};
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Deserialize)]
struct BrewOutput {
    #[serde(default)]
    formulae: Vec<Formula>,
    #[serde(default)]
    casks: Vec<Cask>,
}

#[derive(Deserialize)]
struct Formula {
    name: String,
    desc: Option<String>,
    #[serde(default)]
    installed: Vec<InstalledVersion>,
}

#[derive(Deserialize)]
struct InstalledVersion {
    version: String,
}

#[derive(Deserialize)]
struct Cask {
    token: String,
    desc: Option<String>,
    version: Option<String>,
}

/// `brew info --json=v2 --installed` already costs about 38 seconds of the
/// ~89-second scan. Resolving the prefix via env var and a bounded directory
/// probe avoids paying for a second `brew` subprocess just to ask for `--prefix`.
fn resolve_brew_prefix() -> Option<PathBuf> {
    let env_prefix = std::env::var_os("HOMEBREW_PREFIX").map(PathBuf::from);
    let candidates = [PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")];
    resolve_brew_prefix_from_candidates(env_prefix.as_deref(), &candidates)
}

fn resolve_brew_prefix_from_candidates(
    env_prefix: Option<&Path>,
    candidates: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(prefix) = env_prefix {
        return Some(prefix.to_path_buf());
    }

    candidates
        .iter()
        .find(|prefix| prefix.join("Cellar").is_dir())
        .cloned()
}

fn directory_path_and_mtime(path: &Path) -> Option<(String, Option<DateTime<Local>>)> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_dir() {
        return None;
    }

    let install_date = metadata.modified().ok().map(DateTime::<Local>::from);
    Some((path.to_string_lossy().into_owned(), install_date))
}

fn brew_entry_path_and_install_date(
    prefix: Option<&Path>,
    top_level_dir: &str,
    name: &str,
) -> (Option<String>, Option<DateTime<Local>>) {
    let Some(prefix) = prefix else {
        return (None, None);
    };

    match directory_path_and_mtime(&prefix.join(top_level_dir).join(name)) {
        Some((path, install_date)) => (Some(path), install_date),
        None => (None, None),
    }
}

pub async fn scan() -> Result<Vec<AppEntry>> {
    let output = Command::new("brew")
        .args(["info", "--json=v2", "--installed"])
        .output()
        .await
        .context("failed to run brew")?;

    if !output.status.success() {
        anyhow::bail!("brew exited with {}", output.status);
    }

    let info: BrewOutput =
        serde_json::from_slice(&output.stdout).context("failed to parse brew JSON")?;

    let mut entries = Vec::new();
    let brew_prefix = resolve_brew_prefix();

    for f in &info.formulae {
        let version = f.installed.first().map(|i| i.version.clone());
        // This async scanner does a bounded number of local `stat` calls here.
        // That cost is tiny compared to building a full PATH index, so keeping
        // it synchronous avoids extra coordination without blocking for long.
        let (path, install_date) =
            brew_entry_path_and_install_date(brew_prefix.as_deref(), "Cellar", &f.name);
        entries.push(AppEntry {
            name: f.name.clone(),
            version,
            source: Source::Homebrew,
            language: None,
            install_date,
            last_used: None,
            usage_count: 0,
            path,
            description: f.desc.clone(),
            ui_kind: None,
            category: None,
        });
    }

    for c in &info.casks {
        let (path, install_date) =
            brew_entry_path_and_install_date(brew_prefix.as_deref(), "Caskroom", &c.token);
        entries.push(AppEntry {
            name: c.token.clone(),
            version: c.version.clone(),
            source: Source::HomebrewCask,
            language: None,
            install_date,
            last_used: None,
            usage_count: 0,
            path,
            description: c.desc.clone(),
            ui_kind: None,
            category: None,
        });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::{directory_path_and_mtime, resolve_brew_prefix_from_candidates};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct ScratchDir {
        path: PathBuf,
    }

    impl ScratchDir {
        fn new() -> Self {
            let unique = format!(
                "sysapp-tui-brew-tests-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn brew_prefix_resolution_prefers_env_var() {
        let scratch = ScratchDir::new();
        let env_prefix = scratch.path().join("env-prefix");
        let fallback = scratch.path().join("fallback");
        fs::create_dir_all(fallback.join("Cellar")).unwrap();

        let resolved =
            resolve_brew_prefix_from_candidates(Some(&env_prefix), std::slice::from_ref(&fallback));

        assert_eq!(resolved, Some(env_prefix));
    }

    #[test]
    fn brew_prefix_resolution_uses_first_candidate_with_cellar() {
        let scratch = ScratchDir::new();
        let first = scratch.path().join("first");
        let second = scratch.path().join("second");
        fs::create_dir_all(second.join("Cellar")).unwrap();

        let resolved =
            resolve_brew_prefix_from_candidates(None, &[first.clone(), second.clone()]);

        assert_eq!(resolved, Some(second));
    }

    #[test]
    fn brew_prefix_resolution_returns_none_when_no_candidate_matches() {
        let scratch = ScratchDir::new();
        let first = scratch.path().join("first");
        let second = scratch.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();

        let resolved = resolve_brew_prefix_from_candidates(None, &[first, second]);

        assert_eq!(resolved, None);
    }

    #[test]
    fn directory_path_and_mtime_returns_path_and_date_for_existing_directory() {
        let scratch = ScratchDir::new();
        let dir = scratch.path().join("Cellar").join("ripgrep");
        fs::create_dir_all(&dir).unwrap();

        let result = directory_path_and_mtime(&dir);

        assert!(matches!(result, Some((path, Some(_))) if path == dir.to_string_lossy()));
    }

    #[test]
    fn directory_path_and_mtime_returns_none_for_missing_directory() {
        let scratch = ScratchDir::new();
        let dir = scratch.path().join("Cellar").join("missing");

        let result = directory_path_and_mtime(&dir);

        assert_eq!(result, None);
    }
}

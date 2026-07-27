//! Enrichment passes over the scanned inventory.
//!
//! Nothing here writes to stdout or stderr: the TUI holds the alternate
//! screen for the whole scan, so stray output would be painted over the
//! rendered frame. Progress is reported through `Message` instead.

/// Reachable from `exec` so Enter resolves a binary through the same index
/// and the same Cellar aliases the classifier used, rather than a second
/// resolver that could disagree with the `UI` column on screen.
pub(crate) mod interface;
/// Reachable from the TUI so `C` can recompute one entry's automatic category
/// after the user clears their override.
pub(crate) mod category;
mod language;
mod usage;

use crate::config;
use crate::model::AppEntry;
use chrono::{DateTime, Local};
use interface::PathIndex;
use std::fs;
use std::path::Path;

fn path_mtime(path: &Path) -> Option<DateTime<Local>> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(DateTime::<Local>::from)
}

/// This runs after every scanner-specific pass so a path mtime never overwrites
/// better install metadata from sources that know the real installation moment.
fn fill_install_dates_from_paths(entries: &mut [AppEntry]) {
    // Even the worst case here is only about 900 local `stat` calls, so keeping
    // the fallback synchronous is simpler than bouncing a tiny workload through
    // another blocking task.
    for entry in entries {
        if entry.install_date.is_some() {
            continue;
        }

        let Some(path) = entry.path.as_deref() else {
            continue;
        };

        entry.install_date = path_mtime(Path::new(path));
    }
}

/// Run all enrichment passes on scanned entries.
pub async fn enrich(entries: &mut [AppEntry]) {
    // Building the PATH index walks every directory on PATH synchronously.
    // That can block a Tokio worker for noticeable time, unlike brew's bounded
    // local `stat` calls, so this pass is pushed onto the blocking pool.
    let path_index = tokio::task::spawn_blocking(PathIndex::build)
        .await
        .unwrap_or_else(|_| PathIndex::empty());
    // Config load is a single small read with an empty-config fallback on error,
    // so the coordination cost of another blocking handoff would buy little.
    let cfg = config::load();

    interface::enrich_interface(entries, &path_index);
    category::enrich_categories(entries, &cfg);
    language::enrich_languages(entries).await;
    usage::enrich_usage(entries).await;
    fill_install_dates_from_paths(entries);
}

/// Owned wrapper for `enrich`, so it can be driven by a `Command::perform`
/// future that has to hand the data back to the update loop.
pub async fn enrich_owned(mut entries: Vec<crate::model::AppEntry>) -> Vec<crate::model::AppEntry> {
    enrich(&mut entries).await;
    entries
}

#[cfg(test)]
mod tests {
    use super::fill_install_dates_from_paths;
    use crate::model::{AppEntry, Source};
    use chrono::{Duration, Local};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct ScratchDir {
        path: PathBuf,
    }

    impl ScratchDir {
        fn new() -> Self {
            let unique = format!(
                "sysapp-tui-enricher-tests-{}-{}",
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

    fn test_entry(path: Option<String>, install_date: Option<chrono::DateTime<Local>>) -> AppEntry {
        AppEntry {
            name: "test".into(),
            version: None,
            source: Source::Homebrew,
            language: None,
            install_date,
            last_used: None,
            usage_count: 0,
            path,
            description: None,
            ui_kind: None,
            category: None,
        }
    }

    #[test]
    fn install_date_fallback_fills_missing_date_from_existing_path() {
        let scratch = ScratchDir::new();
        let file = scratch.path().join("tool");
        fs::write(&file, b"binary").unwrap();
        let mut entries = vec![test_entry(Some(file.to_string_lossy().into_owned()), None)];

        fill_install_dates_from_paths(&mut entries);

        assert!(entries[0].install_date.is_some());
    }

    #[test]
    fn install_date_fallback_preserves_existing_date() {
        let scratch = ScratchDir::new();
        let file = scratch.path().join("tool");
        fs::write(&file, b"binary").unwrap();
        let original = Local::now() - Duration::days(7);
        let mut entries = vec![test_entry(
            Some(file.to_string_lossy().into_owned()),
            Some(original),
        )];

        fill_install_dates_from_paths(&mut entries);

        assert_eq!(entries[0].install_date, Some(original));
    }

    #[test]
    fn install_date_fallback_skips_missing_paths() {
        let scratch = ScratchDir::new();
        let missing = scratch.path().join("missing");
        let mut entries = vec![test_entry(Some(missing.to_string_lossy().into_owned()), None)];

        fill_install_dates_from_paths(&mut entries);

        assert_eq!(entries[0].install_date, None);
    }
}


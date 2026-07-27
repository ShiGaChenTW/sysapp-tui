//! On-disk inventory snapshot.
//!
//! A full scan costs ~89 seconds on a typical machine, dominated by
//! `brew info --json=v2 --installed` (~38s on its own). That cost cannot be
//! optimised away — the scanners already run concurrently and brew is queried
//! exactly once. The only way to open quickly is to not scan at launch.
//!
//! So the inventory is written to disk after every scan and read back on the
//! next launch. A stale snapshot shown instantly beats a fresh one shown after
//! a minute and a half; the header displays the snapshot's age so the staleness
//! is never hidden, and `--refresh` (or `r` in the TUI) forces a rescan.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::model::AppEntry;

/// Bump whenever `AppEntry` or anything it contains changes shape.
///
/// A snapshot written by an older binary would otherwise fail to deserialize
/// into the new type, and the user would see a confusing error instead of a
/// rescan. On mismatch we silently discard and rescan — the data is a pure
/// cache of the local system, so there is nothing to migrate or lose.
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub generated_at: DateTime<Local>,
    pub entries: Vec<AppEntry>,
}

impl Snapshot {
    #[cfg(test)]
    fn age_label(&self) -> String {
        age_label(self.generated_at)
    }
}

/// Human-readable age, for the header. Deliberately coarse — the user needs
/// "is this roughly current", not a precise duration. Clamped at zero so a
/// backwards clock jump cannot render a negative age.
pub fn age_label(generated_at: DateTime<Local>) -> String {
    let secs = (Local::now() - generated_at).num_seconds().max(0);
    match secs {
        s if s < 90 => "JUST NOW".into(),
        s if s < 3600 => format!("{}M AGO", s / 60),
        s if s < 172_800 => format!("{}H AGO", s / 3600),
        s => format!("{}D AGO", s / 86_400),
    }
}

/// Path to the snapshot, or `None` if the platform has no cache directory.
///
/// Resolved via `dirs::cache_dir()`, so this is `~/Library/Caches/sysapp-tui/`
/// on macOS and `~/.cache/sysapp-tui/` on Linux — not hard-coded either way.
pub fn path() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("sysapp-tui").join("inventory.json"))
}

/// Read the snapshot, or `None` if there isn't a usable one.
///
/// Every failure mode — missing file, unreadable, corrupt JSON, wrong schema
/// version — collapses to `None` so the caller simply rescans. A cache that
/// can crash the program is worse than no cache.
pub fn load() -> Option<Snapshot> {
    let path = path()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let snapshot: Snapshot = serde_json::from_str(&raw).ok()?;
    if snapshot.version != SCHEMA_VERSION {
        return None;
    }
    // An empty snapshot is never a legitimate scan of a real Mac — pkgutil
    // alone always reports something. Treating it as "no cache" means a
    // previously poisoned file heals itself on the next launch instead of
    // showing an empty tool forever.
    if snapshot.entries.is_empty() {
        return None;
    }
    Some(snapshot)
}

/// Write the snapshot, replacing any existing one.
///
/// Writes to a temporary file and renames, so an interrupted write cannot
/// leave a truncated snapshot behind — the rename is atomic within a
/// filesystem, and the temp file is a sibling to guarantee that.
pub fn save(entries: &[AppEntry]) -> Result<()> {
    // Refuse at the boundary as well as at the call site: persisting an empty
    // inventory would make every subsequent launch open instantly onto nothing.
    anyhow::ensure!(!entries.is_empty(), "refusing to cache an empty inventory");
    let path = path().context("no cache directory on this platform")?;
    let dir = path.parent().context("cache path has no parent")?;
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating {}", dir.display()))?;

    let snapshot = Snapshot {
        version: SCHEMA_VERSION,
        generated_at: Local::now(),
        entries: entries.to_vec(),
    };

    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&snapshot)?)
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Language, Source};

    fn entry(name: &str) -> AppEntry {
        AppEntry {
            name: name.into(),
            version: Some("1.0".into()),
            source: Source::Homebrew,
            language: Some(Language::Rust),
            install_date: Some(Local::now()),
            last_used: None,
            usage_count: 7,
            path: Some("/opt/homebrew/bin/x".into()),
            description: None,
        }
    }

    /// The whole cache rests on `AppEntry` surviving a JSON round-trip.
    #[test]
    fn snapshot_round_trips() {
        let snap = Snapshot {
            version: SCHEMA_VERSION,
            generated_at: Local::now(),
            entries: vec![entry("ripgrep"), entry("系統設定")],
        };
        let json = serde_json::to_string(&snap).expect("serialize");
        let back: Snapshot = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.version, snap.version);
        assert_eq!(back.entries.len(), 2);
        assert_eq!(back.entries[1].name, "系統設定");
        assert_eq!(back.entries[0].usage_count, 7);
        assert_eq!(back.entries[0].source, Source::Homebrew);
    }

    /// A snapshot from a future (or past) schema must be rejected, not
    /// deserialized into the wrong shape.
    #[test]
    fn version_mismatch_is_rejected() {
        let json = serde_json::json!({
            "version": SCHEMA_VERSION + 1,
            "generated_at": Local::now().to_rfc3339(),
            "entries": [],
        })
        .to_string();
        let snap: Snapshot = serde_json::from_str(&json).expect("parses");
        assert_ne!(snap.version, SCHEMA_VERSION, "loader must reject this");
    }

    /// Corrupt JSON must not panic — `load` swallows it and the caller rescans.
    #[test]
    fn corrupt_json_does_not_panic() {
        for bad in ["", "{", "null", r#"{"version":1}"#, "[1,2,3]"] {
            assert!(serde_json::from_str::<Snapshot>(bad).is_err(), "{bad:?}");
        }
    }

    /// A scan that produced nothing must never become the cached state.
    #[test]
    fn empty_inventory_is_never_persisted() {
        let err = save(&[]).expect_err("must refuse");
        assert!(err.to_string().contains("empty"), "{err}");
    }

    #[test]
    fn age_labels_are_coarse_and_ordered() {
        let mk = |secs: i64| Snapshot {
            version: SCHEMA_VERSION,
            generated_at: Local::now() - chrono::Duration::seconds(secs),
            entries: vec![],
        };
        assert_eq!(mk(10).age_label(), "JUST NOW");
        assert_eq!(mk(600).age_label(), "10M AGO");
        assert_eq!(mk(7200).age_label(), "2H AGO");
        assert_eq!(mk(259_200).age_label(), "3D AGO");
    }

    /// A clock that jumped backwards must not produce a negative age.
    #[test]
    fn future_timestamp_clamps_to_just_now() {
        let snap = Snapshot {
            version: SCHEMA_VERSION,
            generated_at: Local::now() + chrono::Duration::hours(3),
            entries: vec![],
        };
        assert_eq!(snap.age_label(), "JUST NOW");
    }
}

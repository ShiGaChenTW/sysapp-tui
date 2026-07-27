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
use crate::tui::i18n::Lang;

/// Bump whenever `AppEntry` or anything it contains changes shape.
///
/// A snapshot written by an older binary would otherwise fail to deserialize
/// into the new type, and the user would see a confusing error instead of a
/// rescan. On mismatch we silently discard and rescan — the data is a pure
/// cache of the local system, so there is nothing to migrate or lose.
const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub generated_at: DateTime<Local>,
    pub entries: Vec<AppEntry>,
}

impl Snapshot {
    #[cfg(test)]
    fn age_label(&self) -> String {
        age_label(self.generated_at, Lang::En)
    }
}

/// Human-readable age, for the header. Deliberately coarse — the user needs
/// "is this roughly current", not a precise duration. Clamped at zero so a
/// backwards clock jump cannot render a negative age.
pub fn age_label(generated_at: DateTime<Local>, lang: Lang) -> String {
    let t = lang.strings();
    let secs = (Local::now() - generated_at).num_seconds().max(0);
    match secs {
        s if s < 90 => t.just_now.into(),
        s if s < 3600 => format!("{}{}", s / 60, t.minutes_ago),
        s if s < 172_800 => format!("{}{}", s / 3600, t.hours_ago),
        // Beyond two months, days stop reading as a duration: the record panel
        // was rendering "512D AGO" for software installed a year and a half
        // ago, which nobody converts in their head. The tiers below are the
        // coarse units a person actually uses, and the divisors are deliberate
        // approximations — this answers "roughly how long", never "exactly".
        s if s < 5_184_000 => format!("{}{}", s / 86_400, t.days_ago),
        s if s < 63_072_000 => format!("{}{}", s / 2_592_000, t.months_ago),
        s => format!("{}{}", s / 31_536_000, t.years_ago),
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
    load_from(&path()?)
}

/// A cache this large is not a snapshot this program wrote.
///
/// The real file is ~200 KB for 900 packages; 64 MB covers a machine with
/// hundreds of thousands. Without the bound, `read_to_string` will happily
/// pull a 500 MB file into memory (measured: 1.07 GB RSS) before serde
/// rejects it, and a larger one scales straight to OOM.
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;

/// `load` against an explicit path, so tests never touch the real cache.
fn load_from(path: &std::path::Path) -> Option<Snapshot> {
    // Stat before opening. A fifo or device at this path would block
    // `read_to_string` forever — before any UI exists, so the user sees a
    // blank terminal with no message and no way to tell what is wrong.
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_CACHE_BYTES {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
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
/// Writes to a process-unique temporary file and renames it into place. The
/// rename is atomic within a filesystem, and the temp file is a sibling to
/// guarantee that, so an interrupted write cannot leave a truncated snapshot.
///
/// The temp name carries the pid: a fixed name would let two instances
/// truncate and interleave writes into the same inode, and one of them would
/// then rename corrupt JSON into place. `load` tolerates that — it just costs
/// a full rescan — but it is cheap to prevent.
///
/// Not fsynced. A power loss between rename and flush can leave the entry
/// durable and the contents not; the cost of that is one cold scan, which does
/// not justify the write barrier on every scan.
pub fn save(entries: &[AppEntry]) -> Result<()> {
    save_to(&path().context("no cache directory on this platform")?, entries)
}

/// `save` against an explicit path, so tests never touch the real cache.
fn save_to(path: &std::path::Path, entries: &[AppEntry]) -> Result<()> {
    // Refuse at the boundary as well as at the call site: persisting an empty
    // inventory would make every subsequent launch open instantly onto nothing.
    anyhow::ensure!(!entries.is_empty(), "refusing to cache an empty inventory");
    let dir = path.parent().context("cache path has no parent")?;
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating {}", dir.display()))?;

    let snapshot = Snapshot {
        version: SCHEMA_VERSION,
        generated_at: Local::now(),
        entries: entries.to_vec(),
    };

    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec(&snapshot)?)
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
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
            ui_kind: None,
            category: None,
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

    /// Drive `load`/`save` against a scratch file rather than testing serde.
    ///
    /// The previous versions of these tests called `serde_json::from_str`
    /// directly and never touched `load` at all — they passed with the version
    /// check deleted, and would have passed with `load` rewritten to `unwrap`.
    /// They also must not use the real cache path: a panic mid-test would
    /// destroy a snapshot that costs ~90 seconds to rebuild.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("sysapp-tui-test-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir.join("inventory.json"))
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn write(&self, contents: &str) {
            std::fs::write(&self.0, contents).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
        }
    }

    fn snapshot_json(version: u32, entries: serde_json::Value) -> String {
        serde_json::json!({
            "version": version,
            "generated_at": Local::now().to_rfc3339(),
            "entries": entries,
        })
        .to_string()
    }

    #[test]
    fn load_rejects_a_foreign_schema_version() {
        let s = Scratch::new("version");
        s.write(&snapshot_json(SCHEMA_VERSION + 1, serde_json::json!([entry("ripgrep")])));
        assert!(load_from(s.path()).is_none(), "a foreign version must not load");
    }

    #[test]
    fn load_rejects_corrupt_input_without_panicking() {
        let s = Scratch::new("corrupt");
        for bad in ["", "{", "null", r#"{"version":1}"#, "[1,2,3]", "\u{0}not json"] {
            s.write(bad);
            assert!(load_from(s.path()).is_none(), "{bad:?} must not load");
        }
    }

    #[test]
    fn load_rejects_an_empty_inventory() {
        let s = Scratch::new("empty");
        s.write(&snapshot_json(SCHEMA_VERSION, serde_json::json!([])));
        assert!(
            load_from(s.path()).is_none(),
            "an empty snapshot must read as no cache so it self-heals"
        );
    }

    /// A fifo at the cache path must not block the process forever.
    #[test]
    #[cfg(unix)]
    fn load_does_not_block_on_a_fifo() {
        use std::os::unix::fs::FileTypeExt;
        let s = Scratch::new("fifo");
        let path = s.path();
        let status = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .expect("mkfifo");
        assert!(status.success());
        assert!(std::fs::metadata(path).unwrap().file_type().is_fifo());

        // Would hang indefinitely without the stat guard.
        assert!(load_from(path).is_none(), "a fifo must read as no cache");
    }

    /// An implausibly large file must be rejected on sight, not read into
    /// memory and then rejected by serde.
    #[test]
    fn load_rejects_an_oversized_file() {
        let s = Scratch::new("huge");
        let f = std::fs::File::create(s.path()).unwrap();
        f.set_len(MAX_CACHE_BYTES + 1).unwrap();
        drop(f);
        assert!(load_from(s.path()).is_none());
    }

    #[test]
    fn load_of_a_missing_file_is_not_an_error() {
        let s = Scratch::new("missing");
        assert!(load_from(s.path()).is_none());
    }

    /// The success path — never exercised before, which is how the shared
    /// temp-file name went unnoticed.
    #[test]
    fn save_then_load_round_trips_and_leaves_no_temp_file() {
        let s = Scratch::new("roundtrip");
        save_to(s.path(), &[entry("ripgrep"), entry("系統設定")]).expect("save");

        let got = load_from(s.path()).expect("must load what was just saved");
        assert_eq!(got.entries.len(), 2);
        assert_eq!(got.entries[1].name, "系統設定");
        assert_eq!(got.version, SCHEMA_VERSION);

        let leftovers: Vec<_> = std::fs::read_dir(s.path().parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files left behind: {leftovers:?}");
    }

    /// Concurrent writers must always leave a snapshot that loads.
    ///
    /// NOT a guard for the process-unique temp name. That change is defensive:
    /// with a shared temp name this test still passes on APFS at 6 writers ×
    /// 4000 entries (verified by reverting the fix and running it three
    /// times), because each writer's `write_all` lands without visible
    /// interleaving. Treat the unique name as cheap insurance against a race
    /// this test cannot reproduce, not as something proven here.
    #[test]
    fn concurrent_saves_leave_a_loadable_snapshot() {
        let s = Scratch::new("concurrent");
        let path = s.path().to_path_buf();
        const WRITERS: usize = 6;
        const ENTRIES: usize = 4000;

        std::thread::scope(|scope| {
            for i in 0..WRITERS {
                let p = path.clone();
                scope.spawn(move || {
                    let batch: Vec<_> = (0..ENTRIES)
                        .map(|n| entry(&format!("package-number-{i}-{n}-with-a-long-name")))
                        .collect();
                    let _ = save_to(&p, &batch);
                });
            }
        });

        let got = load_from(s.path()).expect("one writer must win with intact JSON");
        assert_eq!(
            got.entries.len(),
            ENTRIES,
            "a partial or interleaved write survived into the snapshot"
        );
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
        // The record panel shows install dates, which are routinely months or
        // years old. Days stopped scaling there: this is the tier that keeps
        // "512D AGO" from ever reaching the screen again.
        assert_eq!(mk(86_400 * 512).age_label(), "17MO AGO");
        assert_eq!(mk(86_400 * 900).age_label(), "2Y AGO");

        // Each boundary lands in the tier below it, not the one above.
        assert!(mk(86_400 * 59).age_label().ends_with("D AGO"));
        assert!(mk(86_400 * 61).age_label().ends_with("MO AGO"));
        assert!(mk(86_400 * 729).age_label().ends_with("MO AGO"));
        assert!(mk(86_400 * 731).age_label().ends_with("Y AGO"));
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

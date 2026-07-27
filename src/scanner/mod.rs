mod applications;
mod brew;
mod cargo_scan;
mod gem;
mod go;
mod npm;
mod pip;
mod pkgutil;

use crate::model::{AppEntry, Source};
use anyhow::Result;
use std::collections::HashMap;

/// Every source, in the order the progress screen lists them.
///
/// `brew` is first because it dominates the wall clock (~38s of a ~89s cold
/// scan); showing it at the top makes the slow one the visible one.
pub const SOURCES: [&str; 8] = [
    "brew", "apps", "pkgutil", "gem", "cargo", "pip", "go", "npm",
];

/// Run a single source by name. Unknown names are a programming error.
pub async fn scan_one(name: &'static str) -> Result<Vec<AppEntry>> {
    match name {
        "brew" => brew::scan().await,
        "npm" => npm::scan().await,
        "pip" => pip::scan().await,
        "cargo" => cargo_scan::scan().await,
        "go" => go::scan().await,
        "gem" => gem::scan().await,
        "pkgutil" => pkgutil::scan().await,
        "apps" => applications::scan().await,
        other => anyhow::bail!("unknown scan source {other:?}"),
    }
}

/// Merge raw per-source results into the deduplicated inventory.
pub fn merge(entries: Vec<AppEntry>) -> Vec<AppEntry> {
    dedup(entries)
}

/// Source priority — higher = better metadata, kept over duplicates.
fn source_priority(s: &Source) -> u8 {
    match s {
        Source::Homebrew => 7,      // best CLI metadata (version, desc)
        Source::HomebrewCask => 6,  // best GUI metadata (version, desc)
        Source::Applications => 5,  // has path, install date
        Source::Cargo => 4,
        Source::Go => 3,
        Source::Npm => 2,
        Source::Pip => 2,
        Source::Gem => 2,
        Source::Pkgutil => 1,       // raw package IDs, least useful
    }
}

/// Deduplicate entries by normalized name, keeping the highest-priority source.
/// When sources differ, merge useful fields from the lower-priority entry
/// (path, version) if the higher-priority one lacks them.
fn dedup(entries: Vec<AppEntry>) -> Vec<AppEntry> {
    // Coalesce by lowercased name
    let mut map: HashMap<String, AppEntry> = HashMap::new();

    for e in entries {
        let key = e.name.to_lowercase();
        if let Some(existing) = map.get_mut(&key) {
            let existing_prio = source_priority(&existing.source);
            let incoming_prio = source_priority(&e.source);

            if incoming_prio > existing_prio {
                // Higher-priority source wins — replace, but carry forward
                // non-conflicting fields the winner might lack
                let old_version = existing.version.take();
                let old_path = existing.path.take();
                let old_desc = existing.description.take();
                let old_install_date = existing.install_date;
                let old_last_used = existing.last_used;
                let old_usage = existing.usage_count;

                *existing = e;

                if existing.version.is_none() { existing.version = old_version; }
                if existing.path.is_none() { existing.path = old_path; }
                if existing.description.is_none() { existing.description = old_desc; }
                if existing.install_date.is_none() { existing.install_date = old_install_date; }
                if existing.last_used.is_none() { existing.last_used = old_last_used; }
                if existing.usage_count == 0 { existing.usage_count = old_usage; }
            } else {
                // Lower/equal priority — merge its fields into existing
                if existing.version.is_none() { existing.version = e.version; }
                if existing.path.is_none() { existing.path = e.path; }
                if existing.description.is_none() { existing.description = e.description; }
                if existing.install_date.is_none() { existing.install_date = e.install_date; }
                if existing.last_used.is_none() { existing.last_used = e.last_used; }
                if existing.usage_count == 0 { existing.usage_count = e.usage_count; }
            }
        } else {
            map.insert(key, e);
        }
    }

    let mut result: Vec<AppEntry> = map.into_values().collect();
    result.sort_by_key(|e| e.name.to_lowercase());
    result
}

/// Scan every source concurrently and return the deduplicated inventory.
///
/// Used by the in-TUI rescan, which reports progress with a single spinner
/// rather than per source. The cold-start path uses `scan_one` instead so it
/// can show each source finishing independently.
///
/// A source that fails is dropped, not propagated: a machine without Go or
/// gem installed is normal, and one absent toolchain must not void the scan.
pub async fn scan_all() -> Result<Vec<AppEntry>> {
    let results = futures::future::join_all(SOURCES.map(scan_one)).await;
    let entries = results.into_iter().flatten().flatten().collect();
    Ok(merge(entries))
}

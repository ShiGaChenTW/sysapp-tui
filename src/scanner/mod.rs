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
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    result
}

pub async fn scan_all() -> Result<Vec<AppEntry>> {
    eprintln!("Scanning installed applications...");

    let (r_brew, r_npm, r_pip, r_cargo, r_go, r_gem, r_pkgutil, r_apps) = tokio::join!(
        brew::scan(),
        npm::scan(),
        pip::scan(),
        cargo_scan::scan(),
        go::scan(),
        gem::scan(),
        pkgutil::scan(),
        applications::scan(),
    );

    let mut entries = Vec::new();

    for (label, result) in [
        ("brew", r_brew),
        ("npm", r_npm),
        ("pip", r_pip),
        ("cargo", r_cargo),
        ("go", r_go),
        ("gem", r_gem),
        ("pkgutil", r_pkgutil),
        ("apps", r_apps),
    ] {
        match result {
            Ok(items) => {
                eprintln!("  {label}: {} entries", items.len());
                entries.extend(items);
            }
            Err(e) => eprintln!("  {label}: skipped ({e})"),
        }
    }

    let pre_dedup = entries.len();
    entries = dedup(entries);
    let dupes = pre_dedup - entries.len();

    eprintln!("Total: {} entries ({} deduplicated)", entries.len(), dupes);
    Ok(entries)
}

//! Enrichment passes over the scanned inventory.
//!
//! Nothing here writes to stdout or stderr: the TUI holds the alternate
//! screen for the whole scan, so stray output would be painted over the
//! rendered frame. Progress is reported through `Message` instead.

mod language;
mod usage;

use crate::model::AppEntry;

/// Run all enrichment passes on scanned entries.
pub async fn enrich(entries: &mut [AppEntry]) {
    language::enrich_languages(entries).await;
    usage::enrich_usage(entries).await;
}

/// Owned wrapper for `enrich`, so it can be driven by a `Command::perform`
/// future that has to hand the data back to the update loop.
pub async fn enrich_owned(mut entries: Vec<crate::model::AppEntry>) -> Vec<crate::model::AppEntry> {
    enrich(&mut entries).await;
    entries
}

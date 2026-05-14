mod language;
mod usage;

use crate::model::AppEntry;

/// Run all enrichment passes on scanned entries.
pub async fn enrich(entries: &mut [AppEntry]) {
    eprintln!("  detecting languages...");
    language::enrich_languages(entries).await;
    eprintln!("  collecting usage data...");
    usage::enrich_usage(entries).await;
}

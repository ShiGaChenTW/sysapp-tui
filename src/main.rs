mod enricher;
mod model;
mod scanner;
mod tui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let mut entries = scanner::scan_all().await?;

    eprintln!("Scan complete: {} entries. Enriching...", entries.len());
    enricher::enrich(&mut entries).await;

    eprintln!("Enrichment complete. Launching TUI...");
    tui::run(entries).await?;

    Ok(())
}

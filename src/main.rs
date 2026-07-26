mod cache;
mod enricher;
mod model;
mod scanner;
mod tui;

use anyhow::Result;

const HELP: &str = "\
sysapp-tui — macOS system package scanner

USAGE:
    sysapp-tui [OPTIONS]

OPTIONS:
    -r, --refresh    Ignore the cached snapshot and rescan
    -h, --help       Print this help
    -V, --version    Print version

The inventory is cached after each scan, so subsequent launches open
immediately. Press `r` inside the TUI to refresh without restarting.
";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |long: &str, short: &str| args.iter().any(|a| a == long || a == short);

    if flag("--help", "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if flag("--version", "-V") {
        println!("sysapp-tui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // A cached snapshot opens in milliseconds; a full scan takes ~90 seconds,
    // almost all of it inside `brew info`. Always prefer the cache unless the
    // user explicitly asked for fresh data.
    let snapshot = if flag("--refresh", "-r") {
        None
    } else {
        cache::load()
    };

    match snapshot {
        Some(snapshot) => tui::run(snapshot.entries, Some(snapshot.generated_at)).await,
        None => {
            let entries = scan_and_cache().await?;
            tui::run(entries, None).await
        }
    }
}

/// Full scan + enrich, persisted for the next launch.
async fn scan_and_cache() -> Result<Vec<model::AppEntry>> {
    let mut entries = scanner::scan_all().await?;

    eprintln!("Scan complete: {} entries. Enriching...", entries.len());
    enricher::enrich(&mut entries).await;

    // A cache write failure must not stop the user from seeing their data —
    // it only costs them a slow launch next time.
    if let Err(e) = cache::save(&entries) {
        eprintln!("warning: could not write cache: {e:#}");
    }

    eprintln!("Enrichment complete. Launching TUI...");
    Ok(entries)
}

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

    // On a cold start the TUI opens immediately on a progress screen and runs
    // the scan behind it, so the terminal is never blank.
    tui::run(snapshot.map(|s| (s.entries, Some(s.generated_at)))).await
}

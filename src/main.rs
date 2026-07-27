mod cache;
mod config;
mod enricher;
mod exec;
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
    let mut snapshot = snapshot.map(|s| (s.entries, Some(s.generated_at)));
    let mut resume = None;

    // The interface runs in a loop rather than once, because a terminal
    // program cannot share the tty with it. Enter on a CLI or TUI unit exits
    // the runtime, we run the child here with the terminal restored, then come
    // back in. Suspending in place would mean reaching inside the `tears`
    // runtime; this needs nothing from it.
    loop {
        let Some(handover) = tui::run(snapshot.take(), resume.take()).await? else {
            return Ok(());
        };

        run_foreground(&handover);
        resume = Some(handover.resume);

        // Re-read rather than carrying the old inventory back in: the child
        // may have been an installer, and the snapshot on disk is never older
        // than the one we left with.
        snapshot = cache::load().map(|s| (s.entries, Some(s.generated_at)));
    }
}

/// Run a terminal program with the interface out of the way.
///
/// Failures are printed rather than returned: the interface is about to come
/// back up, and aborting the whole process because one launch failed would
/// throw away the session over something the user can simply not do again.
fn run_foreground(handover: &tui::Suspend) {
    use std::io::Write;

    println!("\n$ {}", handover.name);
    let _ = std::io::stdout().flush();

    // No shell. `program` is a resolved path and `args` is a vector, so a
    // package name containing `;`, a quote or a newline is one opaque argv
    // entry and cannot become a second command.
    let outcome = std::process::Command::new(&handover.program)
        .args(&handover.args)
        .status();

    match outcome {
        Ok(status) if status.success() => {}
        Ok(status) => println!("\n[{} exited with {status}]", handover.name),
        Err(e) => println!("\n[{} could not start: {e}]", handover.name),
    }

    // Without this the interface repaints instantly over whatever the child
    // printed, and the user never sees the output they asked for.
    print!("\n[press Enter to return]");
    let _ = std::io::stdout().flush();
    let _ = std::io::stdin().read_line(&mut String::new());
}

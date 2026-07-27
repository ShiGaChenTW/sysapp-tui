use crate::model::{AppEntry, Source};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Enrich entries with usage data.
///
/// CLI tools: parse `~/.zsh_history` for command frequency.
/// GUI apps: query `mdls` for kMDItemLastUsedDate and kMDItemUseCount.
pub async fn enrich_usage(entries: &mut [AppEntry]) {
    let history = parse_zsh_history().await;

    // Apply history counts to CLI entries
    for entry in entries.iter_mut() {
        if !matches!(entry.source, Source::Applications | Source::HomebrewCask)
            && let Some(&count) = history.get(&entry.name.to_lowercase())
        {
            entry.usage_count = count;
        }
    }

    // Batch mdls for GUI apps
    let app_paths: Vec<(usize, String)> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.source, Source::Applications | Source::HomebrewCask))
        .filter_map(|(i, e)| e.path.as_ref().map(|p| (i, p.clone())))
        .collect();

    let results = batch_mdls(&app_paths).await;
    for (idx, last_used, count) in results {
        if entries[idx].last_used.is_none() {
            entries[idx].last_used = last_used;
        }
        if entries[idx].usage_count == 0 {
            entries[idx].usage_count = count;
        }
    }
}

/// Parse `~/.zsh_history` and count how often each command appears.
async fn parse_zsh_history() -> HashMap<String, u32> {
    let mut counts: HashMap<String, u32> = HashMap::new();

    let home = dirs::home_dir().unwrap_or_default();
    let history_path = home.join(".zsh_history");

    let content = match tokio::fs::read(&history_path).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(_) => return counts,
    };

    for line in content.lines() {
        // zsh extended format: ": 1234567890:0;actual_command"
        let cmd = if let Some(rest) = line.strip_prefix(": ") {
            rest.split_once(';').map(|(_, c)| c).unwrap_or(rest)
        } else {
            line
        };

        // Extract first word (program name), strip path prefixes
        let program = cmd.split_whitespace().next().unwrap_or("");
        let program = program.rsplit('/').next().unwrap_or(program);

        // Handle `sudo <program>`
        let program = if program == "sudo" {
            cmd.split_whitespace()
                .nth(1)
                .unwrap_or("")
                .rsplit('/')
                .next()
                .unwrap_or("")
        } else {
            program
        };

        if !program.is_empty() {
            *counts.entry(program.to_lowercase()).or_default() += 1;
        }
    }

    counts
}

/// Query mdls concurrently for multiple app paths (max 30 parallel).
async fn batch_mdls(
    paths: &[(usize, String)],
) -> Vec<(usize, Option<DateTime<Local>>, u32)> {
    let semaphore = Arc::new(Semaphore::new(30));
    let mut set = JoinSet::new();

    for (idx, path) in paths {
        let sem = semaphore.clone();
        let path = path.clone();
        let idx = *idx;
        set.spawn(async move {
            // Semaphore is never closed, so this cannot fail; tolerate it anyway
        // rather than panicking a task and losing its result.
        let _permit = sem.acquire().await.ok();
            let (last_used, count) = query_mdls(&path).await;
            (idx, last_used, count)
        });
    }

    let mut results = Vec::new();
    // `join_next` yields `Some(Err(_))` for a panicked or cancelled task. A
    // `while let Some(Ok(..))` pattern would stop the loop there and silently
    // drop every task not yet joined — those entries would keep a zero usage
    // count and no last-used date, which makes `is_idle()` report actively
    // used applications as abandoned. Skip the failed task, keep draining.
    while let Some(joined) = set.join_next().await {
        // Nothing is written to stderr here: the TUI holds the alternate
        // screen for the whole scan, so any output would paint over the live
        // frame. A dropped task just leaves that one entry without usage data,
        // which is the same state as an `mdls` query returning nothing.
        if let Ok(result) = joined {
            results.push(result);
        }
    }
    results
}

/// Query a single app's metadata via `mdls`.
async fn query_mdls(app_path: &str) -> (Option<DateTime<Local>>, u32) {
    let output = Command::new("mdls")
        .args([
            "-name",
            "kMDItemLastUsedDate",
            "-name",
            "kMDItemUseCount",
            app_path,
        ])
        .output()
        .await;

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return (None, 0),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut last_used = None;
    let mut use_count = 0u32;

    for line in stdout.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("kMDItemLastUsedDate") {
            let val = val.trim().trim_start_matches('=').trim();
            if val != "(null)" {
                last_used = parse_mdls_date(val);
            }
        } else if let Some(val) = line.strip_prefix("kMDItemUseCount") {
            let val = val.trim().trim_start_matches('=').trim();
            if val != "(null)" {
                use_count = val.parse().unwrap_or(0);
            }
        }
    }

    (last_used, use_count)
}

/// Parse mdls date format: `2024-01-15 08:30:00 +0000`
fn parse_mdls_date(s: &str) -> Option<DateTime<Local>> {
    let s = s.trim().trim_matches('"');
    if let Ok(dt) = DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S %z") {
        return Some(dt.with_timezone(&Local));
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Local.from_local_datetime(&ndt).single();
    }
    None
}

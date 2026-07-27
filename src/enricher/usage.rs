use crate::model::{AppEntry, Source};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Enrich entries with usage data.
///
/// CLI history carries frequency and, when zsh extended history is enabled,
/// the freshest plausible invocation time. GUI metadata still comes from
/// `mdls`, which remains the better source when it already populated a field.
pub async fn enrich_usage(entries: &mut [AppEntry]) {
    let history = parse_zsh_history().await;

    // Apply history data to CLI entries.
    for entry in entries.iter_mut() {
        if !matches!(entry.source, Source::Applications | Source::HomebrewCask)
            && let Some(&(count, last_used)) = history.get(&entry.name.to_lowercase())
        {
            apply_history(entry, count, last_used);
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

/// History never overwrites a date another source already established.
///
/// `mdls` observes GUI launches that never touch a shell, so where both
/// sources have an opinion the Spotlight one is strictly better informed.
/// The count is unconditional: shell history is the only thing that counts
/// invocations at all.
fn apply_history(entry: &mut AppEntry, count: u32, last_used: Option<DateTime<Local>>) {
    entry.usage_count = count;
    if entry.last_used.is_none() {
        entry.last_used = last_used;
    }
}

/// History timestamps are optional because zsh only records them with
/// `EXTENDED_HISTORY`, but even a plain line still proves the command was run.
async fn parse_zsh_history() -> HashMap<String, (u32, Option<DateTime<Local>>)> {
    let home = dirs::home_dir().unwrap_or_default();
    let history_path = home.join(".zsh_history");

    let content = match tokio::fs::read(&history_path).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(_) => return HashMap::new(),
    };

    parse_history_content(&content)
}

fn parse_history_content(content: &str) -> HashMap<String, (u32, Option<DateTime<Local>>)> {
    const MIN_HISTORY_EPOCH: i64 = 946_684_800;

    let mut usage: HashMap<String, (u32, Option<DateTime<Local>>)> = HashMap::new();
    let now_epoch = Local::now().timestamp();

    for line in content.lines() {
        let (timestamp, cmd) = if let Some(rest) = line.strip_prefix(": ") {
            let timestamp = rest.split_once(':').and_then(|(epoch, _)| {
                let epoch = epoch.parse::<i64>().ok()?;
                if !(MIN_HISTORY_EPOCH..=now_epoch).contains(&epoch) {
                    return None;
                }
                DateTime::from_timestamp(epoch, 0).map(|dt| dt.with_timezone(&Local))
            });
            let cmd = rest.split_once(';').map(|(_, c)| c).unwrap_or(rest);
            (timestamp, cmd)
        } else {
            (None, line)
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
            let entry = usage.entry(program.to_lowercase()).or_insert((0, None));
            entry.0 += 1;

            // A bad epoch can come from a hand-edited or corrupted history
            // file. The command count is still real evidence of use, but an
            // impossible date would make dead tools look recently active.
            if let Some(timestamp) = timestamp
                && entry
                    .1
                    .as_ref()
                    .is_none_or(|current| timestamp > *current)
            {
                entry.1 = Some(timestamp);
            }
        }
    }

    usage
}

/// Query mdls concurrently for multiple app paths (max 30 parallel).
async fn batch_mdls(paths: &[(usize, String)]) -> Vec<(usize, Option<DateTime<Local>>, u32)> {
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

#[cfg(test)]
mod tests {
    use super::{apply_history, parse_history_content};
    use crate::model::{AppEntry, Source};
    use chrono::{DateTime, Local};

    fn local_from_epoch(epoch: i64) -> DateTime<Local> {
        DateTime::from_timestamp(epoch, 0)
            .unwrap()
            .with_timezone(&Local)
    }

    #[test]
    fn parse_history_content_tracks_counts_and_newest_valid_timestamps() {
        let older_rg = 1_753_600_002_i64;
        let newer_rg = older_rg + 10;
        let htop_epoch = 1_753_600_000_i64;
        let nginx_epoch = 1_753_600_001_i64;
        let pre_2000_epoch = 946_684_799_i64;
        let future_epoch = Local::now().timestamp() + 86_400;
        let content = format!(
            "vim notes.txt\n: {htop_epoch}:0;htop\n: {nginx_epoch}:0;sudo nginx\n: {older_rg}:0;/usr/local/bin/rg pattern\n: notanumber:0;btop\n: {newer_rg}:0;rg newer\n: {pre_2000_epoch}:0;oldtool\n: {future_epoch}:0;futuretool\n: {older_rg}:0;rg older\n"
        );

        let history = parse_history_content(&content);

        assert_eq!(history["vim"], (1, None));
        assert_eq!(history["htop"], (1, Some(local_from_epoch(htop_epoch))));
        assert_eq!(history["nginx"], (1, Some(local_from_epoch(nginx_epoch))));
        assert_eq!(history["rg"], (3, Some(local_from_epoch(newer_rg))));
        assert_eq!(history["btop"], (1, None));
        assert_eq!(history["oldtool"], (1, None));
        assert_eq!(history["futuretool"], (1, None));
    }

    fn cli_entry() -> AppEntry {
        AppEntry {
            name: "rg".into(),
            version: None,
            source: Source::Homebrew,
            language: None,
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: None,
            description: None,
            ui_kind: None,
            category: None,
        }
    }

    /// The whole point of parsing epochs: a CLI tool that was never eligible
    /// for a `last_used` date now gets one.
    #[test]
    fn history_fills_a_missing_last_used() {
        let mut entry = cli_entry();
        let seen = local_from_epoch(1_753_600_200);

        apply_history(&mut entry, 7, Some(seen));

        assert_eq!(entry.last_used, Some(seen));
        assert_eq!(entry.usage_count, 7);
    }

    /// `mdls` sees launches the shell never records, so a date it already
    /// established must survive the history pass.
    #[test]
    fn history_does_not_overwrite_an_existing_last_used() {
        let mut entry = cli_entry();
        let from_mdls = local_from_epoch(1_753_600_100);
        entry.last_used = Some(from_mdls);

        apply_history(&mut entry, 3, Some(local_from_epoch(1_753_600_200)));

        assert_eq!(entry.last_used, Some(from_mdls), "mdls data was clobbered");
        assert_eq!(entry.usage_count, 3, "count is unconditional");
    }

    /// A timestamp-less history (no `EXTENDED_HISTORY`) still contributes its
    /// count without inventing a date.
    #[test]
    fn history_without_timestamps_still_counts() {
        let mut entry = cli_entry();

        apply_history(&mut entry, 4, None);

        assert_eq!(entry.usage_count, 4);
        assert_eq!(entry.last_used, None);
    }
}

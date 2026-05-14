use crate::model::{AppEntry, Language, Source};
use anyhow::{Context, Result};
use tokio::process::Command;

pub async fn scan() -> Result<Vec<AppEntry>> {
    let output = Command::new("gem")
        .args(["list", "--local"])
        .output()
        .await
        .context("failed to run gem")?;

    if !output.status.success() {
        anyhow::bail!("gem exited with {}", output.status);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("***") {
            continue;
        }

        // Format: "name (version1, version2, ...)"
        let (name, version) = if let Some(paren) = line.find('(') {
            let n = line[..paren].trim();
            let v = line[paren + 1..].trim_end_matches(')').trim();
            // Take first (latest) version
            let first = v.split(',').next().unwrap_or("").trim();
            (n.to_string(), Some(first.to_string()))
        } else {
            (line.to_string(), None)
        };

        entries.push(AppEntry {
            name,
            version,
            source: Source::Gem,
            language: Some(Language::Ruby),
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: None,
            description: None,
        });
    }

    Ok(entries)
}

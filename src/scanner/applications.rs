use crate::model::{AppEntry, Source};
use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use serde::Deserialize;
use tokio::process::Command;

#[derive(Deserialize)]
struct SysProfileOutput {
    #[serde(rename = "SPApplicationsDataType")]
    apps: Vec<AppInfo>,
}

#[derive(Deserialize)]
struct AppInfo {
    _name: String,
    version: Option<String>,
    path: Option<String>,
    #[serde(rename = "lastModified")]
    last_modified: Option<String>,
    obtained_from: Option<String>,
}

pub async fn scan() -> Result<Vec<AppEntry>> {
    let output = Command::new("system_profiler")
        .args(["SPApplicationsDataType", "-json"])
        .output()
        .await
        .context("failed to run system_profiler")?;

    if !output.status.success() {
        anyhow::bail!("system_profiler exited with {}", output.status);
    }

    let profile: SysProfileOutput =
        serde_json::from_slice(&output.stdout).context("failed to parse system_profiler JSON")?;

    let entries = profile
        .apps
        .iter()
        .map(|app| {
            let install_date = app.last_modified.as_deref().and_then(parse_profiler_date);

            let desc = app.obtained_from.clone();

            AppEntry {
                name: app._name.clone(),
                version: app.version.clone(),
                source: Source::Applications,
                language: None,
                install_date,
                last_used: None,
                usage_count: 0,
                path: app.path.clone(),
                description: desc,
                ui_kind: None,
                category: None,
            }
        })
        .collect();

    Ok(entries)
}

/// Parse the date string from system_profiler, e.g. "2024-03-15T10:30:00Z"
fn parse_profiler_date(s: &str) -> Option<DateTime<Local>> {
    // Try ISO 8601 with Z suffix
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Local));
    }
    // Try without timezone (assume local)
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Local.from_local_datetime(&ndt).single();
    }
    None
}

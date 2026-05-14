use crate::model::{AppEntry, Language, Source};
use anyhow::Result;
use chrono::{DateTime, Local};
use std::path::PathBuf;

pub async fn scan() -> Result<Vec<AppEntry>> {
    let bin_dir = dirs::home_dir()
        .map(|h| h.join("go/bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/go/bin"));

    if !bin_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();

    let mut reader = tokio::fs::read_dir(&bin_dir).await?;
    while let Some(entry) = reader.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();

        if name.starts_with('.') {
            continue;
        }

        let meta = entry.metadata().await.ok();
        let install_date: Option<DateTime<Local>> = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(|t| t.into());

        entries.push(AppEntry {
            name,
            version: None,
            source: Source::Go,
            language: Some(Language::Go),
            install_date,
            last_used: None,
            usage_count: 0,
            path: Some(entry.path().to_string_lossy().into_owned()),
            description: None,
        });
    }

    Ok(entries)
}

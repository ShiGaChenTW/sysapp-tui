use crate::model::{AppEntry, Language, Source};
use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::process::Command;

#[derive(Deserialize)]
struct PipPkg {
    name: String,
    version: String,
}

pub async fn scan() -> Result<Vec<AppEntry>> {
    let output = Command::new("pip3")
        .args(["list", "--format=json"])
        .output()
        .await
        .context("failed to run pip3")?;

    if !output.status.success() {
        anyhow::bail!("pip3 exited with {}", output.status);
    }

    let pkgs: Vec<PipPkg> =
        serde_json::from_slice(&output.stdout).context("failed to parse pip JSON")?;

    let entries = pkgs
        .iter()
        .map(|p| AppEntry {
            name: p.name.clone(),
            version: Some(p.version.clone()),
            source: Source::Pip,
            language: Some(Language::Python),
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: None,
            description: None,
            ui_kind: None,
            category: None,
        })
        .collect();

    Ok(entries)
}

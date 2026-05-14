use crate::model::{AppEntry, Source};
use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::process::Command;

#[derive(Deserialize)]
struct BrewOutput {
    #[serde(default)]
    formulae: Vec<Formula>,
    #[serde(default)]
    casks: Vec<Cask>,
}

#[derive(Deserialize)]
struct Formula {
    name: String,
    desc: Option<String>,
    #[serde(default)]
    installed: Vec<InstalledVersion>,
}

#[derive(Deserialize)]
struct InstalledVersion {
    version: String,
}

#[derive(Deserialize)]
struct Cask {
    token: String,
    desc: Option<String>,
    version: Option<String>,
}

pub async fn scan() -> Result<Vec<AppEntry>> {
    let output = Command::new("brew")
        .args(["info", "--json=v2", "--installed"])
        .output()
        .await
        .context("failed to run brew")?;

    if !output.status.success() {
        anyhow::bail!("brew exited with {}", output.status);
    }

    let info: BrewOutput =
        serde_json::from_slice(&output.stdout).context("failed to parse brew JSON")?;

    let mut entries = Vec::new();

    for f in &info.formulae {
        let version = f.installed.first().map(|i| i.version.clone());
        entries.push(AppEntry {
            name: f.name.clone(),
            version,
            source: Source::Homebrew,
            language: None,
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: None,
            description: f.desc.clone(),
        });
    }

    for c in &info.casks {
        entries.push(AppEntry {
            name: c.token.clone(),
            version: c.version.clone(),
            source: Source::HomebrewCask,
            language: None,
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: None,
            description: c.desc.clone(),
        });
    }

    Ok(entries)
}

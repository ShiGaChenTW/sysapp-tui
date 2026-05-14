use crate::model::{AppEntry, Language, Source};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use tokio::process::Command;

#[derive(Deserialize)]
struct NpmList {
    #[serde(default)]
    dependencies: HashMap<String, NpmPkg>,
}

#[derive(Deserialize)]
struct NpmPkg {
    version: Option<String>,
}

pub async fn scan() -> Result<Vec<AppEntry>> {
    let output = Command::new("npm")
        .args(["list", "-g", "--json", "--depth=0"])
        .output()
        .await
        .context("failed to run npm")?;

    // npm list exits non-zero on peer dep warnings; still parse stdout
    let list: NpmList =
        serde_json::from_slice(&output.stdout).context("failed to parse npm JSON")?;

    let entries = list
        .dependencies
        .iter()
        .map(|(name, pkg)| AppEntry {
            name: name.clone(),
            version: pkg.version.clone(),
            source: Source::Npm,
            language: Some(Language::JavaScript),
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: None,
            description: None,
        })
        .collect();

    Ok(entries)
}

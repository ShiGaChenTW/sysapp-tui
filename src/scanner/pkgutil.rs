use crate::model::{AppEntry, Source};
use anyhow::{Context, Result};
use tokio::process::Command;

pub async fn scan() -> Result<Vec<AppEntry>> {
    let output = Command::new("pkgutil")
        .args(["--pkgs"])
        .output()
        .await
        .context("failed to run pkgutil")?;

    if !output.status.success() {
        anyhow::bail!("pkgutil exited with {}", output.status);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|pkg_id| {
            // Extract a readable name from reverse-DNS package ID
            // e.g. "com.apple.pkg.CLTools_Executables" -> "CLTools_Executables"
            let name = pkg_id
                .rsplit('.')
                .next()
                .unwrap_or(pkg_id)
                .to_string();

            AppEntry {
                name,
                version: None,
                source: Source::Pkgutil,
                language: None,
                install_date: None,
                last_used: None,
                usage_count: 0,
                path: None,
                description: Some(pkg_id.trim().to_string()),
            }
        })
        .collect();

    Ok(entries)
}

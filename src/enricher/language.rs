use crate::model::{AppEntry, Language, Source};
use std::path::Path;
use tokio::process::Command;

/// Enrich entries with language detection.
///
/// Phase 1: instant source-based detection (cargo→Rust, pip→Python, etc.)
/// Phase 2: batch `file` command for Homebrew binaries
/// Phase 3: framework inspection for .app bundles
pub async fn enrich_languages(entries: &mut [AppEntry]) {
    // Phase 1: source-based (instant, no I/O)
    for entry in entries.iter_mut() {
        if entry.language.is_some() {
            continue;
        }
        entry.language = match entry.source {
            Source::Cargo => Some(Language::Rust),
            Source::Go => Some(Language::Go),
            Source::Pip => Some(Language::Python),
            Source::Npm => Some(Language::JavaScript),
            Source::Gem => Some(Language::Ruby),
            _ => None,
        };
    }

    // Phase 2: batch `file` on Homebrew binaries
    detect_brew_languages(entries).await;

    // Phase 3: framework inspection for .app bundles
    for entry in entries.iter_mut() {
        if entry.language.is_some() {
            continue;
        }
        if matches!(entry.source, Source::Applications | Source::HomebrewCask) {
            entry.language = detect_app_language(entry.path.as_deref()).await;
        }
    }
}

/// Batch-detect languages for Homebrew formulae by running `file` on their binaries.
async fn detect_brew_languages(entries: &mut [AppEntry]) {
    let candidates: Vec<(usize, String)> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.language.is_none() && e.source == Source::Homebrew)
        .filter_map(|(i, e)| {
            let bin = format!("/opt/homebrew/bin/{}", e.name);
            if Path::new(&bin).exists() {
                return Some((i, bin));
            }
            let sbin = format!("/opt/homebrew/sbin/{}", e.name);
            if Path::new(&sbin).exists() {
                return Some((i, sbin));
            }
            None
        })
        .collect();

    if candidates.is_empty() {
        return;
    }

    for chunk in candidates.chunks(200) {
        let paths: Vec<&str> = chunk.iter().map(|(_, p)| p.as_str()).collect();
        let Ok(output) = Command::new("file").args(&paths).output().await else {
            continue;
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let Some((path_part, desc)) = line.split_once(": ") else {
                continue;
            };
            let path_part = path_part.trim();
            for (idx, p) in chunk.iter() {
                if p == path_part {
                    entries[*idx].language = Some(parse_file_output(desc));
                    break;
                }
            }
        }
    }
}

/// Parse `file` command output to determine language.
fn parse_file_output(desc: &str) -> Language {
    let d = desc.to_lowercase();
    if d.contains("python") {
        Language::Python
    } else if d.contains("ruby") {
        Language::Ruby
    } else     if d.contains("shell script")
        || d.contains("/bin/sh")
        || d.contains("/bin/bash")
        || d.contains("/bin/zsh")
        || d.contains("perl")
    {
        Language::Shell
    } else if d.contains("mach-o") {
        Language::C // default for compiled Homebrew formulae
    } else {
        Language::Unknown
    }
}

/// Detect language for a macOS .app bundle by inspecting its frameworks.
async fn detect_app_language(path: Option<&str>) -> Option<Language> {
    let path = path?;
    let app = Path::new(path);

    // Electron
    if app
        .join("Contents/Frameworks/Electron Framework.framework")
        .exists()
    {
        return Some(Language::Electron);
    }

    // Swift (bundled runtime)
    let frameworks = app.join("Contents/Frameworks");
    if frameworks.exists() {
        if let Ok(mut dir) = tokio::fs::read_dir(&frameworks).await {
            while let Ok(Some(entry)) = dir.next_entry().await {
                let name = entry.file_name();
                let s = name.to_string_lossy();
                if s.starts_with("libswift") && s.ends_with(".dylib") {
                    return Some(Language::Swift);
                }
            }
        }
    }

    // Java (JetBrains, Eclipse)
    if app.join("Contents/runtime/Contents/Home").exists()
        || app.join("Contents/Eclipse").exists()
    {
        return Some(Language::Java);
    }

    // Default: native macOS (Obj-C or Swift without bundled runtime)
    Some(Language::ObjC)
}

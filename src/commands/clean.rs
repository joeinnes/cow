use anyhow::Result;
use std::path::Path;

use crate::cli::CleanArgs;
use crate::state::State;

const BUILD_ARTIFACT_DIRS: &[&str] = &["target", ".build", "DerivedData", ".turbo"];

pub fn run(args: CleanArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    let pastures: Vec<_> = if let Some(ref name) = args.name {
        let matched: Vec<_> = state.pastures.iter()
            .filter(|p| p.name == *name || p.name.ends_with(&format!("/{}", name)))
            .collect();
        if matched.is_empty() {
            anyhow::bail!("No pasture found matching '{}'.", name);
        }
        matched
    } else {
        state.pastures.iter().collect()
    };

    let mut total_freed: u64 = 0;
    let mut any_found = false;

    for pasture in &pastures {
        let mut pasture_freed: u64 = 0;
        let mut found_dirs: Vec<String> = Vec::new();

        for dir_name in BUILD_ARTIFACT_DIRS {
            let artifact_path = pasture.path.join(dir_name);
            if !artifact_path.is_dir() {
                continue;
            }

            let bytes = du_bytes(&artifact_path);
            found_dirs.push(format!("{}/  ({})", dir_name, fmt(bytes)));
            pasture_freed += bytes;
        }

        if found_dirs.is_empty() {
            continue;
        }
        any_found = true;

        println!("  {} [{}]", pasture.name, fmt(pasture_freed));
        for d in &found_dirs {
            println!("    {}", d);
        }

        if args.dry_run {
            continue;
        }

        for dir_name in BUILD_ARTIFACT_DIRS {
            let artifact_path = pasture.path.join(dir_name);
            if artifact_path.is_dir() {
                std::fs::remove_dir_all(&artifact_path)?;
            }
        }

        total_freed += pasture_freed;
    }

    if !any_found {
        println!("No build artifacts found in pastures.");
        return Ok(());
    }

    if args.dry_run {
        let preview_total: u64 = pastures.iter().map(|p| {
            BUILD_ARTIFACT_DIRS.iter().map(|d| du_bytes(&p.path.join(d))).sum::<u64>()
        }).sum();
        println!();
        println!("Dry run — would free {}.", fmt(preview_total));
    } else {
        println!();
        println!("Freed {}.", fmt(total_freed));
    }

    Ok(())
}

fn du_bytes(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let Ok(out) = std::process::Command::new("du")
        .args(["-sk", path.to_str().unwrap_or("")])
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return 0;
    };
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        * 1024
}

fn fmt(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

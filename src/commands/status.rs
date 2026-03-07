use anyhow::{Context, Result};

use crate::{cli::StatusArgs, state::State, vcs::{self, Vcs}};

pub fn run(args: StatusArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    let name = resolve_name(args.name, &state)?;

    let entry = state
        .get(&name)
        .cloned()
        .with_context(|| format!("Workspace '{}' not found.", name))?;

    let (is_dirty, modified_files) = match entry.vcs {
        Vcs::Git => {
            let dirty = vcs::git_is_dirty(&entry.path);
            let files = if dirty { vcs::git_status_short(&entry.path) } else { String::new() };
            (dirty, files)
        }
        // tarpaulin-ignore-start
        Vcs::Jj => {
            let dirty = vcs::jj_is_dirty(&entry.path);
            let files = if dirty { vcs::jj_diff_summary(&entry.path) } else { String::new() };
            (dirty, files)
        }
        // tarpaulin-ignore-end
    };

    let status_str = if is_dirty { "dirty" } else { "clean" };

    // Current branch (may differ from stored branch)
    let branch = match entry.vcs {
        Vcs::Git => vcs::git_current_branch(&entry.path)
            .unwrap_or_else(|| entry.branch.clone().unwrap_or_else(|| "-".to_string())),
        // tarpaulin-ignore-start
        Vcs::Jj => entry.branch.clone().unwrap_or_else(|| "-".to_string()),
        // tarpaulin-ignore-end
    };

    let disk_delta = estimate_disk_delta(&entry);

    println!("Workspace:  {}", entry.name);
    println!("Source:     {}", entry.source.display());
    println!("Branch:     {}", branch);
    println!("VCS:        {}", entry.vcs);
    println!("Status:     {}", status_str);
    println!(
        "Created:    {}",
        entry.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );

    if let Some(bytes) = disk_delta {
        println!("Disk delta: {} (estimated from modified file sizes)", format_bytes(bytes));
    }

    if !modified_files.trim().is_empty() {
        println!("\nModified files:");
        for line in modified_files.lines() {
            println!("  {}", line);
        }
    }

    Ok(())
}

fn resolve_name(name: Option<String>, state: &State) -> Result<String> {
    if let Some(n) = name {
        return Ok(n);
    }
    // Detect from cwd, canonicalising both sides so /var → /private/var symlinks don't break matching.
    let cwd = std::env::current_dir().context("Cannot determine current directory")?;
    let cwd = cwd.canonicalize().unwrap_or(cwd);
    state
        .workspaces
        .iter()
        .find(|w| {
            let wp = w.path.canonicalize().unwrap_or_else(|_| w.path.clone());
            cwd.starts_with(&wp)
        })
        .map(|w| w.name.clone())
        .context(
            "Not in a cow workspace. Specify a workspace name or run from inside a workspace.",
        )
}

fn estimate_disk_delta(entry: &crate::state::WorkspaceEntry) -> Option<u64> {
    if !entry.path.exists() {
        return None;
    }
    match entry.vcs {
        Vcs::Git => {
            let output = std::process::Command::new("git")
                .args(["diff", "--name-only", "HEAD"])
                .current_dir(&entry.path)
                .output()
                .ok()?;
            let files = String::from_utf8_lossy(&output.stdout);
            let total: u64 = files
                .lines()
                .filter_map(|f| std::fs::metadata(entry.path.join(f)).ok())
                .map(|m| m.len())
                .sum();
            Some(total)
        }
        // tarpaulin-ignore-start
        Vcs::Jj => None,
        // tarpaulin-ignore-end
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_under_1kb() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(2048), "2.0 KB");
    }

    #[test]
    fn format_bytes_megabytes() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
    }
}

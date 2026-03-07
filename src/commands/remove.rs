use anyhow::{bail, Result};
use colored::Colorize;
use dialoguer::Confirm;
use std::path::Path;

use crate::{cli::RemoveArgs, state::State, vcs::{self, Vcs}};

pub fn run(args: RemoveArgs) -> Result<()> {
    if !args.all && args.names.is_empty() {
        bail!("Specify one or more workspace names, or use --all.");
    }

    let mut state = State::load()?;
    state.prune_deleted();

    // Collect names to remove
    let names: Vec<String> = if args.all {
        let mut all: Vec<String> = state.workspaces.iter().map(|w| w.name.clone()).collect();
        if let Some(ref source) = args.source {
            let canonical = source
                .canonicalize()
                .unwrap_or_else(|_| source.to_path_buf());
            all.retain(|name| {
                state
                    .workspaces
                    .iter()
                    .find(|w| w.name == *name)
                    .map(|w| w.source == canonical)
                    .unwrap_or(false)
            });
        }
        all
    } else {
        args.names.clone()
    };

    if names.is_empty() {
        println!("No workspaces to remove.");
        return Ok(());
    }

    let mut removed = 0usize;

    for name in &names {
        let Some(entry) = state.get(name).cloned() else {
            eprintln!("Workspace '{}' not found — skipping.", name);
            continue;
        };

        // tarpaulin-ignore-start
        if !entry.path.exists() {
            // Already gone; just prune from state
            state.remove(name);
            continue;
        }
        // tarpaulin-ignore-end

        let confirmed = match entry.vcs {
            Vcs::Git => {
                if vcs::git_is_dirty(&entry.path) && !args.force {
                    let short = vcs::git_status_short(&entry.path);
                    eprintln!(
                        "{} Workspace '{}' has uncommitted changes:",
                        "⚠".yellow(),
                        name
                    );
                    for line in short.lines() {
                        eprintln!("  {}", line);
                    }
                    confirm_or_default("Remove anyway? Changes will be lost.")?
                } else {
                    true
                }
            }

            // tarpaulin-ignore-start
            Vcs::Jj => {
                if vcs::jj_is_dirty(&entry.path) {
                    eprintln!(
                        "Note: workspace '{}' has modifications. \
                         These are preserved in the jj operation log of the source repo.",
                        name
                    );
                }
                if args.force {
                    true
                } else {
                    confirm_or_default(&format!("Remove workspace '{}'?", name))?
                }
            }
            // tarpaulin-ignore-end
        };

        if confirmed {
            // For git workspaces, warn about unpushed commits and optionally
            // offer to push before the workspace is deleted.
            if entry.vcs == Vcs::Git && vcs::git_has_unpushed_commits(&entry.path) {
                if args.force {
                    eprintln!(
                        "Warning: workspace '{}' has unpushed commits that will be lost.",
                        name
                    );
                } else {
                    let pushed = offer_push(name, &entry.path)?;
                    if !pushed {
                        eprintln!(
                            "Warning: workspace '{}' has unpushed commits that will be lost.",
                            name
                        );
                    }
                }
            }

            std::fs::remove_dir_all(&entry.path)?;
            state.remove(name);
            println!("Removed workspace '{}'", name);
            removed += 1;
        }
    }

    state.save()?;

    if removed == 0 && !names.is_empty() {
        println!("No workspaces were removed.");
    }

    Ok(())
}

/// On TTY: prompt the user to push, attempt the push if they say yes.
/// On non-TTY: returns false immediately (caller will print the warning).
fn offer_push(name: &str, path: &Path) -> Result<bool> {
    let prompt = format!(
        "Workspace '{}' has unpushed commits. Push to origin before removing?",
        name
    );
    match Confirm::new().with_prompt(&prompt).default(false).interact_opt() {
        Ok(Some(true)) => {
            let branch = vcs::git_current_branch(path).unwrap_or_default();
            let status = std::process::Command::new("git")
                .args(["push", "--set-upstream", "origin", &branch])
                .current_dir(path)
                .status();
            match status {
                Ok(s) if s.success() => {
                    println!("Pushed '{}' to origin/{branch}.", name);
                    Ok(true)
                }
                _ => {
                    eprintln!("Warning: push failed. Proceeding with removal.");
                    Ok(false)
                }
            }
        }
        // Non-TTY (Err) or user declined: caller handles warning.
        Ok(None) | Ok(Some(false)) | Err(_) => Ok(false),
    }
}

/// Show a yes/no prompt. Returns false if stdin is not a TTY.
fn confirm_or_default(prompt: &str) -> Result<bool> {
    match Confirm::new().with_prompt(prompt).default(false).interact_opt() {
        Ok(Some(answer)) => Ok(answer),
        Ok(None) | Err(_) => {
            eprintln!("(Not a TTY — defaulting to no.)");
            Ok(false)
        }
    }
}

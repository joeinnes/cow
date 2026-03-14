use anyhow::{bail, Context, Result};
use colored::Colorize;
use dialoguer::Confirm;
use std::path::Path;

use crate::{cli::RemoveArgs, state::{PastureEntry, State}, vcs::{self, Vcs}};

pub fn run(args: RemoveArgs) -> Result<()> {
    if !args.all && args.names.is_empty() {
        bail!("Specify one or more pasture names, or use --all.");
    }

    let mut state = State::load()?;
    state.prune_deleted();

    // Collect names to remove
    let names: Vec<String> = if args.all {
        let mut all: Vec<String> = state.pastures.iter().map(|w| w.name.clone()).collect();
        if let Some(ref source) = args.source {
            let canonical = source
                .canonicalize()
                .unwrap_or_else(|_| source.to_path_buf());
            all.retain(|name| {
                state
                    .pastures
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
        println!("No pastures to remove.");
        return Ok(());
    }

    let mut removed = 0usize;

    for name in &names {
        let Some(entry) = state.get(name).cloned() else {
            eprintln!("Pasture '{}' not found — skipping.", name);
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
                        "{} Pasture '{}' has uncommitted changes:",
                        "⚠".yellow(),
                        name
                    );
                    for line in short.lines() {
                        eprintln!("  {}", line);
                    }
                    if args.yes {
                        true
                    } else {
                        confirm_or_default("Remove anyway? Changes will be lost.")?
                    }
                } else {
                    true
                }
            }

            // tarpaulin-ignore-start
            Vcs::Jj => {
                if vcs::jj_is_dirty(&entry.path) {
                    eprintln!(
                        "Note: pasture '{}' has modifications. \
                         These are preserved in the jj operation log of the source repo.",
                        name
                    );
                }
                if args.force || args.yes {
                    true
                } else {
                    confirm_or_default(&format!("Remove pasture '{}'?", name))?
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
                        "Warning: pasture '{}' has unpushed commits that will be lost.",
                        name
                    );
                } else {
                    let pushed = offer_push(name, &entry.path)?;
                    if !pushed {
                        eprintln!(
                            "Warning: pasture '{}' has unpushed commits that will be lost.",
                            name
                        );
                    }
                }
            }

            remove_pasture_dir(&entry, args.force)?;
            state.remove(name);
            println!("🐄 Removed pasture '{}'", name);
            removed += 1;
        }
    }

    state.save()?;

    if removed == 0 && !names.is_empty() {
        println!("No pastures were removed.");
    }

    Ok(())
}

/// On TTY: prompt the user to push, attempt the push if they say yes.
/// On non-TTY: returns false immediately (caller will print the warning).
fn offer_push(name: &str, path: &Path) -> Result<bool> {
    let prompt = format!(
        "Pasture '{}' has unpushed commits. Push to origin before removing?",
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

/// Remove the pasture directory. For git linked worktrees, runs
/// `git worktree remove` so the source repo's back-link is cleaned up.
/// For regular clones (including those with symlinked dirs), uses
/// `remove_dir_all` — symlinks are removed, not followed, so the source
/// repo's files are untouched.
fn remove_pasture_dir(entry: &PastureEntry, force: bool) -> Result<()> {
    if entry.is_worktree {
        // Remove cow-internal files before calling git worktree remove.
        // git treats any untracked file (including .cow-context, which is
        // excluded from tracking but still present on disk) as "untracked",
        // causing worktree remove to fail without --force.
        let _ = std::fs::remove_file(entry.path.join(".cow-context"));

        let mut wt_args = vec!["worktree", "remove"];
        if force { wt_args.push("--force"); }
        let path_str = entry.path.to_str().unwrap_or_default();
        wt_args.push(path_str);
        let status = std::process::Command::new("git")
            .args(&wt_args)
            .current_dir(&entry.source)
            .status()
            .context("Failed to run git worktree remove")?;
        if !status.success() {
            bail!(
                "git worktree remove failed for '{}'. \
                 Use --force to remove an unclean worktree.",
                entry.path.display()
            );
        }
    } else {
        std::fs::remove_dir_all(&entry.path)
            .with_context(|| format!("Failed to remove '{}'", entry.path.display()))?;
    }
    Ok(())
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

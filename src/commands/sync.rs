use anyhow::{bail, Context, Result};
use std::process::Command;

use crate::{cli::SyncArgs, state::State, vcs::{self, Vcs}};

pub fn run(args: SyncArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    let name = resolve_name(args.name, &state)?;

    let entry = state
        .get(&name)
        .cloned()
        .with_context(|| format!("Workspace '{}' not found.", name))?;

    // Extract source_branch before the vcs split to avoid partial-move issues.
    let source_branch_opt = args.source_branch;

    if entry.vcs == Vcs::Jj {
        if vcs::jj_is_dirty(&entry.path) {
            bail!(
                "Workspace '{}' has uncommitted changes. Describe or abandon them before syncing.",
                name
            );
        }

        let source_branch = source_branch_opt
            .context("Cannot determine jj workspace branch; pass a source branch explicitly.")?;

        let remote_name = format!("_cow_sync_{}", name.replace('/', "-"));

        let add_status = Command::new("jj")
            .args(["git", "remote", "add", &remote_name, entry.source.to_str().unwrap()])
            .current_dir(&entry.path)
            .status()
            .context("Failed to add jj sync remote")?;
        if !add_status.success() {
            bail!("Failed to register source as a remote in jj workspace '{}'.", name);
        }

        let fetch_status = Command::new("jj")
            .args(["git", "fetch", "--remote", &remote_name])
            .current_dir(&entry.path)
            .status()
            .context("Failed to fetch from source")?;
        if !fetch_status.success() {
            let _ = Command::new("jj")
                .args(["git", "remote", "remove", &remote_name])
                .current_dir(&entry.path)
                .status();
            bail!("Failed to fetch from source repo for workspace '{}'.", name);
        }

        let tracking_ref = format!("{}@{}", source_branch, remote_name);
        let rebase_status = Command::new("jj")
            .args(["rebase", "-d", &tracking_ref])
            .current_dir(&entry.path)
            .status()
            .context("Failed to run jj rebase")?;

        let _ = Command::new("jj")
            .args(["git", "remote", "remove", &remote_name])
            .current_dir(&entry.path)
            .status();

        if !rebase_status.success() {
            bail!(
                "Failed to rebase jj workspace '{}' onto source/{}.",
                name, source_branch
            );
        }

        println!("Synced '{}' with {}/{}", name, "source", source_branch);
        return Ok(());
    }

    if vcs::git_is_dirty(&entry.path) {
        bail!(
            "Workspace '{}' has uncommitted changes. Stash or commit them before syncing.",
            name
        );
    }

    // Determine which branch in source to sync from.
    let source_branch = match source_branch_opt {
        Some(b) => b,
        None => vcs::git_current_branch(&entry.path)
            .context("Cannot determine workspace branch; pass a source branch explicitly.")?,
    };

    let remote_name = format!("_cow_sync_{}", name.replace('/', "-"));

    // Register source repo as a temporary remote in the workspace.
    let add_status = Command::new("git")
        .args(["remote", "add", &remote_name, entry.source.to_str().unwrap()])
        .current_dir(&entry.path)
        .status()
        .context("Failed to add temporary sync remote")?;
    if !add_status.success() {
        bail!("Failed to register source as a temporary remote in workspace '{}'.", name);
    }

    // Fetch the source branch into the workspace.
    let fetch_status = Command::new("git")
        .args(["fetch", &remote_name, &source_branch])
        .current_dir(&entry.path)
        .status()
        .context("Failed to fetch from source")?;

    if !fetch_status.success() {
        // Clean up before bailing.
        let _ = Command::new("git")
            .args(["remote", "remove", &remote_name])
            .current_dir(&entry.path)
            .status();
        bail!(
            "Failed to fetch branch '{}' from source repo.",
            source_branch
        );
    }

    let tracking_ref = format!("{}/{}", remote_name, source_branch);

    let integrate_status = if args.merge {
        Command::new("git")
            .args(["merge", &tracking_ref])
            .current_dir(&entry.path)
            .status()
            .context("Failed to run git merge")?
    } else {
        Command::new("git")
            .args(["rebase", &tracking_ref])
            .current_dir(&entry.path)
            .status()
            .context("Failed to run git rebase")?
    };

    // Always remove the temporary remote.
    let _ = Command::new("git")
        .args(["remote", "remove", &remote_name])
        .current_dir(&entry.path)
        .status();

    if !integrate_status.success() {
        if args.merge {
            // For merge, just report failure — no automatic abort needed.
            bail!(
                "Failed to merge '{}' with source/{}. Resolve conflicts manually.",
                name, source_branch
            );
        }

        // For rebase, detect whether we're in a conflict state and auto-abort.
        let rebase_dir = entry.path.join(".git").join("rebase-merge");
        if rebase_dir.exists() {
            // Collect conflicted files before aborting.
            let conflicted = Command::new("git")
                .args(["diff", "--name-only", "--diff-filter=U"])
                .current_dir(&entry.path)
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();

            // Auto-abort to leave the workspace in a clean state.
            let _ = Command::new("git")
                .args(["rebase", "--abort"])
                .current_dir(&entry.path)
                .status();

            if conflicted.is_empty() {
                bail!(
                    "Rebase conflict in workspace '{}' with source/{}. \
                     The rebase has been aborted. Try cow sync --merge, or resolve manually.",
                    name, source_branch
                );
            } else {
                bail!(
                    "Rebase conflict in workspace '{}' with source/{}.\n\
                     Conflicting files:\n{}\n\
                     The rebase has been aborted. Try cow sync --merge, or resolve manually.",
                    name, source_branch,
                    conflicted.lines().map(|l| format!("  {}", l)).collect::<Vec<_>>().join("\n")
                );
            }
        }

        bail!(
            "Failed to rebase workspace '{}' onto source/{}.",
            name, source_branch
        );
    }

    println!("Synced '{}' with {}/{}", name, "source", source_branch);
    Ok(())
}

fn resolve_name(name: Option<String>, state: &State) -> Result<String> {
    if let Some(n) = name {
        return Ok(n);
    }
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
            "Not in a cow workspace. Specify a workspace name with --name or run from inside a workspace.",
        )
}

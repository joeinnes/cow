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

    match entry.vcs {
        // tarpaulin-ignore-start
        Vcs::Jj => bail!("Sync is not yet supported for jj workspaces."),
        // tarpaulin-ignore-end
        Vcs::Git => {}
    }

    if vcs::git_is_dirty(&entry.path) {
        bail!(
            "Workspace '{}' has uncommitted changes. Stash or commit them before syncing.",
            name
        );
    }

    // Determine which branch in source to sync from.
    let source_branch = match args.source_branch {
        Some(b) => b,
        None => vcs::git_current_branch(&entry.path)
            .context("Cannot determine workspace branch; pass a source branch explicitly.")?,
    };

    let remote_name = format!("_cow_sync_{}", name);

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
        let strategy = if args.merge { "merge" } else { "rebase" };
        bail!(
            "Failed to {} workspace '{}' onto '{}/{}'. Resolve conflicts and complete manually.",
            strategy, name, "source", source_branch
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

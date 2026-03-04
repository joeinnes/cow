use anyhow::{bail, Context, Result};
use std::fs::File;
use std::process::Command;

use crate::{cli::ExtractArgs, state::State, vcs::Vcs};

pub fn run(args: ExtractArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    let entry = state
        .get(&args.name)
        .cloned()
        .with_context(|| format!("Pasture '{}' not found.", args.name))?;

    if args.patch.is_none() && args.branch.is_none() {
        bail!("Specify --patch <FILE> and/or --branch <NAME>.");
    }

    if let Some(patch_file) = args.patch {
        let file = File::create(&patch_file)
            .with_context(|| format!("Cannot create patch file: {}", patch_file.display()))?;

        let status = match entry.vcs {
            Vcs::Git => {
                // Use the SHA recorded at workspace creation as the base so the
                // patch covers all commits made in the workspace, not just the
                // last one. Fall back to HEAD~1 for workspaces created before
                // this field was added.
                let base = entry
                    .initial_commit
                    .as_deref()
                    .unwrap_or("HEAD~1")
                    .to_string();
                Command::new("git")
                    .args(["format-patch", &format!("{}..HEAD", base), "--stdout"])
                    .current_dir(&entry.path)
                    .stdout(file)
                    .status()
                    .context("Failed to run git format-patch")?
            }
            // tarpaulin-ignore-start
            Vcs::Jj => Command::new("jj")
                .args(["diff", "--git"])
                .current_dir(&entry.path)
                .stdout(file)
                .status()
                .context("Failed to run jj diff --git")?,
            // tarpaulin-ignore-end
        };

        if status.success() {
            println!("Patch written to {}", patch_file.display());
        } else {
            bail!("Patch command failed with status: {}", status);
        }
    }

    if let Some(branch_name) = args.branch {
        match entry.vcs {
            Vcs::Git => {
                let source = &entry.source;
                let remote_name = format!("cow-tmp-{}", args.name.replace('/', "-"));

                // Register the workspace as a temporary local remote in the source repo.
                let add_status = Command::new("git")
                    .args(["remote", "add", &remote_name, entry.path.to_str().unwrap()])
                    .current_dir(source)
                    .status()
                    .context("Failed to add temporary remote")?;
                if !add_status.success() {
                    bail!("Failed to register pasture as a temporary remote in source repo.");
                }

                // Fetch workspace HEAD into source as the named branch.
                let fetch_status = Command::new("git")
                    .args(["fetch", &remote_name, &format!("HEAD:{}", branch_name)])
                    .current_dir(source)
                    .status()
                    .context("Failed to fetch from workspace")?;

                // Always remove the temporary remote, even on failure.
                let _ = Command::new("git")
                    .args(["remote", "remove", &remote_name])
                    .current_dir(source)
                    .status();

                if !fetch_status.success() {
                    bail!("Failed to create branch '{}' in source repo.", branch_name);
                }

                println!(
                    "Branch '{}' created in source repo at {}",
                    branch_name,
                    source.display()
                );
            }
            Vcs::Jj => {
                // Export jj state to the git backend so HEAD reflects the
                // latest committed change.
                let export_status = Command::new("jj")
                    .args(["git", "export"])
                    .current_dir(&entry.path)
                    .status()
                    .context("Failed to run jj git export")?;
                if !export_status.success() {
                    bail!("Failed to export jj pasture state to git backend.");
                }

                // The source is a colocated jj+git repo — plain git remote
                // operations work on its .git directory.
                let source = &entry.source;
                let remote_name = format!("cow-tmp-{}", args.name.replace('/', "-"));

                let add_status = Command::new("git")
                    .args(["remote", "add", &remote_name, entry.path.to_str().unwrap()])
                    .current_dir(source)
                    .status()
                    .context("Failed to add temporary remote")?;
                if !add_status.success() {
                    bail!("Failed to register pasture as a temporary remote in source repo.");
                }

                let fetch_status = Command::new("git")
                    .args(["fetch", &remote_name, &format!("HEAD:{}", branch_name)])
                    .current_dir(source)
                    .status()
                    .context("Failed to fetch from workspace")?;

                let _ = Command::new("git")
                    .args(["remote", "remove", &remote_name])
                    .current_dir(source)
                    .status();

                if !fetch_status.success() {
                    bail!("Failed to create branch '{}' in source repo.", branch_name);
                }

                println!(
                    "Branch '{}' created in source repo at {}",
                    branch_name,
                    source.display()
                );
            }
        }
    }

    Ok(())
}

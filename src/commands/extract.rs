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
        .with_context(|| format!("Workspace '{}' not found.", args.name))?;

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
                let status = Command::new("git")
                    .args([
                        "push",
                        "origin",
                        &format!("HEAD:{}", branch_name),
                    ])
                    .current_dir(&entry.path)
                    .status()
                    .context("Failed to run git push")?;
                if !status.success() {
                    bail!("Failed to push to branch '{}' on origin.", branch_name);
                }
                println!("Pushed to origin/{}", branch_name);
            }
            // tarpaulin-ignore-start
            Vcs::Jj => bail!("Branch push is not yet supported for jj workspaces."),
            // tarpaulin-ignore-end
        }
    }

    Ok(())
}

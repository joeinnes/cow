use anyhow::{bail, Context, Result};

use crate::{
    cli::{CreateArgs, RecreateArgs},
    state::{self, State},
};

pub fn run(args: RecreateArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    let entry = state
        .get(&args.name)
        .cloned()
        .with_context(|| format!("Pasture '{}' not found.", args.name))?;

    if !entry.source.exists() {
        bail!(
            "Source '{}' no longer exists.",
            entry.source.display()
        );
    }

    // Determine branch for the fresh clone.
    let (branch, no_branch) = if args.no_branch {
        (None, true)
    } else if let Some(b) = args.branch.clone() {
        (Some(b), false)
    } else if let Some(b) = entry.branch.clone() {
        (Some(b), false)
    } else {
        (None, true)
    };

    println!("🐄 Recreating '{}' ...", args.name);

    // Remove the existing workspace directory.
    if entry.path.exists() {
        std::fs::remove_dir_all(&entry.path)
            .with_context(|| format!("Failed to remove '{}'", entry.path.display()))?;
    }
    state.remove(&args.name);
    state.save()?;

    // Compute --dir: only needed when the workspace lives outside the default
    // pasture directory.
    let default_pasture = state::default_pasture_dir()?;
    let expected_default = default_pasture.join(&entry.name);
    let dir_arg = if entry.path == expected_default {
        None
    } else {
        entry.path.parent().map(|p| p.to_path_buf())
    };

    super::create::run(CreateArgs {
        name: Some(entry.name.clone()),
        source: Some(entry.source.clone()),
        branch,
        no_branch,
        dir: dir_arg,
        no_clean: false,
        change: None,
        from: None,
        message: None,
        print_path: false,
        no_symlink: false,
        worktree: false,
    })
}

use anyhow::{bail, Context, Result};

use crate::{cli::DiffArgs, state::State, vcs::Vcs};

pub fn run(args: DiffArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    let name = resolve_name(args.name, &state)?;

    let entry = state
        .get(&name)
        .cloned()
        .with_context(|| format!("Workspace '{}' not found.", name))?;

    let status = match entry.vcs {
        Vcs::Git => std::process::Command::new("git")
            .arg("diff")
            .current_dir(&entry.path)
            .status()
            .context("Failed to run git diff")?,
        Vcs::Jj => std::process::Command::new("jj")
            .arg("diff")
            .current_dir(&entry.path)
            .status()
            .context("Failed to run jj diff")?,
    };

    if !status.success() {
        bail!("Diff command exited with status: {}", status);
    }

    Ok(())
}

fn resolve_name(name: Option<String>, state: &State) -> Result<String> {
    if let Some(n) = name {
        return Ok(n);
    }
    let cwd = std::env::current_dir().context("Cannot determine current directory")?;
    state
        .workspaces
        .iter()
        .find(|w| cwd.starts_with(&w.path))
        .map(|w| w.name.clone())
        .context(
            "Not in a swt workspace. Specify a workspace name or run from inside a workspace.",
        )
}

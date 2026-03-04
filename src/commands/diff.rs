use anyhow::{bail, Context, Result};

use crate::{cli::DiffArgs, state::State, vcs::Vcs};

pub fn run(args: DiffArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    let name = resolve_name(args.name, &state)?;

    let entry = state
        .get(&name)
        .cloned()
        .with_context(|| format!("Pasture '{}' not found.", name))?;

    let status = match entry.vcs {
        Vcs::Git => std::process::Command::new("git")
            .arg("diff")
            .current_dir(&entry.path)
            .status()
            .context("Failed to run git diff")?,
        // tarpaulin-ignore-start
        Vcs::Jj => std::process::Command::new("jj")
            .arg("diff")
            .current_dir(&entry.path)
            .status()
            .context("Failed to run jj diff")?,
        // tarpaulin-ignore-end
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
    let cwd = cwd.canonicalize().unwrap_or(cwd);
    state
        .pastures
        .iter()
        .find(|w| {
            let wp = w.path.canonicalize().unwrap_or_else(|_| w.path.clone());
            cwd.starts_with(&wp)
        })
        .map(|w| w.name.clone())
        .context(
            "Not in a cow pasture. Specify a pasture name or run from inside a pasture.",
        )
}

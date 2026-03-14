use anyhow::{bail, Context, Result};
use std::process::Command;

use crate::{cli::FetchFromArgs, state::State, vcs::Vcs};

pub fn run(args: FetchFromArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    // Resolve the current (destination) pasture from CWD or --name.
    let into_name = match args.name {
        Some(n) => n,
        None => {
            let cwd = std::env::current_dir().context("Cannot determine current directory")?;
            let cwd = cwd.canonicalize().unwrap_or(cwd);
            state
                .pastures
                .iter()
                .find(|p| {
                    let pp = p.path.canonicalize().unwrap_or_else(|_| p.path.clone());
                    cwd.starts_with(&pp)
                })
                .map(|p| p.name.clone())
                .context(
                    "Not inside a cow pasture. Run from inside a pasture or use --name.",
                )?
        }
    };

    let into_entry = state
        .get(&into_name)
        .cloned()
        .with_context(|| format!("Pasture '{}' not found.", into_name))?;

    if into_entry.vcs != Vcs::Git {
        bail!("cow fetch-from only supports git pastures.");
    }

    // Resolve the source (from) pasture.
    let from_entry = state
        .get(&args.from)
        .cloned()
        .with_context(|| format!("Pasture '{}' not found.", args.from))?;

    if from_entry.vcs != Vcs::Git {
        bail!("Source pasture '{}' is not a git pasture.", args.from);
    }

    // Guard against cross-project fetches unless --force.
    if into_entry.source != from_entry.source && !args.force {
        bail!(
            "Pastures '{}' and '{}' have different sources:\n  {}\n  {}\n\
             Use --force to fetch across projects.",
            into_name,
            args.from,
            into_entry.source.display(),
            from_entry.source.display()
        );
    }

    // Build a stable ref namespace from the source pasture name.
    // Replace '/' and '.' with '-', then strip leading non-alphanumeric
    // characters — git ref components cannot begin with '.' or '-'.
    let namespace: String = {
        let raw = args.from.replace(['/', '.'], "-");
        let trimmed = raw.trim_start_matches(|c: char| !c.is_alphanumeric());
        if trimmed.is_empty() { "pasture".to_string() } else { trimmed.to_string() }
    };
    let refspec = format!("refs/heads/*:refs/cow/{}/*", namespace);

    println!(
        "Fetching from pasture '{}' at {} ...",
        args.from,
        from_entry.path.display()
    );

    let status = Command::new("git")
        .args([
            "fetch",
            from_entry.path.to_str().unwrap(),
            &refspec,
        ])
        .current_dir(&into_entry.path)
        .status()
        .context("Failed to run git fetch")?;

    if !status.success() {
        bail!("git fetch failed.");
    }

    println!();
    println!("Refs stored under refs/cow/{}/", namespace);
    println!("To rebase: git rebase refs/cow/{}/<branch>", namespace);

    Ok(())
}

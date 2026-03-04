use anyhow::{Context, Result};

use crate::{cli::CdArgs, state::State};

pub fn run(args: CdArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    let entry = state
        .get(&args.name)
        .cloned()
        .with_context(|| format!("Pasture '{}' not found.", args.name))?;

    println!("{}", entry.path.display());
    Ok(())
}

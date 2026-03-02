mod apfs;
mod cli;
mod commands;
mod state;
mod vcs;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    commands::run(cli)
}

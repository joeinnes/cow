mod cd;
mod create;
mod diff;
mod extract;
mod list;
mod mcp;
mod remove;
mod status;
mod sync;

use anyhow::Result;
use crate::cli::{Cli, Commands};

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Cd(args) => cd::run(args),
        Commands::Create(args) => create::run(args),
        Commands::List(args) => list::run(args),
        Commands::Remove(args) => remove::run(args),
        Commands::Status(args) => status::run(args),
        Commands::Diff(args) => diff::run(args),
        Commands::Extract(args) => extract::run(args),
        Commands::Sync(args) => sync::run(args),
        Commands::Mcp => mcp::run(),
    }
}

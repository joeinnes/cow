mod cd;
mod create;
mod gc;
mod run_cmd;
mod fetch_from;
mod materialise;
mod recreate;
mod install;
mod diff;
mod extract;
mod list;
mod mcp;
mod migrate;
mod remove;
mod stats;
mod status;
mod sync;

use anyhow::Result;
use crate::cli::{Cli, Commands};

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Cd(args) => cd::run(args),
        Commands::Path(args) => cd::run(args),
        Commands::Run(args) => run_cmd::run(args),
        Commands::FetchFrom(args) => fetch_from::run(args),
        Commands::Materialise(args) => materialise::run(args),
        Commands::Recreate(args) => recreate::run(args),
        Commands::Create(args) => create::run(args),
        Commands::List(args) => list::run(args),
        Commands::Migrate(args) => migrate::run(args),
        Commands::Remove(args) => remove::run(args),
        Commands::Status(args) => status::run(args),
        Commands::Diff(args) => diff::run(args),
        Commands::Extract(args) => extract::run(args),
        Commands::Sync(args) => sync::run(args),
        Commands::Install => install::run(),
        Commands::Mcp => mcp::run(),
        Commands::Stats => stats::run(),
        Commands::Gc(args) => gc::run(args),
    }
}

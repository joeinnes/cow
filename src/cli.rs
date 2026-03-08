use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "cow",
    about = "Copy-on-write workspace manager for parallel development",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new workspace using APFS copy-on-write
    Create(CreateArgs),
    /// List all active workspaces
    List(ListArgs),
    /// Remove one or more workspaces
    Remove(RemoveArgs),
    /// Show detailed status of a workspace
    Status(StatusArgs),
    /// Show changes in a workspace relative to its last commit
    Diff(DiffArgs),
    /// Extract changes from a workspace as a patch or branch
    Extract(ExtractArgs),
    /// Print the path of a workspace (for shell cd integration)
    Cd(CdArgs),
    /// Sync a workspace with its source repository
    Sync(SyncArgs),
    /// Run as a Model Context Protocol (MCP) stdio server
    Mcp,
}

#[derive(clap::Args, Debug)]
pub struct CreateArgs {
    /// Name for the new workspace (auto-generated if omitted)
    pub name: Option<String>,

    /// Source repository path (defaults to current directory)
    #[arg(long)]
    pub source: Option<PathBuf>,

    /// Git branch to check out in the new workspace (created if it does not exist).
    /// Defaults to the workspace name when a name is given.
    #[arg(long)]
    pub branch: Option<String>,

    /// Do not switch or create a branch (stay on the source repo's current branch)
    #[arg(long)]
    pub no_branch: bool,

    /// jj change to edit in the new workspace
    #[arg(long)]
    pub change: Option<String>,

    /// Parent directory for workspaces (defaults to ~/.cow/workspaces/)
    #[arg(long)]
    pub dir: Option<PathBuf>,

    /// Skip post-clone cleanup of runtime artefacts (pid files, socket files, etc.)
    #[arg(long)]
    pub no_clean: bool,
}

#[derive(clap::Args, Debug)]
pub struct ListArgs {
    /// Only show workspaces created from this source repo
    #[arg(long)]
    pub source: Option<PathBuf>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct RemoveArgs {
    /// Workspace names to remove
    pub names: Vec<String>,

    /// Skip dirty-state warnings and remove immediately
    #[arg(long)]
    pub force: bool,

    /// Remove all workspaces (can be combined with --source)
    #[arg(long)]
    pub all: bool,

    /// Only remove workspaces from this source repo
    #[arg(long)]
    pub source: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
pub struct StatusArgs {
    /// Workspace name (defaults to current directory if it is a workspace)
    pub name: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct DiffArgs {
    /// Workspace name (defaults to current directory if it is a workspace)
    pub name: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct CdArgs {
    /// Workspace name
    pub name: String,
}

#[derive(clap::Args, Debug)]
pub struct SyncArgs {
    /// Branch in the source repo to sync from (defaults to workspace's current branch)
    pub source_branch: Option<String>,

    /// Workspace name (defaults to current directory)
    #[arg(long, short = 'n')]
    pub name: Option<String>,

    /// Merge instead of rebase
    #[arg(long)]
    pub merge: bool,
}

#[derive(clap::Args, Debug)]
pub struct ExtractArgs {
    /// Workspace name
    pub name: String,

    /// Write changes as a patch file
    #[arg(long)]
    pub patch: Option<PathBuf>,

    /// Push the workspace branch to origin under this name
    #[arg(long)]
    pub branch: Option<String>,
}

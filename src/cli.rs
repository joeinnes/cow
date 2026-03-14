use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "cow",
    about = "Copy-on-write pasture manager for parallel development",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new cow pasture (copy-on-write workspace) using APFS. 'create a cow pasture' → this command.
    Create(CreateArgs),
    /// List all active pastures
    List(ListArgs),
    /// Remove one or more pastures
    Remove(RemoveArgs),
    /// Show detailed status of a pasture
    Status(StatusArgs),
    /// Show changes in a pasture relative to its last commit
    Diff(DiffArgs),
    /// Extract changes from a pasture as a patch or branch
    Extract(ExtractArgs),
    /// Print the path of a pasture (for shell cd integration)
    Cd(CdArgs),
    /// Print the absolute path of a pasture
    Path(CdArgs),
    /// Sync a pasture with its source repository
    Sync(SyncArgs),
    /// Migrate existing git worktrees, jj workspaces, or orphaned directories to cow pastures
    Migrate(MigrateArgs),
    /// Remove a pasture and re-create it from the same source
    Recreate(RecreateArgs),
    /// Run a command inside a pasture's working directory
    Run(RunArgs),
    /// Replace symlinked directories in a pasture with real clonefiles
    Materialise(MaterialiseArgs),
    /// Fetch refs from another pasture into the current one (enables cross-pasture rebase)
    FetchFrom(FetchFromArgs),
    /// Install cowcd shell function and tab completion into your shell config
    Install,
    /// Run as a Model Context Protocol (MCP) stdio server
    Mcp,
    /// Show estimated disk savings across all pastures
    Stats,
    /// Remove pastures whose branches have been pushed or merged to origin
    Gc(GcArgs),
}

#[derive(clap::Args, Debug)]
pub struct CreateArgs {
    /// Name for the new pasture (auto-generated if omitted)
    pub name: Option<String>,

    /// Source repository path (defaults to current directory)
    #[arg(long)]
    pub source: Option<PathBuf>,

    /// Git branch to check out in the new pasture (created if it does not exist).
    /// Defaults to the pasture name when a name is given.
    #[arg(long)]
    pub branch: Option<String>,

    /// Do not switch or create a branch (stay on the source repo's current branch)
    #[arg(long)]
    pub no_branch: bool,

    /// jj change to edit directly in the new pasture (jj edit <change>). Use --from to branch from a change instead.
    #[arg(long)]
    pub change: Option<String>,

    /// jj change to use as parent — creates a new change on top (jj new <rev>) rather than editing it directly
    #[arg(long)]
    pub from: Option<String>,

    /// Parent directory for pastures (defaults to ~/.cow/pastures/)
    #[arg(long)]
    pub dir: Option<PathBuf>,

    /// Skip post-clone cleanup of runtime artefacts (pid files, socket files, etc.)
    #[arg(long)]
    pub no_clean: bool,

    /// Set the initial jj change description (jj repos only)
    #[arg(long, short = 'm')]
    pub message: Option<String>,

    /// Print only the pasture path on stdout after creation (suppress other output)
    #[arg(long)]
    pub print_path: bool,

    /// Skip large-directory auto-detection and always do a full clone
    #[arg(long)]
    pub no_symlink: bool,

    /// Create a git linked worktree instead of a CoW clone (git repos only).
    /// All pastures share the same .git/objects/ — cross-pasture rebase works
    /// without any remote dance.
    #[arg(long)]
    pub worktree: bool,
}

#[derive(clap::Args, Debug)]
pub struct ListArgs {
    /// Only show pastures created from this source repo
    #[arg(long)]
    pub source: Option<PathBuf>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Show the absolute path of each pasture
    #[arg(long)]
    pub paths: bool,
}

#[derive(clap::Args, Debug)]
pub struct RemoveArgs {
    /// Pasture names to remove
    pub names: Vec<String>,

    /// Skip dirty-state warnings and remove immediately
    #[arg(long)]
    pub force: bool,

    /// Skip confirmation prompts (still shows dirty warnings)
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Remove all pastures (can be combined with --source)
    #[arg(long)]
    pub all: bool,

    /// Only remove pastures from this source repo
    #[arg(long)]
    pub source: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
pub struct StatusArgs {
    /// Pasture name (defaults to current directory if it is a pasture)
    pub name: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct DiffArgs {
    /// Pasture name (defaults to current directory if it is a pasture)
    pub name: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct CdArgs {
    /// Pasture name
    pub name: String,
}

#[derive(clap::Args, Debug)]
pub struct SyncArgs {
    /// Pasture name (defaults to current directory)
    pub name: Option<String>,

    /// Branch in the source repo to sync from (defaults to pasture's current branch)
    #[arg(long)]
    pub source_branch: Option<String>,

    /// Merge instead of rebase
    #[arg(long)]
    pub merge: bool,
}

#[derive(clap::Args, Debug)]
pub struct MigrateArgs {
    /// Source repository path (defaults to current directory)
    #[arg(long)]
    pub source: Option<PathBuf>,

    /// Migrate all discovered candidates
    #[arg(long)]
    pub all: bool,

    /// Skip dirty-state checks and migrate anyway
    #[arg(long)]
    pub force: bool,

    /// Show what would be done without making any changes
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(clap::Args, Debug)]
pub struct RecreateArgs {
    /// Pasture name to recreate
    pub name: String,

    /// Override the branch checked out in the fresh clone
    #[arg(long)]
    pub branch: Option<String>,

    /// Do not switch or create a branch (stay on source's current branch)
    #[arg(long)]
    pub no_branch: bool,
}

#[derive(clap::Args, Debug)]
pub struct RunArgs {
    /// Pasture name
    pub name: String,

    /// Command and arguments to run
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
    pub command: Vec<String>,
}

#[derive(clap::Args, Debug)]
pub struct ExtractArgs {
    /// Pasture name
    pub name: String,

    /// Write changes as a patch file
    #[arg(long)]
    pub patch: Option<PathBuf>,

    /// Push the workspace branch to origin under this name
    #[arg(long)]
    pub branch: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct MaterialiseArgs {
    /// Pasture name
    pub name: String,
}

#[derive(clap::Args, Debug)]
pub struct GcArgs {
    /// Only remove pastures whose branch has been merged into the default branch
    #[arg(long)]
    pub merged: bool,

    /// Show what would be removed without removing anything
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompts
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Skip dirty-state warnings and remove immediately
    #[arg(long)]
    pub force: bool,

    /// Fetch from origin before checking (updates remote-tracking refs)
    #[arg(long)]
    pub fetch: bool,
}

#[derive(clap::Args, Debug)]
pub struct FetchFromArgs {
    /// Name of the pasture to fetch from
    pub from: String,

    /// Name of the pasture to fetch into (defaults to current directory)
    #[arg(long, short = 'n')]
    pub name: Option<String>,

    /// Allow fetching from a pasture with a different source repo
    #[arg(long)]
    pub force: bool,
}

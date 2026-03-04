# cow

Copy-on-write pasture manager for parallel development on macOS.

Uses APFS `clonefile(2)` to create instant, near-zero-cost copies of a repository. Each pasture looks and behaves like a full repo but only consumes disk for files that are actually modified.

## Why

Running multiple coding agents in parallel requires isolated workspaces per feature. Existing options are slow or heavy:

- **git worktree** — checks out every tracked file fresh; no `node_modules`, no build artefacts
- **full clone** — duplicates the entire `.git` directory, slow and wasteful
- **containers** — significant overhead for what should be a simple isolation problem

On APFS (the default on every modern Mac), `clonefile(2)` creates an instant block-level clone in a single syscall. A 5 GB monorepo cloned 10 times still costs ~5 GB total until agents start making changes. `node_modules`, `dist`, `.next`, `.env` — all immediately available, no install step needed.

## Install

```sh
brew tap joeinnes/tap
brew install cow
```

Or build from source:

```sh
cargo install cow-cli
```

## Quick start

```sh
# Inside a git repo
cow create feature-x        # clone repo to ~/.cow/pastures/feature-x, branch feature-x
cow create                  # auto-name: agent-1, agent-2, ...

# Point an agent at the pasture
CLAUDE_WORKSPACE=~/.cow/pastures/feature-x claude

# Review and clean up
cow list
cow status feature-x
cow remove feature-x
```

## Commands

### `cow create [OPTIONS] [NAME]`

Create a new pasture from a repository using APFS CoW. When a name is given it is also used as the branch name (created if it does not exist).

Large dependency directories (`node_modules`, `vendor`, `.venv`, `Pods`, etc.) are handled automatically:

- **Dep dirs** (e.g. `node_modules`) — each top-level package entry is symlinked individually, so existing packages are instantly available and new installs write locally to the pasture.
- **Other large dirs** (e.g. `fixtures/`) — the whole directory is symlinked to the source.

Run `cow materialise <name>` to replace all symlinks with real clonefiles for a fully independent pasture.

| Option | Description |
|--------|-------------|
| `--source <PATH>` | Source repo (default: current directory) |
| `--branch <BRANCH>` | Override the branch name (default: pasture name) |
| `--no-branch` | Do not switch or create a branch |
| `--change <CHANGE>` | jj change to edit in the new pasture |
| `--from <REV>` | jj revision to branch from (creates a new change on top) |
| `--dir <PATH>` | Parent directory for pastures (default: `~/.cow/pastures/`) |
| `--worktree` | Create a git linked worktree instead of a CoW clone (git only) |
| `--no-symlink` | Skip large-directory detection and always do a full clone |
| `--no-clean` | Skip post-clone cleanup of runtime artefacts |
| `-m, --message <MSG>` | Set the initial jj change description (jj repos only) |
| `--print-path` | Print only the pasture path on stdout (for scripting) |

#### `--worktree` mode

When `--worktree` is passed, cow creates a standard `git linked worktree` instead of an APFS clone. All pastures created this way share the same `.git/objects/` pack store with the source, so cross-pasture rebase works without any remote dance — use `cow fetch-from` to pull refs between pastures.

### `cow list [OPTIONS]`

List all active pastures.

| Option | Description |
|--------|-------------|
| `--source <PATH>` | Filter to pastures from this source |
| `--json` | Machine-readable JSON output |

### `cow status [NAME]`

Detailed status of a pasture. Defaults to the current directory if it is a pasture.

### `cow diff [NAME]`

Show changes in a pasture relative to its last commit. Passthrough to `git diff` / `jj diff`.

### `cow run <NAME> <CMD> [ARGS...]`

Run a command inside a pasture's working directory.

cow automatically detects the package manager from lockfiles (`pnpm-lock.yaml`, `yarn.lock`, `bun.lockb`/`bun.lock`, `package-lock.json`) and injects shims so that install subcommands (e.g. `npm install <pkg>`) write to the pasture-local `node_modules` rather than the shared source copy.

Three environment variables are always set for the subprocess:

| Variable | Value |
|----------|-------|
| `COW_PASTURE` | Pasture name |
| `COW_SOURCE` | Absolute path to the source repository |
| `COW_PASTURE_PATH` | Absolute path to the pasture directory |

```sh
cow run feature-x npm install lodash   # installs to pasture-local node_modules
cow run feature-x pnpm test
cow run feature-x cargo build
```

### `cow materialise <NAME>`

Replace symlinked dependency directories in a pasture with real APFS clonefiles. After materialisation the pasture is fully independent — packages can be added, removed, or upgraded without affecting the source or other pastures.

### `cow extract [OPTIONS] <NAME>`

Extract changes from a pasture back into the source repository.

| Option | Description |
|--------|-------------|
| `--patch <FILE>` | Write a patch file |
| `--branch <NAME>` | Create this branch in the source repo at pasture HEAD |

### `cow remove [OPTIONS] <NAME...>`

Remove one or more pastures. Warns before removing pastures with uncommitted changes, and offers to push unpushed commits first.

| Option | Description |
|--------|-------------|
| `--force` | Skip dirty-state warnings and remove without prompting |
| `-y, --yes` | Skip confirmation prompts (still shows dirty warnings) |
| `--all` | Remove all pastures |
| `--source <PATH>` | Scope `--all` to this source |

### `cow cd <NAME>`

Print the absolute path of a pasture. Designed for shell integration:

```sh
# Add to ~/.zshrc or ~/.bashrc
function cowcd() { cd "$(cow cd "$1")"; }

# Then:
cowcd feature-x
```

### `cow sync [SOURCE_BRANCH]`

Fetch the latest commits from the source repository and rebase the pasture onto them. Defaults to syncing with the pasture's own branch; pass a branch name to sync with a different one (e.g. `cow sync main`).

| Option | Description |
|--------|-------------|
| `--merge` | Merge instead of rebase |
| `--name <NAME>` | Target a named pasture instead of detecting from cwd |

### `cow fetch-from <FROM> [OPTIONS]`

Fetch all refs from another pasture into this one. Useful for rebasing one agent's work on top of another's without touching any remote.

| Option | Description |
|--------|-------------|
| `--name <NAME>` | Pasture to fetch into (defaults to current directory) |
| `--force` | Allow fetching from a pasture with a different source |

```sh
# Rebase feature-y on top of feature-x's work
cow fetch-from feature-x --name feature-y
cd ~/.cow/pastures/feature-y && git rebase cow/feature-x/feature-x
```

### `cow migrate [OPTIONS]`

Discover existing git linked worktrees, jj secondary workspaces, and orphaned cow pasture directories for a source repository, and migrate each one into a proper cow-managed APFS clone pasture.

Useful when you have been using `git worktree` or `jj workspace add` directly and want to bring those pastures under cow management without starting over.

| Option | Description |
|--------|-------------|
| `--source <PATH>` | Source repo (default: current directory) |
| `--all` | Migrate all discovered candidates |
| `--force` | Migrate dirty pastures (those with uncommitted changes) |
| `--dry-run` | Print what would happen without making any changes |

Without `--all`, the command lists discovered candidates and exits — nothing is modified.

**What each candidate type does:**

| Type | Action |
|------|--------|
| git linked worktree | APFS clone source, check out the same branch, remove old worktree |
| jj secondary workspace | `jj workspace add` at new location, forget old workspace |
| Orphaned cow directory | Register in state as-is (no clone, non-destructive) |

Dirty candidates (uncommitted changes) are skipped unless `--force` is passed. Orphaned directories are always registered regardless of dirty status, since registration is non-destructive.

```sh
# See what would be migrated
cow migrate --source ~/repos/myapp

# Migrate everything (skip dirty ones)
cow migrate --source ~/repos/myapp --all

# Migrate everything including dirty pastures
cow migrate --source ~/repos/myapp --all --force

# Preview without making changes
cow migrate --source ~/repos/myapp --all --dry-run
```

### `cow mcp`

Run as a [Model Context Protocol](https://modelcontextprotocol.io) stdio server. Exposes the following tools so agents can manage pastures without human intervention:

| Tool | Description |
|------|-------------|
| `cow_create` | Create a new pasture |
| `cow_list` | List all active pastures |
| `cow_remove` | Remove one or more pastures |
| `cow_status` | Detailed pasture status (JSON) |
| `cow_sync` | Sync a pasture with its source |
| `cow_extract` | Extract changes as a branch or patch |
| `cow_migrate` | Migrate existing worktrees/workspaces |
| `cow_materialise` | Replace symlinks with real clonefiles |
| `cow_fetch_from` | Fetch refs from another pasture |
| `cow_run` | Run a command inside a pasture |

Add to your MCP config (`~/.claude.json` or project `.mcp.json`):

```json
{
  "mcpServers": {
    "cow": {
      "command": "cow",
      "args": ["mcp"]
    }
  }
}
```

## Feature branch workflow

The intended lifecycle for an agent working on a feature branch:

```sh
# 1. Create pasture — also creates the feature branch
cow create feature-x --source ~/repos/myapp

# 2. Point your agent at the pasture
#    The agent commits to feature-x inside the pasture.

# 3. While the agent works, main moves on. Bring the pasture up to date:
cow sync main              # rebase pasture onto latest main from source

# 4. When the agent is done, land the branch in your source repo for review:
cow extract feature-x --branch feature-x
#    feature-x now exists locally in ~/repos/myapp — review, then push normally.

# 5. Push and open a PR from your source repo as usual:
cd ~/repos/myapp && git push origin feature-x

# 6. Remove the pasture (cow will offer to push if there are unpushed commits):
cow remove feature-x
```

**Direction:** `cow sync` goes FROM source TO pasture (brings source changes into your pasture). `cow extract --branch` goes the other way — FROM pasture TO source (lands your pasture branch in the source repo for review). Neither touches a remote.

When a pasture is freshly created it starts at the same HEAD as the source, so `cow sync` is effectively a no-op until the source repo accumulates new commits.

## How it works

On APFS, `clonefile(2)` creates a copy-on-write clone of a file in constant time. The clone shares all disk blocks with the original until either copy is modified, at which point APFS transparently copies only the modified block (not the whole file).

`cow create` calls `clonefile(2)` directly on the source directory. The kernel clones the entire tree in one atomic operation — the same mechanism Time Machine uses. The result is a full copy of the repository — including `node_modules`, build outputs, caches, and `.env` files — with near-zero disk overhead. A 2 GB repository clones in around 130 ms.

## Post-clone cleanup

Some runtime artefacts should not carry over (pid files, socket files). The `create` command strips these by default (`--no-clean` to skip).

Add a `.cow.json` to your repo to define project-specific cleanup:

```json
{
  "post_clone": {
    "remove": [".next/server", "*.pid", "*.sock"],
    "run": ["npm run codegen"]
  }
}
```

## Comparison

| | cow 🐄 | git worktree | full clone |
|-|--------|-------------|------------|
| Creation time | Instant | Seconds–minutes | Minutes |
| Disk overhead | ~0 (CoW) | Full working tree | Full repo |
| `node_modules` ready | Yes | No | No |
| `.env` / build cache | Yes | No | No |
| macOS/Linux | macOS | Both | Both |

## Limitations

- macOS requires APFS (`clonefile(2)` is an APFS-only syscall). Linux uses `cp --reflink=always` (btrfs or xfs); falls back to a regular copy with a warning on unsupported filesystems.
- Git submodules are not tested and may not work correctly.
- The source must be a primary git repo, not a git worktree.
- `cow sync` and `cow extract --branch` are not yet supported for jj repos.

## Development

### Test coverage

`cargo tarpaulin` reports ~90% overall coverage. `src/commands/migrate.rs` sits lower (~72%) for three verified reasons:

**1. jj code paths require a live jj repo**

`discover_jj_workspaces`, the `JjWorkspace` arm of `migrate_candidate`, the jj secondary-workspace guard in `run()`, and the `Some(Vcs::Jj)` branch in `discover_orphaned` all require an actual jj repository with secondary workspaces configured. These paths are annotated with `// tarpaulin-ignore-start/end` but may still appear in the uncovered-lines report depending on the tarpaulin version — the annotations suppress them from the hit/miss ratio but not always from the "Uncovered Lines" list.

**2. Integration tests run as subprocesses**

The integration tests invoke the compiled `cow` binary as a child process via `assert_cmd`. tarpaulin instruments the test binary, not the spawned process, so lines that are exercised exclusively through integration tests do not appear as covered. This is a known limitation of tarpaulin's instrumentation model; the paths are genuinely tested, just not visible to the coverage tool.

**3. Defensive error paths**

Several `bail!` branches guard against conditions that are difficult to trigger in automated tests: `clonefile(2)` returning an error, `git worktree remove` failing after a successful migration, a destination directory appearing between the exists-check and the clone call, and a name collision in state. These are not covered by any current test.

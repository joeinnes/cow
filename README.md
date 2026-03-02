# cow

Copy-on-write workspace manager for parallel development on macOS.

Uses APFS `clonefile` (via `cp -rc`) to create instant, near-zero-cost copies of a repository. Each workspace looks and behaves like a full repo but only consumes disk for files that are actually modified.

## Why

Running multiple coding agents in parallel requires isolated workspaces per feature. Existing options are slow or heavy:

- **git worktree** — checks out every tracked file fresh; no `node_modules`, no build artefacts
- **full clone** — duplicates the entire `.git` directory, slow and wasteful
- **containers** — significant overhead for what should be a simple isolation problem

On APFS (the default on every modern Mac), `cp -rc` creates an instant block-level clone. A 5 GB monorepo cloned 10 times still costs ~5 GB until agents start making changes. `node_modules`, `dist`, `.next`, `.env` — all immediately available, no install step needed.

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
cow create feature-x        # clone repo to ~/.cow/workspaces/feature-x, branch feature-x
cow create                  # auto-name: agent-1, agent-2, ...

# Point an agent at the workspace
CLAUDE_WORKSPACE=~/.cow/workspaces/feature-x claude

# Review and clean up
cow list
cow status feature-x
cow remove feature-x
```

## Commands

### `cow create [OPTIONS] [NAME]`

Create a new workspace from a repository using APFS CoW. When a name is given it is also used as the branch name (created if it does not exist).

| Option | Description |
|--------|-------------|
| `--source <PATH>` | Source repo (default: current directory) |
| `--branch <BRANCH>` | Override the branch name (default: workspace name) |
| `--no-branch` | Do not switch or create a branch |
| `--change <CHANGE>` | jj change to edit in the new workspace |
| `--dir <PATH>` | Parent directory for workspaces (default: `~/.cow/workspaces/`) |
| `--no-clean` | Skip post-clone cleanup of runtime artefacts |

### `cow list [OPTIONS]`

List all active workspaces.

| Option | Description |
|--------|-------------|
| `--source <PATH>` | Filter to workspaces from this source |
| `--json` | Machine-readable JSON output |

### `cow status [NAME]`

Detailed status of a workspace. Defaults to the current directory if it is a workspace.

### `cow diff [NAME]`

Show changes in a workspace relative to its last commit. Passthrough to `git diff` / `jj diff`.

### `cow extract [OPTIONS] <NAME>`

Extract changes from a workspace back into the source repository.

| Option | Description |
|--------|-------------|
| `--patch <FILE>` | Write a patch file |
| `--branch <NAME>` | Create this branch in the source repo at workspace HEAD |

### `cow remove [OPTIONS] <NAME...>`

Remove one or more workspaces. Warns before removing workspaces with uncommitted changes, and offers to push unpushed commits first.

| Option | Description |
|--------|-------------|
| `--force` | Skip dirty-state warnings and prompts |
| `--all` | Remove all workspaces |
| `--source <PATH>` | Scope `--all` to this source |

### `cow cd <NAME>`

Print the absolute path of a workspace. Designed for shell integration:

```sh
# Add to ~/.zshrc or ~/.bashrc
function cowcd() { cd "$(cow cd "$1")"; }

# Then:
cowcd feature-x
```

### `cow sync [SOURCE_BRANCH]`

Fetch the latest commits from the source repository and rebase the workspace onto them. Defaults to syncing with the workspace's own branch; pass a branch name to sync with a different one (e.g. `cow sync main`).

| Option | Description |
|--------|-------------|
| `--merge` | Merge instead of rebase |
| `--name <NAME>` | Target a named workspace instead of detecting from cwd |

### `cow mcp`

Run as a [Model Context Protocol](https://modelcontextprotocol.io) stdio server. Exposes `cow_create`, `cow_list`, `cow_remove`, `cow_status`, and `cow_diff` as MCP tools so agents can manage workspaces without human intervention.

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
# 1. Create workspace — also creates the feature branch
cow create feature-x --source ~/repos/myapp

# 2. Point your agent at the workspace
#    The agent commits to feature-x inside the workspace.

# 3. While the agent works, main moves on. Bring the workspace up to date:
cow sync main              # rebase workspace onto latest main from source

# 4. When the agent is done, land the branch in your source repo for review:
cow extract feature-x --branch feature-x
#    feature-x now exists locally in ~/repos/myapp — review, then push normally.

# 5. Push and open a PR from your source repo as usual:
cd ~/repos/myapp && git push origin feature-x

# 6. Remove the workspace (cow will offer to push if there are unpushed commits):
cow remove feature-x
```

**Direction:** `cow sync` goes FROM source TO workspace (brings source changes into your workspace). `cow extract --branch` goes the other way — FROM workspace TO source (lands your workspace branch in the source repo for review). Neither touches a remote.

When a workspace is freshly created it starts at the same HEAD as the source, so `cow sync` is effectively a no-op until the source repo accumulates new commits.

## How it works

On APFS, `clonefile(2)` creates a copy-on-write clone of a file in constant time. The clone shares all disk blocks with the original until either copy is modified, at which point APFS transparently copies only the modified block (not the whole file).

`cow create` runs `cp -rc <source> <dest>`, which uses `clonefile` for each file. The result is a full copy of the repository — including `node_modules`, build outputs, caches, and `.env` files — with near-zero disk overhead and instant creation time.

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
| macOS/Linux | Yes | No | No |

## Limitations

- macOS requires APFS (`cp -rc` uses `clonefile(2)`). Linux uses `cp --reflink=always` (btrfs or xfs); falls back to a regular copy with a warning on unsupported filesystems.
- Git submodules are not tested and may not work correctly.
- The source must be a primary git repo, not a git worktree.
- `cow sync` and `cow extract --branch` are not yet supported for jj workspaces.

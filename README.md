# sparse-worktree (`swt`)

Copy-on-write workspace manager for parallel development.

Uses macOS APFS `clonefile` (via `cp -rc`) to create instant, near-zero-cost copies of a repository. Each workspace looks and behaves like a full repo but only consumes disk for files that are actually modified.

## Why

Running multiple coding agents in parallel requires isolated workspaces per feature. Existing options are slow or heavy:

- **git worktree** — checks out every tracked file fresh; no `node_modules`, no build artefacts
- **full clone** — duplicates the entire `.git` directory, slow and wasteful
- **containers** — significant overhead for what should be a simple isolation problem

On APFS (the default on every modern Mac), `cp -rc` creates an instant block-level clone. A 5 GB monorepo cloned 10 times still costs ~5 GB until agents start making changes. `node_modules`, `dist`, `.next`, `.env` — all immediately available, no install step needed.

## Install

```sh
brew install joeinn.es/tap/sparse-worktree
```

Or build from source:

```sh
cargo install --path .
```

## Quick start

```sh
# Inside a git repo
swt create my-feature          # clone current repo to ~/.swt/workspaces/my-feature
swt create                     # auto-name: agent-1, agent-2, ...

# Point an agent at the workspace
claude --workspace ~/.swt/workspaces/agent-1

# Review and clean up
swt list
swt remove agent-1
```

## Commands

### `swt create [OPTIONS] [NAME]`

Create a new workspace from a repository using APFS CoW.

| Option | Description |
|--------|-------------|
| `--source <PATH>` | Source repo (default: current directory) |
| `--branch <BRANCH>` | Git branch to check out (created if it does not exist) |
| `--change <CHANGE>` | jj change to edit |
| `--dir <PATH>` | Parent directory for workspaces (default: `~/.swt/workspaces/`) |
| `--no-clean` | Skip post-clone cleanup of runtime artefacts |

### `swt list [OPTIONS]`

List all active workspaces.

| Option | Description |
|--------|-------------|
| `--source <PATH>` | Filter to workspaces from this source |
| `--json` | Machine-readable JSON output |

### `swt remove [OPTIONS] <NAME...>`

Remove one or more workspaces. Warns before removing workspaces with uncommitted changes.

| Option | Description |
|--------|-------------|
| `--force` | Skip dirty-state warnings and prompts |
| `--all` | Remove all workspaces |
| `--source <PATH>` | Scope `--all` to this source |

### `swt status [NAME]`

Detailed status of a workspace. Defaults to the current directory if it is a workspace.

### `swt diff [NAME]`

Show changes in a workspace relative to its last commit. Passthrough to `git diff` / `jj diff`.

### `swt extract [OPTIONS] <NAME>`

Extract changes from a workspace.

| Option | Description |
|--------|-------------|
| `--patch <FILE>` | Write a patch file |
| `--branch <NAME>` | Push to this branch name on origin |

## How it works

On APFS, `clonefile(2)` creates a copy-on-write clone of a file in constant time. The clone shares all disk blocks with the original until either copy is modified, at which point APFS transparently copies only the modified block (not the whole file).

`swt create` runs `cp -rc <source> <dest>`, which uses `clonefile` for each file. The result is a full copy of the repository — including `node_modules`, build outputs, caches, and `.env` files — with near-zero disk overhead and instant creation time.

## Post-clone cleanup

Some runtime artefacts should not carry over (pid files, socket files, stale lock files). The `create` command strips these by default (`--no-clean` to skip).

Add a `.swt.json` to your repo to define project-specific cleanup:

```json
{
  "post_clone": {
    "remove": [".next/server", "*.pid", "*.sock"],
    "run": ["npm run codegen"]
  }
}
```

## Comparison

| | swt | git worktree | full clone |
|-|-----|-------------|------------|
| Creation time | Instant | Seconds–minutes | Minutes |
| Disk overhead | ~0 (CoW) | Full working tree | Full repo |
| node_modules ready | Yes | No | No |
| macOS only | Yes | No | No |
| APFS required | Yes | No | No |

## Limitations

- macOS and APFS only (v1). Linux support via OverlayFS is a future consideration.
- Git submodules are not tested and may not work correctly.
- The source must be a primary git repo, not a git worktree.

## Homebrew tap

```ruby
brew tap joeinn.es/tap
brew install sparse-worktree
```

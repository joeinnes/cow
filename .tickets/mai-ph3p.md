---
id: mai-ph3p
status: closed
deps: []
links: []
created: 2026-03-03T09:02:24Z
type: epic
priority: 1
assignee: Joe
---
# cow migrate command

New subcommand: cow migrate [--source] [--all] [--force] [--dry-run]. Discovers git linked worktrees, jj secondary workspaces, and orphaned (unregistered) cow workspace directories for a given source repo, then migrates each to a proper cow-managed APFS clone workspace.

## Acceptance Criteria

cow migrate --all registers all found candidates in state; git worktrees are replaced by APFS clones and old worktree removed; jj workspaces are replaced and old workspace forgotten; orphaned directories are registered in-place; dirty candidates are skipped unless --force; --dry-run makes no changes


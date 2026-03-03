---
id: cow-r9tk
status: in_progress
deps: []
links: []
created: 2026-03-03T12:01:29Z
type: bug
priority: 1
assignee: Joe
---
# Run git worktree prune after cow clone to clear stale refs

When clonefile copies a git repo, .git/worktrees/ entries carry absolute paths from the source. These stale refs confuse git in the new workspace. Fix: run 'git worktree prune' in the destination immediately after any git CoW clone (affects both cow create and cow migrate).


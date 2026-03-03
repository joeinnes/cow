---
id: cow-65jp
status: open
deps: [cow-r9tk]
links: []
created: 2026-03-03T12:01:29Z
type: bug
priority: 1
assignee: Joe
---
# cow migrate: fail loudly and roll back on branch checkout failure

Currently migrate warns and continues when branch checkout fails, leaving the workspace on the wrong branch. This silent-wrong-state is the worst outcome. After the worktree-prune fix, if checkout still fails, migrate should fail loudly and either roll back or not register the workspace.


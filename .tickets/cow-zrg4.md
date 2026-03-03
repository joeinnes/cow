---
id: cow-zrg4
status: closed
deps: []
links: []
created: 2026-03-02T16:45:33Z
type: task
priority: 2
assignee: Joe
---
# Add dirty/branch status to cow list output

cow list --json only returns name, path, source, branch, vcs, created_at. An agent managing multiple workspaces needs to know which are dirty and what branch each is on without calling cow status N times. Add dirty (bool) and current_branch (string) to the cow list --json output. The human-readable table should also show these columns. This gives agents a single-call cross-workspace overview.


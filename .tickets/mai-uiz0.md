---
id: mai-uiz0
status: closed
deps: []
links: []
created: 2026-03-02T14:47:32Z
type: task
priority: 2
assignee: Joe
---
# cow create: use workspace name as branch by default

When a name is explicitly provided to cow create, use it as the branch name by default (checkout or create). This makes 'cow create feature-x' the idiomatic one-liner — no --branch flag needed. Auto-generated names (cow create with no name) should not switch branch. Add --no-branch flag to opt out of branch creation for generic sandbox workspaces. --branch <name> remains as an explicit override when workspace and branch names differ. Change is in run(): when args.name.is_some() && args.branch.is_none(), default args.branch to the provided name.


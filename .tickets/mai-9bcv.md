---
id: mai-9bcv
status: closed
deps: []
links: []
created: 2026-03-02T16:45:33Z
type: task
priority: 2
assignee: Joe
---
# Implement cow extract --branch for jj workspaces

cow extract --branch currently bails with 'not yet supported for jj'. For jj workspaces, the equivalent is to use jj git export (to write jj commits to the .git backend), then use the existing local-fetch mechanism (add workspace as temp remote in source, git fetch HEAD:branch, remove remote) — same as the git path. Alternatively use jj bookmark create <name> + push to source via local remote.


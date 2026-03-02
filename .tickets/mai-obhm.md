---
id: mai-obhm
status: closed
deps: []
links: []
created: 2026-03-02T14:37:53Z
type: task
priority: 2
assignee: Joe
---
# Redesign extract --branch to land changes in source repo locally

Current: pushes directly to remote origin from workspace — wrong mental model, requires network/auth, hardcodes 'origin'. Better: add a temporary local remote pointing at the workspace, fetch the branch into the SOURCE repo as a local branch, remove the remote. No network needed, no remote name assumptions, user reviews before pushing. API unchanged: cow extract <ws> --branch <name> creates branch <name> in source repo at workspace HEAD. Also reconsider --patch: consider using git bundle or preserving commit history rather than format-patch flattening.


---
id: mai-gayk
status: closed
deps: []
links: []
created: 2026-03-02T15:41:52Z
type: task
priority: 2
assignee: Joe
---
# Inject workspace context at creation for agent self-discovery

When an agent boots inside a workspace it has no immediate knowledge of: source repo path, current branch, initial commit SHA, or workspace name. Add a .cow-context file written into the workspace root at cow create time (JSON with name, source, branch, initial_commit, created_at). Agents can read this file to orient themselves without needing to call cow status or parse env vars.


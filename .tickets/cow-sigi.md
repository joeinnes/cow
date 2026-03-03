---
id: cow-sigi
status: closed
deps: []
links: []
created: 2026-03-02T12:51:20Z
type: task
priority: 2
assignee: Joe
---
# Fix extract --patch fragility

Currently runs 'git format-patch HEAD~1 --stdout' which only works with exactly one commit. Fix: record the HEAD SHA at workspace creation time in WorkspaceEntry.initial_commit, then use that SHA as the base in extract. Fall back to HEAD~1 for workspaces created before this change.


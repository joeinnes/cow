---
id: cow-ym9i
status: closed
deps: []
links: []
created: 2026-03-02T15:41:52Z
type: task
priority: 2
assignee: Joe
---
# Improve cow sync conflict detection and recovery guidance

When git rebase or merge fails mid-sync due to conflicts, cow exits with a generic error and leaves the workspace in rebase/merge conflict state. This is a hard block for agents with no path forward. Detect the conflict state after failure, auto-abort the rebase (git rebase --abort), report which files conflicted, and suggest the manual resolution path (sync --merge as an alternative, or resolve manually then rebase --continue).


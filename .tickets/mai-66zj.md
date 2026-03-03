---
id: mai-66zj
status: closed
deps: [mai-bprz]
links: []
created: 2026-03-02T09:50:31Z
type: task
priority: 0
assignee: Joe Innes
---
# Command: swt list

Implement src/commands/list.rs. Load state, prune missing workspaces. Filter by --source. Compute current branch and dirty status dynamically. Print aligned table (NAME, SOURCE, BRANCH, STATUS, CREATED) with coloured status. Support --json output.


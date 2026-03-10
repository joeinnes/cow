---
id: cow-c930
status: closed
deps: [cow-bprz]
links: []
created: 2026-03-02T09:50:31Z
type: task
priority: 0
assignee: Joe Innes
---
# Command: swt remove

Implement src/commands/remove.rs. Accept NAME... positional args or --all (scoped by --source). For each workspace: git dirty → scary warning + prompt (skip with --force). jj → info note about op log + gentle confirm (skip with --force). Clean git → remove without prompt. rm -rf and prune from state.


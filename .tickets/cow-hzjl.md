---
id: cow-hzjl
status: closed
deps: []
links: []
created: 2026-03-02T15:41:52Z
type: task
priority: 2
assignee: Joe
---
# Add cow cd shell integration

There is no way to jump into a workspace from the shell without parsing cow list --json with jq. Add a cow cd <name> command that prints the workspace path to stdout, designed for eval $(cow cd feature-x) or a shell function wrapper. Document the shell function in README: function cowcd() { cd $(cow cd "$1") }


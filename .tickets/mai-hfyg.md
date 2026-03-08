---
id: mai-hfyg
status: closed
deps: []
links: []
created: 2026-03-02T12:51:20Z
type: task
priority: 2
assignee: Joe
---
# Implement --change arg for jj workspaces

CreateArgs.change is parsed but never passed to setup_jj. After the CoW clone, if --change is supplied run 'jj edit <change>' in the workspace directory.


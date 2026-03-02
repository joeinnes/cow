---
id: mai-63m9
status: closed
deps: []
links: []
created: 2026-03-02T16:45:33Z
type: task
priority: 2
assignee: Joe
---
# Implement cow sync for jj workspaces

cow sync currently bails with 'not yet supported for jj'. For jj workspaces, the equivalent of git fetch + rebase is: jj git fetch (to pull source commits into the repo's git backend), then jj rebase -d <source-branch> to move the working copy onto the updated tip. Should respect the --merge flag by using jj rebase without --rebase-descendants, or surface a sensible equivalent. Refuse if working copy has changes (jj diff --summary non-empty).


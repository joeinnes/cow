---
id: cow-in79
status: closed
deps: []
links: []
created: 2026-03-02T14:38:00Z
type: task
priority: 2
assignee: Joe
---
# Add cow sync command

New command: cow sync [<name>] — fetches latest commits from the source repo into the workspace and rebases (default) or merges the workspace branch onto them. Implementation: add source repo as a temporary local git remote in workspace, fetch, rebase workspace HEAD onto source HEAD, remove remote. Options: --merge instead of rebase; --onto <branch> to specify source branch (default: source repo's current HEAD branch). Failure modes to handle: uncommitted changes in workspace (refuse with helpful message directing to git stash); merge/rebase conflicts (leave workspace in conflict state, print instructions). For jj: not yet supported (bail with clear message).


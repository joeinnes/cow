---
id: mai-f7ty
status: closed
deps: []
links: []
created: 2026-03-02T14:47:52Z
type: task
priority: 2
assignee: Joe
---
# Add cow sync command

New command: cow sync [<source-branch>] — fetches the latest from the source repo and rebases (default) or merges the workspace onto it. Auto-detects workspace from cwd like status/diff. Without source-branch arg, syncs with the same branch name as the workspace's current branch in the source repo. 'cow sync main' rebases workspace onto source's main. --merge flag swaps to merge strategy. --name <ws> to target a named workspace instead of cwd. Implementation: in workspace, add source path as temp git remote (_cow_sync), fetch, rebase/merge, remove remote — all local filesystem ops, no network. Refuse if workspace has uncommitted changes (print helpful message). For jj: bail with not yet supported.


---
id: mai-lwfo
status: closed
deps: []
links: []
created: 2026-03-02T14:47:39Z
type: task
priority: 2
assignee: Joe
---
# cow remove: offer to push unpushed commits before removing

When removing a git workspace interactively (not --force, stdin is TTY), check if the workspace branch has commits not yet on the remote: git log origin/<branch>..HEAD. If commits exist, prompt 'Branch has N unpushed commit(s). Push to origin before removing? [y/N]'. If yes, run git push origin <branch> (with --set-upstream if branch has no tracking). With --force: skip the prompt and remove without pushing (but print a warning if unpushed commits exist). Non-TTY: warn about unpushed commits on stderr, proceed with removal. No change for jj workspaces.


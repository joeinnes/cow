---
id: cow-5ol0
status: closed
deps: [cow-bprz]
links: []
created: 2026-03-02T09:50:31Z
type: task
priority: 0
assignee: Joe Innes
---
# Command: swt create

Implement src/commands/create.rs. Validate source is a git/jj repo, not a git worktree, and is on APFS. Generate workspace name if not provided (agent-1, agent-2...). cp -rc for CoW clone. Git post-clone: checkout branch (create if needed), detect current branch. jj post-clone: jj workspace forget. Cleanup step (pid/sock files, .swt.json patterns). Update state.json. Print workspace path.


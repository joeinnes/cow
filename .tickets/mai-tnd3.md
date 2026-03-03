---
id: mai-tnd3
status: closed
deps: []
links: []
created: 2026-03-02T21:43:32Z
type: task
priority: 2
assignee: Joe
---
# Detect and reject secondary jj workspaces as sources

cow create allows secondary jj workspaces as --source because detect_vcs finds .jj as a directory in both primary and secondary workspaces. There is no equivalent of the git worktree guard for jj. A secondary jj workspace should be rejected with a clear error telling the user to use the primary workspace instead. Add is_jj_workspace() to vcs.rs (check for absence of .jj/repo/ or presence of a repo_path pointer) and wire it into the worktree guard in create.rs alongside the existing git check.


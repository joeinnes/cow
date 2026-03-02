---
id: mai-bprz
status: closed
deps: [mai-yjme]
links: []
created: 2026-03-02T09:50:17Z
type: task
priority: 0
assignee: Joe Innes
---
# Core modules: cli.rs, state.rs, apfs.rs, vcs.rs

Implement the foundational modules. cli.rs: clap CLI definitions for all subcommands (create, list, remove, status, diff, extract). state.rs: WorkspaceEntry, State struct, load/save/prune to ~/.swt/state.json. apfs.rs: is_apfs() using libc::statfs. vcs.rs: detect_vcs(), is_git_worktree(), git_current_branch(), git_is_dirty(), git_status_short(), jj_is_dirty().


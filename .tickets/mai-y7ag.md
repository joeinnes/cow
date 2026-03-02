---
id: mai-y7ag
status: closed
deps: [mai-5ol0, mai-66zj, mai-c930, mai-ints]
links: []
created: 2026-03-02T09:50:37Z
type: task
priority: 1
assignee: Joe Innes
---
# Integration tests

Write tests in tests/integration.rs. Use tempfile for temp git repos. Tests: create from git repo (verify workspace exists, correct branch), create with --branch new/existing, create from git worktree (should fail with helpful error), list (create multiple, verify output), remove clean workspace, remove dirty git workspace (verify warning + prompt), remove with --force (no prompt). All tests gated on #[cfg(target_os = "macos")] since they require APFS.


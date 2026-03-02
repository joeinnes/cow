---
id: mai-6ksh
status: closed
deps: []
links: []
created: 2026-03-02T10:35:18Z
type: task
priority: 0
assignee: Joe
---
# Rename swt → cow / sparse-worktree → cow-cli

Rename binary from swt to cow, crate name from sparse-worktree to cow-cli, state dir from ~/.swt to ~/.cow. Touch: Cargo.toml, src/cli.rs, src/state.rs, build.rs, Makefile, README.md, tests/integration.rs. Rebuild and re-run tests to verify.


---
id: mai-najj
status: closed
deps: []
links: []
created: 2026-03-02T10:04:46Z
type: task
priority: 0
assignee: Joe
---
# Man page generation via clap_mangen

Add build-dependencies (clap_mangen, clap/derive). Write build.rs that includes src/cli.rs and renders swt.1 to OUT_DIR and target/man/swt.1. Update Makefile install/release targets to copy the man page. Verify cargo build generates the file.


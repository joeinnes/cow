---
id: cow-yrdb
status: closed
deps: []
links: []
created: 2026-03-03T16:50:00Z
type: task
priority: 2
assignee: Joe Innes
---
# Warn about (or auto-resolve) conflicted jj bookmarks on cow create

When cloning a jj workspace, bookmarks from the source may already be in a conflicted state. These conflicts are irrelevant to the new workspace's work but surface in `jj status` and can confuse the user.

`cow create` should detect conflicted bookmarks in the new workspace after cloning and either:
- Warn the user listing which bookmarks are conflicted and suggesting `jj bookmark set` to resolve, or
- Offer an `--resolve-bookmarks` flag that auto-resolves conflicts by setting each conflicted bookmark to the local target.

The warning approach is lower risk as a first step.

## Resolution

Won't fix. A cloned workspace should faithfully reflect the source state — if the source has conflicted bookmarks, so should the clone. This is correct semantics, not a bug.

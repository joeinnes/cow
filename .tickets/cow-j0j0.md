---
id: cow-j0j0
status: closed
deps: []
links: []
created: 2026-03-03T10:46:17Z
type: bug
priority: 1
assignee: Joe
---
# Atomic rollback on failed cow create

If cow create fails mid-way (e.g. jj workspace add succeeds but a later step fails), it leaves an inconsistent state: the jj workspace record exists but the directory is wrong or missing. cow remove cannot find the workspace, so the user must manually run jj workspace forget and rm -rf. Creation should either complete fully or roll back fully on any error.

## Acceptance Criteria

any failure during cow create cleans up both the filesystem directory and any jj/git workspace records before exiting with an error; no manual recovery steps needed


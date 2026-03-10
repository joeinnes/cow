---
id: cow-zpft
status: closed
deps: []
links: []
created: 2026-03-03T10:46:17Z
type: chore
priority: 3
assignee: Joe
---
# Print teardown reminder in cow create output

First-time users reach for jj and rm directly for teardown out of habit. The final line of cow create output should remind them of the correct command, e.g. 'To remove this workspace: cow remove <name>'. Small change, high discoverability impact.

## Acceptance Criteria

cow create success output includes a 'To remove: cow remove <name>' line


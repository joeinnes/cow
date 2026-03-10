---
id: cow-8per
status: closed
deps: []
links: []
created: 2026-03-03T10:46:17Z
type: bug
priority: 2
assignee: Joe
---
# cow list and cow remove handle half-created workspaces

When creation fails partway, the workspace may not appear in cow list (because it was never written to state) yet leave behind jj workspace records or stale directories. There is currently no way to discover or remove such orphaned state through cow itself. cow list should surface partially-created workspaces, and cow remove (or a new cow repair command) should be able to clean them up.

## Acceptance Criteria

a workspace that failed mid-creation is either fully rolled back (preferred, see atomic rollback ticket) or visible in cow list with a 'partial' indicator and removable via cow remove


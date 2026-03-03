---
id: cow-d49r
status: closed
deps: []
links: []
created: 2026-03-03T10:53:40Z
type: chore
priority: 3
assignee: Joe
---
# Clarify --change help text: 'edit this commit directly', not 'branch from here'

The --change flag description currently reads 'jj change to edit in the new workspace'. This doesn't make clear that it calls jj edit (edits the commit directly) rather than creating a new change on top. Users expecting 'branch from this revision' semantics will be confused. Update the help string to something like 'jj change ID to edit directly in the new workspace (use jj new <rev> inside for branching)'. Error handling for immutable refs is tracked separately in cow-6ml1.

## Acceptance Criteria

--change help text clearly distinguishes 'edit this commit' from 'branch from here' semantics


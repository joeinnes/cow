---
id: cow-bqy8
status: closed
deps: []
links: []
created: 2026-03-03T10:46:17Z
type: feature
priority: 1
assignee: Joe
---
# Add --from flag for jj workspace creation

Add --from <rev> to cow create for jj repos. --change edits a commit directly (immutable refs like main@origin fail). --from should run jj new <rev> as part of creation so the workspace starts on a new change on top of the given revision. The distinction between 'edit this commit' and 'start work here' is not obvious from the current help text; --from makes the safe default explicit.

## Acceptance Criteria

cow create --from main@origin creates a new change on top of main@origin in the workspace; --change retains its current 'edit this commit' semantics; help text for both flags clearly explains the difference


---
id: cow-tzqb
status: closed
deps: []
links: []
created: 2026-03-03T10:46:17Z
type: chore
priority: 2
assignee: Joe
---
# Improve --change help text and error message for immutable refs

The current --change flag silently tries to edit whatever ref is given. When passed an immutable ref (e.g. main@origin) jj rejects it with an opaque error. The help text should explain that --change means 'edit this commit directly' and suggest --from for branching from a revision. The error path should also catch the immutable-ref case and print a clear actionable message.

## Acceptance Criteria

help text for --change explains immutable-ref limitation and points to --from; passing an immutable ref prints a user-friendly error suggesting --from instead


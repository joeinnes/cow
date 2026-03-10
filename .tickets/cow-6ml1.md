---
id: cow-6ml1
status: closed
deps: []
links: []
created: 2026-03-03T10:51:39Z
type: bug
priority: 2
assignee: Joe
---
# Narrow --change error: detect immutable jj ref and suggest alternative

When --change is passed an immutable jj ref (e.g. main@origin), jj rejects it with an opaque error that leaks through to the user. cow should detect this case — check whether the change ID is immutable before invoking jj edit — and print a clear, actionable message explaining that immutable refs cannot be edited and suggesting 'create without --change, then jj new <rev> inside the workspace' as the workaround. The broader --from flag is tracked separately.

## Acceptance Criteria

passing --change main@origin (or any immutable ref) prints a user-friendly cow error message with a suggested workaround, not a raw jj error


---
id: cow-b3so
status: closed
deps: []
links: []
created: 2026-03-03T10:53:50Z
type: chore
priority: 3
assignee: Joe
---
# Mention .cow.json in cow create output to improve discoverability

Users copying stale build artefacts (dist/, build/, .next/) are surprised when tests fail in a new workspace. The .cow.json post_clone.remove mechanism already solves this, but users don't know it exists. cow create should print a brief note when no .cow.json is found — e.g. 'Tip: add a .cow.json to remove stale build dirs and run post-clone setup. See docs.' This is the right fix rather than a --no-build-cache flag.

## Acceptance Criteria

cow create output mentions .cow.json when the source repo does not have one; existing .cow.json projects are unaffected


---
id: cow-9rgd
status: closed
deps: []
links: []
created: 2026-03-03T10:46:17Z
type: chore
priority: 3
assignee: Joe
---
# Note stale build artefacts in cow create output

APFS cloning copies dist/, build/, and other artefact directories verbatim. These may be stale relative to what the workspace branch will build, causing confusing test failures. cow create should print a brief note (e.g. 'Note: build artefacts were copied — run your build step if tests behave unexpectedly'). A --no-build-cache flag that removes common output dirs post-clone (dist/, build/, .next/, target/ etc.) would also help.

## Acceptance Criteria

cow create output includes a note about copied build artefacts; optionally a --no-build-cache flag removes common output directories after cloning


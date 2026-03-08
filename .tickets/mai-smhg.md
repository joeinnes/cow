---
id: mai-smhg
status: closed
deps: []
links: []
created: 2026-03-02T14:48:37Z
type: task
priority: 2
assignee: Joe
---
# Document feature branch workflow and cow sync mental model

Add a 'Feature branch workflow' section to the README explaining the end-to-end lifecycle: (1) cow create feature-x — creates workspace checked out on feature-x (new branch from source HEAD); (2) point agent at workspace, commits accumulate; (3) cow sync main — rebase workspace onto latest main when source has moved; (4) cow extract feature-x --branch feature-x — lands branch in source repo locally for review; (5) push and open PR from source repo as normal; (6) cow remove feature-x (with push offer). Clarify that on creation the workspace starts at source HEAD, so cow sync is a no-op until source moves. Note that cow sync syncs FROM source TO workspace (not the other way — that is what extract is for).


---
id: cow-x812
status: closed
deps: []
links: []
created: 2026-03-03T16:50:00Z
type: task
priority: 2
assignee: Joe Innes
---
# Show one-line change summary for dirty workspaces in cow list

`cow list` currently shows a dirty indicator but no detail about what's changed. Users naturally reach for `jj status` or `git status` directly rather than `cow status`/`cow diff`, partly because there's no hook in the list view to prompt them.

Add a one-line summary to the dirty indicator in `cow list` output — e.g. the first line of `git status --short` or `jj status`, or a file-changed count:

```
my-feature   ~/cow/my-feature   dirty (3 files)   main
```

This makes the list view more informative at a glance and naturally surfaces `cow status`/`cow diff` as the next step. Keep it short — the goal is a hint, not a full status dump. Should be suppressible with `--json` (already structured) and possibly `--quiet`.

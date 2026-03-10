---
id: cow-1t0b
status: closed
deps: []
links: []
created: 2026-03-03T16:50:00Z
type: task
priority: 1
assignee: Joe Innes
---
# Add --message/-m flag to cow create for initial jj change description

After `cow create` on a jj repo, the new workspace has an empty change with no description. The CLAUDE.md convention (and good jj hygiene) is to always describe changes immediately, but this requires a separate `jj describe -m "..."` step.

Add `-m`/`--message` to `cow create` so the initial change description can be set in one command:

```sh
cow create my-feature -m "Refactor auth flow"
```

For git repos this flag can be silently ignored (or used as the branch description if we ever support that). Implementation: after workspace creation, if VCS is jj and --message is provided, run `jj describe -m "<message>"` inside the new workspace.

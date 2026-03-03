---
id: cow-pazi
status: closed
deps: []
links: []
created: 2026-03-03T16:50:00Z
type: task
priority: 1
assignee: Joe Innes
---
# Add -y/--yes flag to cow remove

`cow remove` currently prompts interactively and defaults to "no" when not in a TTY. The workaround is `--force`, but that implies overriding a safety check rather than simply confirming intent.

Add a `-y`/`--yes` flag that skips the confirmation prompt without the "I'm overriding a guard" connotation of `--force`. `--force` should remain for its original purpose (removing dirty workspaces). This makes `cow remove` usable cleanly in scripts and non-TTY contexts like AI agent shells.

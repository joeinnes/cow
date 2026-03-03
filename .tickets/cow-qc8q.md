---
id: cow-qc8q
status: closed
deps: []
links: []
created: 2026-03-02T15:41:52Z
type: task
priority: 2
assignee: Joe
---
# Add --json flag to cow status

cow list has --json but cow status only prints human-readable text. Orchestrating systems and agents that need to inspect workspace state programmatically have no reliable parsing target. Add cow status [NAME] --json that outputs a single JSON object with the same fields as cow list --json plus dirty, modified_files, and initial_commit.


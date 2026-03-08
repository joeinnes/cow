---
id: mai-p00x
status: closed
deps: []
links: []
created: 2026-03-02T15:41:52Z
type: task
priority: 2
assignee: Joe
---
# Expose cow_sync and cow_extract in MCP server

The MCP server only covers cow_create, cow_list, cow_remove, cow_status, cow_diff. An agent can create a workspace and inspect it but cannot sync with main or land its branch when done — requiring human intervention mid-task. Add cow_sync and cow_extract as MCP tools to complete the agent-driven workflow lifecycle.


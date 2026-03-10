---
id: cow-wis3
status: closed
deps: []
links: []
created: 2026-03-02T10:12:39Z
type: feature
priority: 0
assignee: Joe
---
# Add swt mcp subcommand (MCP server)

Expose swt's capabilities as an MCP (Model Context Protocol) server so that Claude Code and other MCP-aware agents can discover and invoke workspaces without human configuration.

Subcommand: swt mcp [--port <PORT>] [--stdio]
- Default transport: stdio (most compatible with Claude Code)
- Optional HTTP/SSE transport for other clients

Tools to expose:
- create_workspace: args = {name?, source?, branch?, change?, dir?, no_clean?} — calls swt create
- list_workspaces: args = {source?, json?} — returns workspace list as structured JSON
- remove_workspace: args = {names, force?, all?, source?} — calls swt remove
- workspace_status: args = {name?} — returns status struct
- workspace_diff: args = {name?} — returns diff output as string

Each tool should have a JSON schema description so Claude can understand what it does without reading source code.

Implementation approach:
- Add rmcp or mcp-server crate (or hand-roll the JSON-RPC over stdio protocol — it is simple)
- tools/list returns tool schemas
- tools/call dispatches to the existing command implementations
- Reuse Result<()> impls from commands/ — extract core logic into functions that return structured data rather than printing, so MCP and CLI both consume them

Also: add a CLAUDE.md snippet to README so users know to add 'swt mcp' to their MCP server config.


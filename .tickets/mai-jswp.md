---
id: mai-jswp
status: closed
deps: []
links: []
created: 2026-03-02T10:12:39Z
type: feature
priority: 1
assignee: Joe
---
# Marketing landing page

Build a marketing/landing page for sparse-worktree (swt) using /frontend-design skill.

Content to cover:
- Hero: headline + one-sentence value prop + install command (brew install ...)
- Problem: running parallel agents needs isolated workspaces; current options are slow/heavy
- Solution: APFS CoW — instant clone, zero disk overhead, node_modules included
- Key stats/claims: instant creation, ~0 disk overhead until modified, full working directory
- Comparison table: swt vs git worktree vs full clone (time, disk, node_modules, setup)
- CLI demo: animated or code-block walkthrough of swt create / swt list / swt remove
- MCP integration section: how to wire it up with Claude Code
- Install section: brew + cargo
- Footer: GitHub link, license

Design direction: developer tool aesthetic — dark theme, monospace accents, terminal feel. Should look at home alongside tools like mise, atuin, zoxide.


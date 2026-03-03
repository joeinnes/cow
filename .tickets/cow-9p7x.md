---
id: cow-9p7x
status: closed
deps: []
links: []
created: 2026-03-03T16:50:00Z
type: task
priority: 1
assignee: Joe Innes
---
# Add --print-path flag to cow create

`cow cd` works via shell integration but doesn't help in non-TTY or piped contexts (e.g. AI agent shells). Add a `--print-path` flag to `cow create` that outputs just the final workspace path on stdout after creation, making it easy to capture and use in automation:

```sh
WS=$(cow create my-feature --print-path)
```

This is distinct from the normal create output (which goes to stderr or is human-readable). The flag should suppress all other output and print only the path.

---
id: cow-i13e
status: closed
deps: []
links: []
created: 2026-03-02T21:43:37Z
type: task
priority: 2
assignee: Joe
---
# jj clone is slow due to copying .jj/repo git backend

When cloning a primary jj workspace, cp -rc copies the full .jj/repo/store/git/ git backend, which contains the entire repo history and can be very large. This defeats the CoW speed advantage. The fix is to exclude .jj from the cp -rc clone and instead run 'jj workspace add <dest>' against the source repo to create a proper independent workspace. This gives true isolation without duplicating the git store.


---
id: mai-dxos
status: closed
deps: []
links: []
created: 2026-03-02T15:42:19Z
type: task
priority: 2
assignee: Joe
---
# Linux reflink support via cp --reflink

cow create uses cp -rc which is APFS CoW-specific on macOS. On Linux, cp --reflink=always achieves the same copy-on-write behaviour on btrfs and xfs. Detect the OS at runtime: on Linux, attempt cp --reflink=always first; if the filesystem does not support reflinking (exit non-zero), fall back to a regular cp with a warning that disk overhead will be higher. Remove the hard APFS guard on Linux (keep it on macOS where statfs detection is reliable). Update README to document Linux support and filesystem requirements (btrfs, xfs, or APFS).


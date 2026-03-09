---
id: mai-8q1a
status: closed
deps: []
links: []
created: 2026-03-02T10:12:39Z
type: task
priority: 1
assignee: Joe
---
# Homebrew distribution

Prepare the project for distribution via a Homebrew tap.

Steps:
1. Choose and document a GitHub repo name (sparse-worktree or swt)
2. Create release workflow: .github/workflows/release.yml
   - Trigger on push to v* tags
   - Build universal binary (lipo aarch64 + x86_64) using existing Makefile release target
   - Create GitHub release and upload tarball
3. Create homebrew-tap repo structure (document as a separate repo: homebrew-tap)
   - Formula file: Formula/sparse-worktree.rb
   - Install block: bin.install 'swt' and man1.install 'swt.1'
   - Test block: system bin/'swt', '--version'
4. Update README with correct brew tap install instructions
5. Update Makefile release target to also output the formula SHA line in a machine-readable way for easy formula updates

The release workflow should output the SHA256 so it can be pasted directly into the Homebrew formula.


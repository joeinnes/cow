# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`cow` is a Clone-on-Write workspace manager for macOS. It uses the APFS `clonefile(2)` syscall to create instant, near-zero-cost copies ("pastures") of git and jj repositories for parallel development.

## Commands

```bash
cargo build            # Build the binary
cargo test             # Run unit tests
cargo test -- --nocapture  # Run tests with output
cargo tarpaulin        # Code coverage (requires cargo-tarpaulin)
make install           # Build release binary and install to /usr/local/bin
```

Package manager: Cargo. Binary name: `cow` (crate name: `cow-cli`).

## Tech Stack

- Rust (stable)
- clap 4 (CLI argument parsing, derive macros)
- anyhow (error handling)
- serde + serde_json (state file serialisation)
- libc (direct `clonefile` syscall)
- chrono (timestamps in state)

## Architecture

### Key Directories

```
src/
  main.rs          # Entry point, CLI dispatch
  cli.rs           # clap CLI definitions
  commands/        # One file per subcommand (create, list, remove, status, etc.)
  state.rs         # State file read/write (~/.cow/state.json)
  vcs.rs           # VCS detection (git / jj)
  apfs.rs          # clonefile(2) wrapper
build.rs           # Build script
tests/             # Integration tests (require macOS/APFS)
```

### State File

`~/.cow/state.json` tracks all registered pastures. Each entry records source path, pasture path, name, branch, and creation time.

### APFS Cloning

`apfs.rs` calls `libc::clonefile(src, dst, 0)` directly. This is the same mechanism Time Machine uses — O(1) block-level clone, no data copied until modified.

### VCS Support

- **git**: creates a branch, checks it out in the pasture
- **jj**: uses `jj workspace add --config signing.behavior="drop"` to suppress SSH passphrase prompts on the ephemeral init commit; subsequent user commits are signed normally

## Testing

Integration tests are all gated on `#[cfg(target_os = "macos")]` and set `HOME` per-test to isolate state files. Run `cargo test` on macOS to execute them.

## Distribution

Published to crates.io as `cow-cli`. Homebrew tap at `joeinnes/tap`.

```bash
brew tap joeinnes/tap
brew install cow
```

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{
    cli::CreateArgs,
    state::{self, State, PastureEntry},
    vcs::{self, Vcs},
};
#[cfg(target_os = "macos")]
use crate::apfs;

/// Directory names that use per-package symlinks rather than whole-dir symlinks
/// when they exceed the large-dir threshold.
const DEP_DIR_NAMES: &[&str] = &[
    "node_modules", "vendor", ".venv", "venv", "env", "Pods", "bower_components",
];

pub fn run(args: CreateArgs) -> Result<()> {
    // Resolve source path
    let source = match args.source {
        Some(p) => p
            .canonicalize()
            .with_context(|| format!("Cannot resolve source path: {}", p.display()))?,
        None => std::env::current_dir().context("Cannot determine current directory")?,
    };

    // Detect VCS
    let detected_vcs = vcs::detect_vcs(&source)
        .context("No VCS found. Source must be a git or jj repository.")?;

    // --worktree requires git
    if args.worktree && detected_vcs != Vcs::Git {
        bail!("--worktree is only supported for git repositories.");
    }

    // Reject git worktrees as sources
    if detected_vcs == Vcs::Git && vcs::is_git_worktree(&source) {
        bail!(
            "Source '{}' is a git worktree, not a primary repository.\n\
             Please use the main (primary) repository as --source instead.",
            source.display()
        );
    }

    // Reject secondary jj workspaces as sources
    if detected_vcs == Vcs::Jj && vcs::is_jj_secondary_workspace(&source) {
        bail!(
            "Source '{}' is a jj workspace, not a primary repository.\n\
             Please use the main (primary) repository as --source instead.",
            source.display()
        );
    }

    // APFS check (macOS only; worktrees don't use clonefile so skip for them)
    // tarpaulin-ignore-start
    #[cfg(target_os = "macos")]
    if !args.worktree && !apfs::is_apfs(&source) {
        bail!(
            "Source filesystem is not APFS. cow requires APFS for copy-on-write clones on macOS.\n\
             Run `diskutil info {}` to see the filesystem type.",
            source.display()
        );
    }
    // tarpaulin-ignore-end

    // Warn about submodules (not supported, but don't block)
    if source.join(".gitmodules").exists() {
        eprintln!(
            "Warning: source repository has git submodules. \
             Submodule support is untested and may produce a broken pasture."
        );
    }

    // Load + prune state
    let mut state = State::load()?;
    state.prune_deleted();

    // Pasture parent directory
    let has_custom_dir = args.dir.is_some();
    let pasture_dir = match args.dir {
        Some(d) => d,
        None => state::default_pasture_dir()?,
    };

    // Source basename used to scope the pasture name.
    let basename = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("pasture")
        .to_string();

    // Pasture name
    let name_was_given = args.name.is_some();
    let name = match args.name {
        Some(n) => {
            if n.contains('/') {
                validate_name(&n)?;
                n
            } else {
                validate_name(&n)?;
                format!("{}/{}", basename, n)
            }
        }
        None => state.next_scoped_name(&basename),
    };

    // Default: use the unscoped part of the pasture name as the branch when
    // the caller gave an explicit name and did not pass --branch or --no-branch.
    let name_suffix = name.rsplit('/').next().unwrap_or(&name).to_string();
    let branch_arg = if args.branch.is_none() && !args.no_branch && name_was_given {
        Some(name_suffix.clone())
    } else {
        args.branch
    };

    // Uniqueness check
    if state.get(&name).is_some() {
        bail!("A pasture named '{}' already exists. Choose a different name.", name);
    }

    // With --dir, use just the unscoped name suffix as the directory name.
    let dest = if has_custom_dir {
        pasture_dir.join(&name_suffix)
    } else {
        pasture_dir.join(&name)
    };
    if dest.exists() {
        bail!(
            "Destination '{}' already exists on disk. \
             Remove it first or choose a different name.",
            dest.display()
        );
    }

    // Create all parent directories up to but not including dest itself.
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create pasture directory: {}", parent.display()))?;
    }

    // --worktree path: delegate entirely to git worktree add.
    if args.worktree {
        return run_worktree(&source, &dest, &name, branch_arg.as_deref(), args.print_path, &mut state);
    }

    // Detect large dirs to symlink (macOS + git only; skip when --no-symlink).
    // Split into dep dirs (per-package symlinks) and plain large dirs (whole-dir symlinks).
    #[cfg(target_os = "macos")]
    let (dep_candidates, whole_candidates): (Vec<(PathBuf, usize)>, Vec<(PathBuf, usize)>) = {
        if !args.no_symlink && detected_vcs == Vcs::Git {
            let threshold = read_symlink_threshold(&source).unwrap_or(10_000);
            let all = find_symlink_candidates(&source, threshold)?;
            all.into_iter().partition(|(p, _)| is_dep_dir(p))
        } else {
            (Vec::new(), Vec::new())
        }
    };
    #[cfg(not(target_os = "macos"))]
    let (dep_candidates, whole_candidates): (Vec<(PathBuf, usize)>, Vec<(PathBuf, usize)>) =
        (Vec::new(), Vec::new());

    // Print symlink warning BEFORE any other output.
    let has_any_candidates = !dep_candidates.is_empty() || !whole_candidates.is_empty();
    if has_any_candidates && !args.print_path {
        println!("⚠  Large directories handled to avoid inode overhead:");
        for (path, _) in &dep_candidates {
            let pkg_count = std::fs::read_dir(source.join(path))
                .map(|rd| rd.count())
                .unwrap_or(0);
            println!(
                "     {}/   {}  (per-package, new installs will be local)",
                path.display(),
                format_count(pkg_count)
            );
        }
        for (path, count) in &whole_candidates {
            println!(
                "     {}/   {}  (shared with source — writes affect source)",
                path.display(),
                format_count(*count)
            );
        }
        println!("   To fully clone: cow materialise {}", name);
        println!();
    }

    // Regular output
    if !args.print_path {
        println!("Detected VCS: {}", detected_vcs);
        println!("🐄 Cloning {} ...", source.display());
    }
    cow_clone(&source, &dest, &detected_vcs, &whole_candidates, &dep_candidates)?;

    // All post-clone steps. If any fail the clone is rolled back.
    let symlinked_dir_strings: Vec<String> = whole_candidates
        .iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    let linked_dir_strings: Vec<String> = dep_candidates
        .iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    let setup_result = post_clone_setup(
        &source,
        &dest,
        &detected_vcs,
        branch_arg.as_deref(),
        args.change.as_deref(),
        args.from.as_deref(),
        args.message.as_deref(),
        args.no_clean,
        &name,
        symlinked_dir_strings,
        linked_dir_strings,
        &mut state,
    );

    if let Err(ref e) = setup_result {
        eprintln!("cow: pasture setup failed — rolling back: {:#}", e);
        rollback_clone(&source, &dest, &detected_vcs);
        return setup_result;
    }

    if args.print_path {
        println!("{}", dest.display());
    } else {
        let has_cow_json = source.join(".cow.json").exists();
        println!("🐄 Created pasture '{}' at {}", name, dest.display());
        println!("To remove: cow remove {}", name);
        if !has_cow_json {
            println!(
                "Tip: add a .cow.json to remove stale build dirs or run post-clone setup. \
                 See the README for details."
            );
        }
    }
    Ok(())
}

/// Create a pasture as a git linked worktree instead of a CoW clone.
fn run_worktree(
    source: &Path,
    dest: &Path,
    name: &str,
    branch: Option<&str>,
    print_path: bool,
    state: &mut State,
) -> Result<()> {
    // Build the git worktree add command.
    let mut cmd_args = vec!["worktree", "add"];

    // Determine whether to create a new branch, check out an existing one,
    // or detach HEAD.
    let branch_exists = branch.map(|b| git_branch_exists(source, b)).unwrap_or(false);
    let branch_in_use = branch.map(|b| git_branch_in_worktree(source, b)).unwrap_or(false);

    if branch_in_use {
        bail!(
            "Branch '{}' is already checked out in another worktree. \
             Choose a different name, or use --no-branch to detach.",
            branch.unwrap()
        );
    }

    // Collect extra args before appending dest/branch so lifetimes work.
    let new_branch_flag;
    let branch_name;
    let extra: Vec<&str> = if let Some(b) = branch {
        if branch_exists {
            branch_name = b.to_string();
            cmd_args.push(dest.to_str().unwrap());
            cmd_args.push(&branch_name);
            vec![]
        } else {
            new_branch_flag = format!("-b");
            branch_name = b.to_string();
            cmd_args.push(&new_branch_flag);
            cmd_args.push(&branch_name);
            cmd_args.push(dest.to_str().unwrap());
            vec![]
        }
    } else {
        cmd_args.push("--detach");
        cmd_args.push(dest.to_str().unwrap());
        vec![]
    };
    let _ = extra; // suppress unused warning

    if !print_path {
        println!("Detected VCS: git");
        println!("🐄 Creating worktree at {} ...", dest.display());
    }

    let status = Command::new("git")
        .args(&cmd_args)
        .current_dir(source)
        .status()
        .context("Failed to run git worktree add")?;
    if !status.success() {
        bail!("git worktree add failed for '{}'.", dest.display());
    }

    let initial_commit = vcs::git_head_sha(dest);
    let resolved_branch = branch.map(|b| b.to_string())
        .or_else(|| vcs::git_current_branch(dest));

    let entry = PastureEntry {
        name: name.to_string(),
        path: dest.to_path_buf(),
        source: source.to_path_buf(),
        vcs: Vcs::Git,
        branch: resolved_branch,
        initial_commit,
        created_at: chrono::Utc::now(),
        symlinked_dirs: Vec::new(),
        linked_dirs: Vec::new(),
        is_worktree: true,
    };
    state.add(entry.clone());
    state.save()?;
    write_context_file(&entry)?;

    if print_path {
        println!("{}", dest.display());
    } else {
        println!("🐄 Created worktree pasture '{}' at {}", name, dest.display());
        println!("To remove: cow remove {}", name);
    }
    Ok(())
}

/// Execute all setup steps that follow the initial CoW clone.
/// Extracted so that `run()` can roll back atomically on any failure.
fn post_clone_setup(
    source: &Path,
    dest: &Path,
    detected_vcs: &Vcs,
    branch_arg: Option<&str>,
    change_arg: Option<&str>,
    from_arg: Option<&str>,
    message: Option<&str>,
    no_clean: bool,
    name: &str,
    symlinked_dirs: Vec<String>,
    linked_dirs: Vec<String>,
    state: &mut State,
) -> Result<()> {
    let initial_commit = if *detected_vcs == Vcs::Git {
        vcs::git_head_sha(dest)
    } else {
        // tarpaulin-ignore-start
        None
        // tarpaulin-ignore-end
    };

    let branch = match detected_vcs {
        Vcs::Git => setup_git(dest, branch_arg)?,
        // tarpaulin-ignore-start
        Vcs::Jj => {
            setup_jj(dest, change_arg, from_arg, message)?;
            None
        }
        // tarpaulin-ignore-end
    };

    if !no_clean {
        cleanup_runtime_artefacts(dest)?;
        let config_path = source.join(".cow.json");
        if config_path.exists() {
            cleanup_from_config(dest, &config_path)?;
        }
    }

    let entry = PastureEntry {
        name: name.to_string(),
        path: dest.to_path_buf(),
        source: source.to_path_buf(),
        vcs: detected_vcs.clone(),
        branch,
        initial_commit,
        created_at: chrono::Utc::now(),
        symlinked_dirs,
        linked_dirs,
        is_worktree: false,
    };
    state.add(entry.clone());
    state.save()?;
    write_context_file(&entry)?;
    Ok(())
}

/// Undo a partial clone: forget the jj workspace record (if jj) then remove
/// the cloned directory.
fn rollback_clone(source: &Path, dest: &Path, vcs: &Vcs) {
    // tarpaulin-ignore-start
    if *vcs == Vcs::Jj {
        if let Some(ws_name) = dest.file_name().and_then(|n| n.to_str()) {
            let _ = Command::new("jj")
                .args(["workspace", "forget", ws_name])
                .current_dir(source)
                .status();
        }
    }
    // tarpaulin-ignore-end
    if dest.exists() {
        if let Err(e) = std::fs::remove_dir_all(dest) {
            eprintln!(
                "cow: warning — could not remove partial pasture at '{}': {}",
                dest.display(),
                e
            );
        }
    }
}

/// Write a context file so agents can orient themselves without shelling out to
/// `cow status`.
///
/// For git workspaces the file is `.cow-context` in the repo root, excluded
/// from git tracking via `.git/info/exclude` so it never appears as untracked.
///
/// For jj workspaces the file is `.jj/cow-context`. jj does not scan inside
/// `.jj/` for working-copy changes, so the file is invisible to `jj diff`.
fn write_context_file(entry: &PastureEntry) -> Result<()> {
    let ctx = serde_json::json!({
        "name": entry.name,
        "source": entry.source.to_string_lossy(),
        "branch": entry.branch,
        "vcs": entry.vcs.to_string(),
        "initial_commit": entry.initial_commit,
        "created_at": entry.created_at.to_rfc3339(),
    });
    let ctx_content = serde_json::to_string_pretty(&ctx)?;

    if entry.vcs == Vcs::Jj {
        // Store inside .jj/ — invisible to jj's working-copy tracking.
        let ctx_path = entry.path.join(".jj").join("cow-context");
        std::fs::write(&ctx_path, ctx_content)
            .with_context(|| format!("Failed to write .jj/cow-context to {}", ctx_path.display()))?;
        return Ok(());
    }

    // Git: write .cow-context and agent orientation files, then exclude all
    // of them so they never appear as untracked.
    let ctx_path = entry.path.join(".cow-context");
    std::fs::write(&ctx_path, ctx_content)
        .with_context(|| format!("Failed to write .cow-context to {}", ctx_path.display()))?;

    // For scoped names (project/branch) write agent files one level up, into
    // the scope directory (~/.cow/pastures/project/). That directory is just a
    // container — it has no cloned content — so there is nothing to clobber.
    // For unscoped names the workspace IS the scope directory, so write there
    // but skip any file that already exists (could be the project's own).
    let agent_dir = if entry.name.contains('/') {
        entry.path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| entry.path.clone())
    } else {
        entry.path.clone()
    };

    let claude_md_paths = collect_claude_md_paths(&entry.source);
    let agents_md = build_agents_md(&claude_md_paths, &entry.source);
    let redirect = "See [AGENTS.md](./AGENTS.md) for pasture context and project instructions.\n";
    for (name, content) in &[
        ("AGENTS.md", agents_md.as_str()),
        ("CLAUDE.md", redirect),
        ("GEMINI.md", redirect),
    ] {
        let path = agent_dir.join(name);
        if !path.exists() {
            std::fs::write(&path, content)
                .with_context(|| format!("Failed to write {}", path.display()))?;
        }
    }

    // Update .git/info/exclude in the workspace. Agent files in the scope
    // directory are outside the git repo and need no exclusion; files written
    // directly into the workspace (unscoped case) do.
    let exclude_path = entry.path.join(".git").join("info").join("exclude");
    if let Ok(existing) = std::fs::read_to_string(&exclude_path) {
        let mut content = existing;
        let names: &[&str] = if entry.name.contains('/') {
            &[".cow-context"]
        } else {
            &[".cow-context", "AGENTS.md", "CLAUDE.md", "GEMINI.md"]
        };
        for name in names {
            if !content.contains(name) {
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(name);
                content.push('\n');
            }
        }
        std::fs::write(&exclude_path, &content)
            .with_context(|| format!("Failed to update {}", exclude_path.display()))?;
    }

    Ok(())
}

/// Walk from `source` up to the filesystem root and collect paths of any
/// `CLAUDE.md` files found, ordered from root (most general) to source
/// (most specific).
fn collect_claude_md_paths(source: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut dir = source.to_path_buf();
    loop {
        let candidate = dir.join("CLAUDE.md");
        if candidate.exists() {
            paths.push(candidate);
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => break,
        }
    }
    paths.reverse();
    paths
}

/// Build the content of the `AGENTS.md` orientation file.
///
/// Written once to the scope directory and shared by all workspaces for the
/// same project, so it contains only project-level information (no per-branch
/// or per-commit details — those live in each workspace's `.cow-context`).
fn build_agents_md(claude_md_paths: &[PathBuf], source: &Path) -> String {
    let instructions_block = if claude_md_paths.is_empty() {
        String::from("*(No CLAUDE.md files found in the source hierarchy.)*\n")
    } else {
        claude_md_paths
            .iter()
            .map(|p| format!("- {}\n", p.display()))
            .collect()
    };

    format!(
        "# Cow Pastures\n\
         \n\
         This directory contains [cow](https://github.com/joeinnes/cow) \
         pastures cloned from:\n\
         \n\
         `{source}`\n\
         \n\
         Each subdirectory is a pasture — an independent copy-on-write clone. \
         See `.cow-context` inside each one for its name, branch, and initial commit SHA.\n\
         \n\
         ## Project instructions\n\
         \n\
         Read the following files for project-level conventions and tooling guidance \
         (most general to most specific):\n\
         \n\
         {instructions}\n\
         ## Cow commands\n\
         \n\
         Run from inside any pasture:\n\
         \n\
         - `cow status` — pasture status and diff summary\n\
         - `cow sync` — rebase onto the latest source branch\n\
         - `cow extract --patch <file>` — export changes as a patch\n\
         - `cow extract --branch <name>` — push changes as a branch to origin\n\
         - `cow remove <name>` — delete a pasture\n\
         - `cow list` — list all pastures\n\
         ",
        source = source.display(),
        instructions = instructions_block,
    )
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Pasture name cannot be empty.");
    }
    if name.contains('\0') {
        bail!("Pasture name '{}' contains invalid characters.", name);
    }
    if name.starts_with('/') || name.ends_with('/') {
        bail!("Pasture name '{}' contains invalid characters.", name);
    }
    // At most one '/' — more than one means multiple path components.
    if name.chars().filter(|&c| c == '/').count() > 1 {
        bail!("Pasture name '{}' contains invalid characters.", name);
    }
    // Neither part may be '.' or '..'.
    for part in name.split('/') {
        if part == "." || part == ".." {
            bail!("Pasture name '{}' is not allowed.", name);
        }
    }
    Ok(())
}

/// Clone `src` to `dst` using `clonefile(2)`. `dst` must not already exist.
/// Atomically clones the entire directory tree in O(1) kernel time on APFS.
#[cfg(target_os = "macos")]
fn clonefile_dir(src: &Path, dst: &Path) -> Result<()> {
    use std::ffi::CString;
    let src_c = CString::new(
        src.to_str().with_context(|| format!("Source path is not valid UTF-8: {}", src.display()))?,
    )
    .context("Source path contains a null byte")?;
    let dst_c = CString::new(
        dst.to_str().with_context(|| format!("Dest path is not valid UTF-8: {}", dst.display()))?,
    )
    .context("Dest path contains a null byte")?;
    let ret = unsafe { libc::clonefile(src_c.as_ptr(), dst_c.as_ptr(), 0) };
    if ret != 0 {
        bail!(
            "clonefile failed '{}' to '{}': {}",
            src.display(),
            dst.display(),
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

/// Copies every top-level entry from `source` to `dest` except `.jj/`,
/// using `clonefile(2)` for each entry.
#[cfg(all(target_os = "macos", test))]
fn jj_copy_working_tree(source: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)
        .with_context(|| format!("Failed to create destination directory: {}", dest.display()))?;

    for entry in std::fs::read_dir(source)
        .with_context(|| format!("Failed to read source directory: {}", source.display()))?
    {
        let entry = entry?;
        if entry.file_name() == ".jj" {
            continue;
        }
        let src = entry.path();
        let dst = dest.join(entry.file_name());
        clonefile_dir(&src, &dst)
            .with_context(|| format!("Failed to clone '{}' to '{}'", src.display(), dst.display()))?;
    }

    Ok(())
}

/// CoW clone for jj repos: runs `jj workspace add <dest>` first (dest must not
/// yet exist), then copies any source entries that jj did not materialise
/// (untracked files: node_modules, build artefacts, .env, etc.) using
/// clonefile(2). Order matters — jj rejects a non-empty destination.
// tarpaulin-ignore-start
#[cfg(target_os = "macos")]
fn jj_cow_clone(source: &Path, dest: &Path) -> Result<()> {
    // Step 1: let jj create and initialise the workspace.
    // Disable commit signing for this invocation — the empty workspace-root
    // commit does not need to be signed, and prompting for an SSH passphrase
    // mid-clone is surprising UX.
    let status = Command::new("jj")
        .args([
            "--config", "signing.behavior=\"drop\"",
            "workspace", "add", dest.to_str().unwrap(),
        ])
        .current_dir(source)
        .status()
        .context("Failed to run jj workspace add")?;
    if !status.success() {
        bail!("jj workspace add failed for '{}'.", dest.display());
    }

    // Step 2: copy untracked source entries that jj did not materialise.
    for entry in std::fs::read_dir(source)
        .with_context(|| format!("Failed to read source directory: {}", source.display()))?
    {
        let entry = entry?;
        if entry.file_name() == ".jj" {
            continue;
        }
        let dst = dest.join(entry.file_name());
        if dst.exists() {
            continue; // jj already materialised this entry
        }
        clonefile_dir(&entry.path(), &dst)
            .with_context(|| format!("Failed to clone '{}'", entry.path().display()))?;
    }

    Ok(())
}
// tarpaulin-ignore-end

fn cow_clone(
    source: &Path,
    dest: &Path,
    vcs: &Vcs,
    whole_candidates: &[(PathBuf, usize)],
    dep_candidates: &[(PathBuf, usize)],
) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        if *vcs == Vcs::Jj {
            return jj_cow_clone(source, dest);
        }
        if whole_candidates.is_empty() && dep_candidates.is_empty() {
            // Fast path: single clonefile(2) syscall, O(1) on APFS.
            return clonefile_dir(source, dest);
        }
        // Selective clone: per-package for dep dirs, whole-dir for others, clonefile the rest.
        let whole_paths: Vec<PathBuf> = whole_candidates.iter().map(|(p, _)| p.clone()).collect();
        let dep_paths: Vec<PathBuf> = dep_candidates.iter().map(|(p, _)| p.clone()).collect();
        return selective_clone(source, source, dest, &whole_paths, &dep_paths);
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Linux: attempt copy-on-write via cp --reflink=always (btrfs, xfs).
        // Fall back to a regular copy with a warning if the filesystem does not support it.
        let reflink_status = Command::new("cp")
            .args(["--reflink=always", "-R", source.to_str().unwrap(), dest.to_str().unwrap()])
            .status();

        match reflink_status {
            Ok(s) if s.success() => return Ok(()),
            _ => {
                eprintln!(
                    "Warning: filesystem does not support reflinks (btrfs/xfs required). \
                     Falling back to a regular copy — disk overhead will be higher."
                );
                // Clean up any partial output from the failed reflink attempt.
                let _ = std::fs::remove_dir_all(dest);

                let status = Command::new("cp")
                    .args(["-R", source.to_str().unwrap(), dest.to_str().unwrap()])
                    .status()
                    .context("Failed to run cp")?;
                if !status.success() {
                    bail!(
                        "cp -R failed when cloning '{}' to '{}'.",
                        source.display(),
                        dest.display()
                    );
                }
                return Ok(());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Symlink candidate detection
// ---------------------------------------------------------------------------

/// Walk `dir` recursively (post-order). Returns the total entry count for
/// `dir`'s subtree. For each subdirectory that exceeds `threshold` and has no
/// large descendant already identified, records it as a symlink candidate.
fn collect_candidates(
    root: &Path,
    dir: &Path,
    threshold: usize,
    candidates: &mut Vec<(PathBuf, usize)>,
) -> Result<usize> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(0),
    };
    let mut total: usize = 0;
    for entry in rd.flatten() {
        // Never descend into or count VCS metadata — symlinking .git/.jj would break things.
        let name = entry.file_name();
        if matches!(name.to_string_lossy().as_ref(), ".git" | ".jj") {
            continue;
        }
        total += 1;
        if let Ok(ft) = entry.file_type() {
            if ft.is_dir() {
                let path = entry.path();
                let before = candidates.len();
                let sub = collect_candidates(root, &path, threshold, candidates)?;
                total = total.saturating_add(sub);
                // If no deeper candidate was found inside this subdir and the
                // subdir itself is large, it is the deepest large directory.
                if candidates.len() == before && sub > threshold {
                    if let Ok(rel) = path.strip_prefix(root) {
                        candidates.push((rel.to_path_buf(), sub));
                    }
                }
            }
        }
    }
    Ok(total)
}

/// Find the minimal set of large directories in `source`. Returns each as
/// (relative path, total entry count). "Minimal" means we prefer deeper dirs
/// — e.g. `homepage/node_modules/` over `homepage/` itself.
fn find_symlink_candidates(source: &Path, threshold: usize) -> Result<Vec<(PathBuf, usize)>> {
    let mut candidates = Vec::new();
    collect_candidates(source, source, threshold, &mut candidates)?;
    Ok(candidates)
}

/// Read `pre_clone.symlink_threshold` from `.cow.json` in `source`, if present.
fn read_symlink_threshold(source: &Path) -> Option<usize> {
    #[derive(serde::Deserialize)]
    struct CowConfig { pre_clone: Option<PreClone> }
    #[derive(serde::Deserialize)]
    struct PreClone { symlink_threshold: Option<usize> }
    let content = std::fs::read_to_string(source.join(".cow.json")).ok()?;
    let cfg: CowConfig = serde_json::from_str(&content).ok()?;
    cfg.pre_clone?.symlink_threshold
}

/// Format a count with comma separators, e.g. 382760 → "382,760".
fn format_count(n: usize) -> String {
    let s = n.to_string();
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result
}

// ---------------------------------------------------------------------------
// Selective clone (macOS — replaces the single clonefile(2) call when
// large dirs are present)
// ---------------------------------------------------------------------------

/// Clone `source` into `dest`:
/// - whole-dir symlink for entries in `whole_candidates`
/// - per-package symlinks (real dir, each top-level entry symlinked) for entries in `dep_candidates`
/// - recurse into dirs that contain any candidate descendant
/// - clonefile everything else
#[cfg(target_os = "macos")]
fn selective_clone(
    orig_source: &Path,
    source: &Path,
    dest: &Path,
    whole_candidates: &[PathBuf],
    dep_candidates: &[PathBuf],
) -> Result<()> {
    std::fs::create_dir_all(dest)
        .with_context(|| format!("Failed to create directory: {}", dest.display()))?;

    for entry in std::fs::read_dir(source)
        .with_context(|| format!("Failed to read directory: {}", source.display()))?
    {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dest.join(entry.file_name());
        let rel = src_path
            .strip_prefix(orig_source)
            .expect("source is always under orig_source");
        let rel_buf = rel.to_path_buf();

        if whole_candidates.contains(&rel_buf) {
            // Whole-dir symlink (plain large dir — writes affect source).
            std::os::unix::fs::symlink(&src_path, &dst_path)
                .with_context(|| format!("Failed to symlink '{}'", src_path.display()))?;
        } else if dep_candidates.contains(&rel_buf) {
            // Per-package symlinks: create a real dir, then handle each entry.
            //
            // pnpm uses a virtual store: top-level node_modules entries are
            // relative symlinks into .pnpm/ (e.g. next → .pnpm/next@x/node_modules/next).
            // If we symlink them to the *source* copies (absolute paths), tools
            // like Turbopack see them as crossing the pasture boundary and refuse
            // to follow them. Detect pnpm by the presence of .pnpm/ and instead:
            //   - whole-dir symlink .pnpm → source/.pnpm  (one pointer to the store)
            //   - copy other relative symlinks verbatim so they resolve within
            //     the pasture's own .pnpm/ tree, never leaving the pasture root.
            std::fs::create_dir_all(&dst_path)
                .with_context(|| format!("Failed to create dir '{}'", dst_path.display()))?;
            let is_pnpm = src_path.join(".pnpm").exists();
            for child in std::fs::read_dir(&src_path)
                .with_context(|| format!("Failed to read dep dir '{}'", src_path.display()))?
            {
                let child = child?;
                let child_dst = dst_path.join(child.file_name());
                if is_pnpm && child.file_name() == ".pnpm" {
                    // Symlink the entire virtual store directory to the source.
                    std::os::unix::fs::symlink(child.path(), &child_dst)
                        .with_context(|| format!("Failed to symlink .pnpm '{}'", child.path().display()))?;
                } else if is_pnpm && child.file_type()?.is_symlink() {
                    // Copy the relative symlink verbatim — it resolves within
                    // the pasture's own .pnpm/ rather than crossing the boundary.
                    let target = std::fs::read_link(child.path())
                        .with_context(|| format!("Failed to read symlink '{}'", child.path().display()))?;
                    std::os::unix::fs::symlink(&target, &child_dst)
                        .with_context(|| format!("Failed to recreate symlink '{}'", child_dst.display()))?;
                } else {
                    std::os::unix::fs::symlink(child.path(), &child_dst)
                        .with_context(|| format!("Failed to symlink package '{}'", child.path().display()))?;
                }
            }
        } else if entry.file_type()?.is_dir()
            && (whole_candidates.iter().any(|c| c.starts_with(rel) && c != rel)
                || dep_candidates.iter().any(|c| c.starts_with(rel) && c != rel))
        {
            // Has a candidate descendant — recurse.
            selective_clone(orig_source, &src_path, &dst_path, whole_candidates, dep_candidates)?;
        } else {
            clonefile_dir(&src_path, &dst_path)
                .with_context(|| format!("Failed to clone '{}'", src_path.display()))?;
        }
    }
    Ok(())
}

/// Returns true if the last component of `rel` is a known dependency directory name.
fn is_dep_dir(rel: &Path) -> bool {
    rel.file_name()
        .and_then(|n| n.to_str())
        .map(|n| DEP_DIR_NAMES.contains(&n))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Git worktree helpers (used by run_worktree)
// ---------------------------------------------------------------------------

/// Returns true if `branch` exists in `source` repo (local branch).
fn git_branch_exists(source: &Path, branch: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{}", branch)])
        .current_dir(source)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Returns true if `branch` is currently checked out in any worktree of `source`.
fn git_branch_in_worktree(source: &Path, branch: &str) -> bool {
    let Ok(output) = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(source)
        .output()
    else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let target = format!("branch refs/heads/{}", branch);
    text.lines().any(|line| line == target)
}

fn setup_git(workspace: &Path, branch: Option<&str>) -> Result<Option<String>> {
    // Remove stale worktree refs inherited from the CoW clone. The source repo
    // may have had linked worktrees whose absolute paths are baked into
    // .git/worktrees/. Those entries are invalid in the clone: git worktree
    // prune won't remove them reliably because the original directories still
    // exist on disk (git only checks existence, not whether the back-link
    // points to this repo). Deleting the directory is safe — a fresh workspace
    // starts with no linked worktrees.
    let worktrees_dir = workspace.join(".git").join("worktrees");
    if worktrees_dir.exists() {
        let _ = std::fs::remove_dir_all(&worktrees_dir);
    }

    let Some(branch) = branch else {
        return Ok(vcs::git_current_branch(workspace));
    };

    // Try checkout, fall back to creating the branch.
    // Suppress stderr on the first attempt — a missing branch causes git to
    // print "pathspec did not match" which is expected noise, not an error.
    let status = Command::new("git")
        .args(["checkout", branch])
        .current_dir(workspace)
        .stderr(std::process::Stdio::null())
        .status()
        .context("Failed to run git checkout")?;

    if !status.success() {
        let status = Command::new("git")
            .args(["checkout", "-b", branch])
            .current_dir(workspace)
            .status()
            .context("Failed to run git checkout -b")?;
        if !status.success() {
            bail!("Failed to check out branch '{}' in pasture.", branch);
        }
    }

    Ok(Some(branch.to_string()))
}

// tarpaulin-ignore-start
fn setup_jj(workspace: &Path, change: Option<&str>, from: Option<&str>, message: Option<&str>) -> Result<()> {
    // The cp -Rc clone already has its own .jj/ at a different path, so it is
    // independent from the source without any extra steps.

    if let Some(rev) = from {
        // --from: create a new change on top of the given revision.
        let status = Command::new("jj")
            .args(["new", rev])
            .current_dir(workspace)
            .status()
            .context("Failed to run jj new")?;
        if !status.success() {
            bail!(
                "Failed to create a new change from '{}' in workspace.\n\
                 If '{}' is immutable, try passing a mutable descendant instead.",
                rev, rev
            );
        }
    } else if let Some(change_id) = change {
        let status = Command::new("jj")
            .args(["edit", change_id])
            .current_dir(workspace)
            .status()
            .context("Failed to run jj edit")?;
        if !status.success() {
            bail!(
                "Failed to edit change '{}' in workspace.\n\
                 If '{}' is immutable, use --from {} to create a new change on top.",
                change_id, change_id, change_id
            );
        }
    }

    if let Some(msg) = message {
        let status = Command::new("jj")
            .args(["describe", "-m", msg])
            .current_dir(workspace)
            .status()
            .context("Failed to run jj describe")?;
        if !status.success() {
            bail!("Failed to set initial change description in pasture.");
        }
    }

    Ok(())
}
// tarpaulin-ignore-end

fn cleanup_runtime_artefacts(workspace: &Path) -> Result<()> {
    for pattern in &["*.pid", "*.sock", "*.socket"] {
        remove_glob(workspace, pattern)?;
    }
    Ok(())
}

fn cleanup_from_config(workspace: &Path, config_path: &Path) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct CowConfig {
        post_clone: Option<PostClone>,
    }
    #[derive(serde::Deserialize)]
    struct PostClone {
        remove: Option<Vec<String>>,
        run: Option<Vec<String>>,
    }

    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read .cow.json: {}", config_path.display()))?;
    let config: CowConfig = serde_json::from_str(&content)
        .with_context(|| "Failed to parse .cow.json")?;

    let Some(post_clone) = config.post_clone else {
        return Ok(());
    };

    if let Some(patterns) = post_clone.remove {
        for pattern in &patterns {
            remove_glob_or_dir(workspace, pattern)?;
        }
    }

    if let Some(commands) = post_clone.run {
        for cmd in &commands {
            println!("Running post-clone: {}", cmd);
            let status = Command::new("sh")
                .args(["-c", cmd])
                .current_dir(workspace)
                .status()
                .with_context(|| format!("Failed to run post-clone command: {}", cmd))?;
            if !status.success() {
                bail!("Post-clone command failed (exit non-zero): {}", cmd);
            }
        }
    }

    Ok(())
}

fn remove_glob(base: &Path, pattern: &str) -> Result<()> {
    let full = format!("{}/{}", base.display(), pattern);
    for entry in glob::glob(&full)? {
        let path = entry?;
        if path.is_file() {
            std::fs::remove_file(&path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

fn remove_glob_or_dir(base: &Path, pattern: &str) -> Result<()> {
    let full = format!("{}/{}", base.display(), pattern);
    for entry in glob::glob(&full)? {
        let path = entry?;
        if path.is_dir() {
            std::fs::remove_dir_all(&path)
                .with_context(|| format!("Failed to remove dir {}", path.display()))?;
        } else if path.is_file() {
            std::fs::remove_file(&path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn clonefile_dir_clones_directory_tree() {
        use tempfile::TempDir;
        let src = TempDir::new().unwrap();
        let dst_parent = TempDir::new().unwrap();
        let dst = dst_parent.path().join("clone");

        std::fs::write(src.path().join("hello.txt"), "hello").unwrap();
        std::fs::create_dir(src.path().join("sub")).unwrap();
        std::fs::write(src.path().join("sub/nested.txt"), "world").unwrap();

        clonefile_dir(src.path(), &dst).unwrap();

        assert!(dst.join("hello.txt").exists(), "hello.txt should be cloned");
        assert!(dst.join("sub/nested.txt").exists(), "nested.txt should be cloned");
        assert_eq!(std::fs::read_to_string(dst.join("hello.txt")).unwrap(), "hello");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn clonefile_dir_fails_if_dest_exists() {
        use tempfile::TempDir;
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap(); // already exists

        std::fs::write(src.path().join("file.txt"), "data").unwrap();

        let err = clonefile_dir(src.path(), dst.path()).unwrap_err();
        assert!(err.to_string().contains("clonefile failed"), "unexpected: {err}");
    }

    #[test]
    fn validate_name_accepts_valid() {
        assert!(validate_name("my-workspace").is_ok());
        assert!(validate_name("agent-1").is_ok());
        assert!(validate_name("abc").is_ok());
        assert!(validate_name("project/feature-x").is_ok());
        assert!(validate_name("brightblur/cache-cleanup").is_ok());
    }

    #[test]
    fn validate_name_rejects_empty() {
        assert!(validate_name("").is_err());
    }

    #[test]
    fn validate_name_rejects_multiple_slashes() {
        assert!(validate_name("foo/bar/baz").is_err());
    }

    #[test]
    fn validate_name_rejects_leading_slash() {
        assert!(validate_name("/foo").is_err());
    }

    #[test]
    fn validate_name_rejects_trailing_slash() {
        assert!(validate_name("foo/").is_err());
    }

    #[test]
    fn validate_name_rejects_null_byte() {
        assert!(validate_name("foo\0bar").is_err());
    }

    #[test]
    fn validate_name_rejects_dot() {
        assert!(validate_name(".").is_err());
        assert!(validate_name("..").is_err());
        assert!(validate_name("./bar").is_err());
        assert!(validate_name("../bar").is_err());
        assert!(validate_name("foo/..").is_err());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn jj_cow_clone_copies_files_but_not_jj_dir() {
        use tempfile::TempDir;
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        let dst_path = dst.path().join("dest");

        // Set up a fake jj primary workspace
        std::fs::create_dir_all(src.path().join(".jj/repo")).unwrap();
        std::fs::write(src.path().join(".jj/repo/big-object"), "fake git data").unwrap();
        std::fs::write(src.path().join("src.rs"), "fn main() {}").unwrap();
        std::fs::create_dir(src.path().join("node_modules")).unwrap();
        std::fs::write(src.path().join("node_modules/dep.js"), "module.exports=1").unwrap();

        jj_copy_working_tree(src.path(), &dst_path).unwrap();

        assert!(dst_path.join("src.rs").exists(), "src.rs should be copied");
        assert!(dst_path.join("node_modules/dep.js").exists(), "node_modules should be copied");
        assert!(!dst_path.join(".jj").exists(), ".jj should NOT be copied");
    }

    #[test]
    fn run_rejects_jj_secondary_workspace() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        // .jj/ with no .jj/repo/ → secondary workspace
        std::fs::create_dir(dir.path().join(".jj")).unwrap();
        let args = crate::cli::CreateArgs {
            source: Some(dir.path().to_path_buf()),
            name: Some("test-ws".into()),
            branch: None,
            no_branch: true,
            dir: None,
            no_clean: true,
            change: None,
            from: None,
            message: None,
            print_path: false,
            no_symlink: true,
            worktree: false,
        };
        let err = run(args).unwrap_err();
        assert!(
            err.to_string().contains("jj workspace"),
            "unexpected error: {err}"
        );
    }
}

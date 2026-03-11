use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

use crate::{
    cli::CreateArgs,
    state::{self, State, WorkspaceEntry},
    vcs::{self, Vcs},
};
#[cfg(target_os = "macos")]
use crate::apfs;

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

    // APFS check (macOS only — on Linux we attempt reflink and fall back gracefully)
    // tarpaulin-ignore-start
    #[cfg(target_os = "macos")]
    if !apfs::is_apfs(&source) {
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
             Submodule support is untested and may produce a broken workspace."
        );
    }

    // Load + prune state
    let mut state = State::load()?;
    state.prune_deleted();

    // Workspace parent directory
    let has_custom_dir = args.dir.is_some();
    let workspace_dir = match args.dir {
        Some(d) => d,
        None => state::default_workspace_dir()?,
    };

    // Source basename used to scope the workspace name.
    let basename = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace")
        .to_string();

    // Workspace name
    let name_was_given = args.name.is_some();
    let name = match args.name {
        Some(n) => {
            if n.contains('/') {
                // User supplied an explicit scope — validate the whole thing.
                validate_name(&n)?;
                n
            } else {
                // Auto-scope: prepend source basename.
                validate_name(&n)?;
                format!("{}/{}", basename, n)
            }
        }
        None => state.next_scoped_name(&basename),
    };

    // Default: use the unscoped part of the workspace name as the branch when
    // the caller gave an explicit name and did not pass --branch or --no-branch.
    let name_suffix = name.rsplit('/').next().unwrap_or(&name).to_string();
    let branch_arg = if args.branch.is_none() && !args.no_branch && name_was_given {
        Some(name_suffix.clone())
    } else {
        args.branch
    };

    // Uniqueness check
    if state.get(&name).is_some() {
        bail!("A workspace named '{}' already exists. Choose a different name.", name);
    }

    // With --dir, use just the unscoped name suffix as the directory name so
    // the workspace lands exactly at <dir>/<suffix> rather than <dir>/<scope>/<suffix>.
    let dest = if has_custom_dir {
        workspace_dir.join(&name_suffix)
    } else {
        workspace_dir.join(&name)
    };
    if dest.exists() {
        bail!(
            "Destination '{}' already exists on disk. \
             Remove it first or choose a different name.",
            dest.display()
        );
    }

    // dest may have a sub-directory component (e.g. workspaces/project/name),
    // so create all parents up to but not including dest itself.
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create workspace directory: {}", parent.display()))?;
    }

    // CoW clone
    if !args.print_path {
        println!("Cloning {} ...", source.display());
    }
    cow_clone(&source, &dest, &detected_vcs)?;

    // All post-clone steps. If any fail the clone is rolled back so the user
    // is never left with an orphaned directory or dangling jj workspace record.
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
        &mut state,
    );

    if let Err(ref e) = setup_result {
        eprintln!("cow: workspace setup failed — rolling back: {:#}", e);
        rollback_clone(&source, &dest, &detected_vcs);
        return setup_result;
    }

    if args.print_path {
        println!("{}", dest.display());
    } else {
        let has_cow_json = source.join(".cow.json").exists();
        println!("Created workspace '{}' at {}", name, dest.display());
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

    let entry = WorkspaceEntry {
        name: name.to_string(),
        path: dest.to_path_buf(),
        source: source.to_path_buf(),
        vcs: detected_vcs.clone(),
        branch,
        initial_commit,
        created_at: chrono::Utc::now(),
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
                "cow: warning — could not remove partial workspace at '{}': {}",
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
fn write_context_file(entry: &WorkspaceEntry) -> Result<()> {
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

    // Git: write to root and exclude from git tracking.
    let ctx_path = entry.path.join(".cow-context");
    std::fs::write(&ctx_path, ctx_content)
        .with_context(|| format!("Failed to write .cow-context to {}", ctx_path.display()))?;

    let exclude_path = entry.path.join(".git").join("info").join("exclude");
    if let Ok(existing) = std::fs::read_to_string(&exclude_path) {
        if !existing.contains(".cow-context") {
            let mut content = existing;
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(".cow-context\n");
            std::fs::write(&exclude_path, content)
                .with_context(|| format!("Failed to update {}", exclude_path.display()))?;
        }
    }

    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Workspace name cannot be empty.");
    }
    if name.contains('\0') {
        bail!("Workspace name '{}' contains invalid characters.", name);
    }
    if name.starts_with('/') || name.ends_with('/') {
        bail!("Workspace name '{}' contains invalid characters.", name);
    }
    // At most one '/' — more than one means multiple path components.
    if name.chars().filter(|&c| c == '/').count() > 1 {
        bail!("Workspace name '{}' contains invalid characters.", name);
    }
    // Neither part may be '.' or '..'.
    for part in name.split('/') {
        if part == "." || part == ".." {
            bail!("Workspace name '{}' is not allowed.", name);
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

fn cow_clone(source: &Path, dest: &Path, vcs: &Vcs) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        if *vcs == Vcs::Jj {
            return jj_cow_clone(source, dest);
        }
        // macOS: clonefile(2) atomically clones the entire directory tree.
        // This is a single kernel call — O(1) on APFS, no per-file traversal.
        return clonefile_dir(source, dest);
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

    // Try checkout, fall back to creating the branch
    let status = Command::new("git")
        .args(["checkout", branch])
        .current_dir(workspace)
        .status()
        .context("Failed to run git checkout")?;

    if !status.success() {
        let status = Command::new("git")
            .args(["checkout", "-b", branch])
            .current_dir(workspace)
            .status()
            .context("Failed to run git checkout -b")?;
        if !status.success() {
            bail!("Failed to check out branch '{}' in workspace.", branch);
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
            bail!("Failed to set initial change description in workspace.");
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
        };
        let err = run(args).unwrap_err();
        assert!(
            err.to_string().contains("jj workspace"),
            "unexpected error: {err}"
        );
    }
}

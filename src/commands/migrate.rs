use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{
    cli::MigrateArgs,
    state::{self, State, WorkspaceEntry},
    vcs::{self, Vcs},
};

#[derive(Debug, Clone, PartialEq)]
pub enum CandidateKind {
    /// A git linked worktree (`.git` is a file pointing at the main gitdir).
    GitWorktree,
    /// A jj secondary workspace.
    JjWorkspace { workspace_name: String },
    /// A directory that already exists in the cow workspaces dir but is not
    /// registered in state.
    Orphaned,
}

#[derive(Debug, Clone)]
pub struct MigrateCandidate {
    pub path: PathBuf,
    pub name: String,
    pub kind: CandidateKind,
    pub branch: Option<String>,
    pub is_dirty: bool,
}

pub fn run(args: MigrateArgs) -> Result<()> {
    let source = match args.source {
        Some(p) => p
            .canonicalize()
            .with_context(|| format!("Cannot resolve source path: {}", p.display()))?,
        None => std::env::current_dir().context("Cannot determine current directory")?,
    };

    let detected_vcs =
        vcs::detect_vcs(&source).context("No VCS found at source. Must be a git or jj repository.")?;

    if detected_vcs == Vcs::Git && vcs::is_git_worktree(&source) {
        bail!(
            "Source '{}' is a git worktree, not a primary repository.",
            source.display()
        );
    }
    if detected_vcs == Vcs::Jj && vcs::is_jj_secondary_workspace(&source) {
        bail!(
            "Source '{}' is a jj workspace, not a primary repository.",
            source.display()
        );
    }

    let mut state = State::load()?;
    state.prune_deleted();

    let candidates = discover_candidates(&source, &detected_vcs, &state)?;

    if candidates.is_empty() {
        println!("No candidates found to migrate.");
        return Ok(());
    }

    if !args.all {
        println!("Found {} candidate(s) to migrate:", candidates.len());
        for c in &candidates {
            let dirty_flag = if c.is_dirty { " [dirty]" } else { "" };
            println!("  {} — {}{}", c.name, c.path.display(), dirty_flag);
        }
        println!("Run with --all to migrate all of them.");
        return Ok(());
    }

    let workspace_dir = state::default_workspace_dir()?;
    let mut any_migrated = false;

    for candidate in &candidates {
        // Orphaned workspaces are registered in-place (non-destructive), so
        // the dirty check does not apply to them.
        let requires_dirty_check = candidate.kind != CandidateKind::Orphaned;
        if requires_dirty_check && candidate.is_dirty && !args.force {
            println!(
                "Skipping '{}' — has uncommitted changes (use --force to override).",
                candidate.name
            );
            continue;
        }

        if args.dry_run {
            println!("[dry-run] Would migrate '{}' ({})", candidate.name, candidate.path.display());
            continue;
        }

        match migrate_candidate(&source, &detected_vcs, candidate, &workspace_dir, &mut state) {
            Ok(()) => {
                println!("Migrated '{}'", candidate.name);
                any_migrated = true;
            }
            Err(e) => eprintln!("Failed to migrate '{}': {:#}", candidate.name, e),
        }
    }

    if any_migrated {
        state.save()?;
    }

    Ok(())
}

fn discover_candidates(source: &Path, vcs: &Vcs, state: &State) -> Result<Vec<MigrateCandidate>> {
    let mut candidates = Vec::new();

    match vcs {
        Vcs::Git => candidates.extend(discover_git_worktrees(source, state)?),
        // tarpaulin-ignore-start
        Vcs::Jj => candidates.extend(discover_jj_workspaces(source, state)?),
        // tarpaulin-ignore-end
    }

    candidates.extend(discover_orphaned(source, state)?);
    Ok(candidates)
}

fn discover_git_worktrees(source: &Path, state: &State) -> Result<Vec<MigrateCandidate>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(source)
        .output()
        .context("Failed to run git worktree list")?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let worktrees = parse_git_worktree_list(&stdout);

    let mut candidates = Vec::new();
    for (path, branch) in worktrees {
        if state.workspaces.iter().any(|w| w.path == path) {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_string());
        let is_dirty = vcs::git_is_dirty(&path);
        candidates.push(MigrateCandidate {
            path,
            name,
            kind: CandidateKind::GitWorktree,
            branch,
            is_dirty,
        });
    }

    Ok(candidates)
}

// tarpaulin-ignore-start
fn discover_jj_workspaces(source: &Path, state: &State) -> Result<Vec<MigrateCandidate>> {
    let output = Command::new("jj")
        .args(["workspace", "list"])
        .current_dir(source)
        .output()
        .context("Failed to run jj workspace list")?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ws_names = parse_jj_workspace_list(&stdout);

    let mut candidates = Vec::new();
    for ws_name in ws_names {
        let root_out = Command::new("jj")
            .args(["workspace", "root", "--workspace", &ws_name])
            .current_dir(source)
            .output();
        let Ok(root_out) = root_out else { continue };
        if !root_out.status.success() {
            continue;
        }
        let path = PathBuf::from(String::from_utf8_lossy(&root_out.stdout).trim());
        if state.workspaces.iter().any(|w| w.path == path) {
            continue;
        }
        let is_dirty = vcs::jj_is_dirty(&path);
        candidates.push(MigrateCandidate {
            path,
            name: ws_name.clone(),
            kind: CandidateKind::JjWorkspace { workspace_name: ws_name },
            branch: None,
            is_dirty,
        });
    }

    Ok(candidates)
}
// tarpaulin-ignore-end

fn discover_orphaned(source: &Path, state: &State) -> Result<Vec<MigrateCandidate>> {
    let workspace_dir = state::default_workspace_dir()?;
    if !workspace_dir.exists() {
        return Ok(vec![]);
    }

    let registered_paths: std::collections::HashSet<PathBuf> =
        state.workspaces.iter().map(|w| w.path.clone()).collect();

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&workspace_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() || registered_paths.contains(&path) {
            continue;
        }

        if let Some(ctx_source) = read_cow_context_source(&path) {
            if ctx_source == source {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "unknown".to_string());
                let ws_vcs = vcs::detect_vcs(&path);
                let branch = if ws_vcs == Some(Vcs::Git) {
                    vcs::git_current_branch(&path)
                } else {
                    None
                };
                let is_dirty = match &ws_vcs {
                    Some(Vcs::Git) => vcs::git_is_dirty(&path),
                    // tarpaulin-ignore-start
                    Some(Vcs::Jj) => vcs::jj_is_dirty(&path),
                    // tarpaulin-ignore-end
                    None => false,
                };
                candidates.push(MigrateCandidate {
                    path,
                    name,
                    kind: CandidateKind::Orphaned,
                    branch,
                    is_dirty,
                });
            }
        }
    }

    Ok(candidates)
}

fn read_cow_context_source(path: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(path.join(".cow-context")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    Some(PathBuf::from(json.get("source")?.as_str()?))
}

fn migrate_candidate(
    source: &Path,
    vcs: &Vcs,
    candidate: &MigrateCandidate,
    workspace_dir: &Path,
    state: &mut State,
) -> Result<()> {
    match &candidate.kind {
        CandidateKind::Orphaned => {
            let ws_vcs = vcs::detect_vcs(&candidate.path).unwrap_or(vcs.clone());
            let initial_commit = if ws_vcs == Vcs::Git {
                vcs::git_head_sha(&candidate.path)
            } else {
                None
            };
            state.add(WorkspaceEntry {
                name: candidate.name.clone(),
                path: candidate.path.clone(),
                source: source.to_path_buf(),
                vcs: ws_vcs,
                branch: candidate.branch.clone(),
                initial_commit,
                created_at: chrono::Utc::now(),
            });
            Ok(())
        }

        CandidateKind::GitWorktree => {
            let dest = workspace_dir.join(&candidate.name);
            if dest.exists() {
                bail!("Destination '{}' already exists.", dest.display());
            }
            if state.get(&candidate.name).is_some() {
                bail!("A workspace named '{}' already exists in state.", candidate.name);
            }

            std::fs::create_dir_all(workspace_dir)
                .context("Failed to create workspace directory")?;

            cow_clone_git(source, &dest)?;

            if let Some(ref branch) = candidate.branch {
                let status = Command::new("git")
                    .args(["checkout", branch])
                    .current_dir(&dest)
                    .status()
                    .context("Failed to run git checkout")?;
                if !status.success() {
                    eprintln!(
                        "Warning: could not check out branch '{}' in migrated workspace.",
                        branch
                    );
                }
            }

            let initial_commit = vcs::git_head_sha(&dest);
            write_context_file_git(&WorkspaceEntry {
                name: candidate.name.clone(),
                path: dest.clone(),
                source: source.to_path_buf(),
                vcs: Vcs::Git,
                branch: candidate.branch.clone(),
                initial_commit: initial_commit.clone(),
                created_at: chrono::Utc::now(),
            })?;

            state.add(WorkspaceEntry {
                name: candidate.name.clone(),
                path: dest,
                source: source.to_path_buf(),
                vcs: Vcs::Git,
                branch: candidate.branch.clone(),
                initial_commit,
                created_at: chrono::Utc::now(),
            });

            // Remove the old git worktree.
            let rm_status = Command::new("git")
                .args(["worktree", "remove", "--force", candidate.path.to_str().unwrap_or("")])
                .current_dir(source)
                .status()
                .context("Failed to run git worktree remove")?;
            if !rm_status.success() {
                eprintln!(
                    "Warning: could not remove old worktree at '{}'. \
                     You may need to remove it manually.",
                    candidate.path.display()
                );
            }

            Ok(())
        }

        // tarpaulin-ignore-start
        CandidateKind::JjWorkspace { workspace_name } => {
            let dest = workspace_dir.join(&candidate.name);
            if dest.exists() {
                bail!("Destination '{}' already exists.", dest.display());
            }
            if state.get(&candidate.name).is_some() {
                bail!("A workspace named '{}' already exists in state.", candidate.name);
            }

            std::fs::create_dir_all(workspace_dir)
                .context("Failed to create workspace directory")?;

            let add_status = Command::new("jj")
                .args([
                    "--config",
                    "signing.behavior=\"drop\"",
                    "workspace",
                    "add",
                    dest.to_str().unwrap_or(""),
                    "--name",
                    &candidate.name,
                ])
                .current_dir(source)
                .status()
                .context("Failed to run jj workspace add")?;
            if !add_status.success() {
                bail!("jj workspace add failed for '{}'.", dest.display());
            }

            state.add(WorkspaceEntry {
                name: candidate.name.clone(),
                path: dest,
                source: source.to_path_buf(),
                vcs: Vcs::Jj,
                branch: None,
                initial_commit: None,
                created_at: chrono::Utc::now(),
            });

            let forget_status = Command::new("jj")
                .args(["workspace", "forget", workspace_name])
                .current_dir(source)
                .status()
                .context("Failed to run jj workspace forget")?;
            if !forget_status.success() {
                eprintln!(
                    "Warning: could not forget old workspace '{}'. \
                     Run 'jj workspace forget {}' manually.",
                    workspace_name, workspace_name
                );
            }

            Ok(())
        }
        // tarpaulin-ignore-end
    }
}

/// APFS clone (macOS) or regular copy (Linux) of a git source repo.
fn cow_clone_git(source: &Path, dest: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::ffi::CString;
        let src_c =
            CString::new(source.to_str().context("non-UTF-8 source path")?).context("null byte in source path")?;
        let dst_c =
            CString::new(dest.to_str().context("non-UTF-8 dest path")?).context("null byte in dest path")?;
        let ret = unsafe { libc::clonefile(src_c.as_ptr(), dst_c.as_ptr(), 0) };
        if ret != 0 {
            bail!("clonefile failed: {}", std::io::Error::last_os_error());
        }
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let status = Command::new("cp")
            .args(["-R", source.to_str().unwrap_or(""), dest.to_str().unwrap_or("")])
            .status()
            .context("Failed to run cp")?;
        if !status.success() {
            bail!("cp -R failed cloning '{}' to '{}'", source.display(), dest.display());
        }
        Ok(())
    }
}

/// Write a `.cow-context` file and add it to `.git/info/exclude` so it is
/// not tracked by git.
fn write_context_file_git(entry: &WorkspaceEntry) -> Result<()> {
    let ctx = serde_json::json!({
        "name": entry.name,
        "source": entry.source.to_string_lossy(),
        "branch": entry.branch,
        "vcs": entry.vcs.to_string(),
        "initial_commit": entry.initial_commit,
        "created_at": entry.created_at.to_rfc3339(),
    });
    let ctx_path = entry.path.join(".cow-context");
    std::fs::write(&ctx_path, serde_json::to_string_pretty(&ctx)?)
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

/// Parse `git worktree list --porcelain` output.
/// Returns `(path, branch)` for each *linked* worktree — the main worktree
/// (always the first block) is excluded.
pub fn parse_git_worktree_list(output: &str) -> Vec<(PathBuf, Option<String>)> {
    let mut result = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;
    let mut is_main = true;

    for line in output.lines() {
        if let Some(path_str) = line.strip_prefix("worktree ") {
            // Flush previous block (skip the main one).
            if !is_main {
                if let Some(path) = current_path.take() {
                    result.push((path, current_branch.take()));
                }
            }
            current_path = Some(PathBuf::from(path_str));
            current_branch = None;
        } else if let Some(branch_ref) = line.strip_prefix("branch ") {
            current_branch = Some(
                branch_ref
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch_ref)
                    .to_string(),
            );
        } else if line.is_empty() {
            // End of a block.
            if is_main {
                // Discard the main worktree entry and flip the flag.
                is_main = false;
                current_path = None;
                current_branch = None;
            } else if let Some(path) = current_path.take() {
                result.push((path, current_branch.take()));
            }
        }
    }

    // Handle last block with no trailing blank line.
    if !is_main {
        if let Some(path) = current_path {
            result.push((path, current_branch));
        }
    }

    result
}

/// Parse `jj workspace list` output and return the names of all non-current
/// workspaces.
///
/// Each line looks like:
/// ```text
/// default: abc123 commit message (editing: working copy)
/// my-ws:   def456 other commit
/// ```
pub fn parse_jj_workspace_list(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let colon = line.find(':')?;
            let name = line[..colon].trim().to_string();
            let rest = &line[colon + 1..];
            if rest.contains("(editing: working copy)") || rest.contains("(current)") {
                return None;
            }
            Some(name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_git_worktree_list ───────────────────────────────────────────────

    #[test]
    fn parse_git_worktree_list_no_linked_worktrees() {
        let output = "worktree /path/to/main\nHEAD abc1234567890\nbranch refs/heads/main\n";
        assert!(
            parse_git_worktree_list(output).is_empty(),
            "only the main worktree should yield an empty list"
        );
    }

    #[test]
    fn parse_git_worktree_list_one_linked() {
        let output = "\
worktree /path/to/main
HEAD abc1234567890
branch refs/heads/main

worktree /path/to/feature
HEAD def1234567890
branch refs/heads/feature-x
";
        let result = parse_git_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, PathBuf::from("/path/to/feature"));
        assert_eq!(result[0].1, Some("feature-x".to_string()));
    }

    #[test]
    fn parse_git_worktree_list_two_linked() {
        let output = "\
worktree /path/to/main
HEAD abc1234567890
branch refs/heads/main

worktree /path/to/feat-a
HEAD def1234567890
branch refs/heads/feat-a

worktree /path/to/feat-b
HEAD ghi1234567890
branch refs/heads/feat-b

";
        let result = parse_git_worktree_list(output);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, PathBuf::from("/path/to/feat-a"));
        assert_eq!(result[1].0, PathBuf::from("/path/to/feat-b"));
    }

    #[test]
    fn parse_git_worktree_list_detached_head() {
        let output = "\
worktree /path/to/main
HEAD abc1234567890
branch refs/heads/main

worktree /path/to/detached
HEAD def1234567890
detached
";
        let result = parse_git_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, PathBuf::from("/path/to/detached"));
        assert_eq!(result[0].1, None, "detached HEAD should have no branch");
    }

    #[test]
    fn parse_git_worktree_list_no_trailing_newline() {
        let output = "\
worktree /path/to/main
HEAD abc1234567890
branch refs/heads/main

worktree /path/to/linked
HEAD def1234567890
branch refs/heads/linked";
        let result = parse_git_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, PathBuf::from("/path/to/linked"));
        assert_eq!(result[0].1, Some("linked".to_string()));
    }

    // ── parse_jj_workspace_list ───────────────────────────────────────────────

    #[test]
    fn parse_jj_workspace_list_excludes_current() {
        let output = "\
default: abc123 (editing: working copy)
my-ws: def456 some commit
";
        let result = parse_jj_workspace_list(output);
        assert_eq!(result, vec!["my-ws"]);
    }

    #[test]
    fn parse_jj_workspace_list_multiple_non_current() {
        let output = "\
default: abc123 (editing: working copy)
ws-a: def456
ws-b: ghi789
";
        let result = parse_jj_workspace_list(output);
        assert_eq!(result, vec!["ws-a", "ws-b"]);
    }

    #[test]
    fn parse_jj_workspace_list_all_current() {
        let output = "default: abc123 (editing: working copy)\n";
        assert!(parse_jj_workspace_list(output).is_empty());
    }

    #[test]
    fn parse_jj_workspace_list_empty_output() {
        assert!(parse_jj_workspace_list("").is_empty());
    }
}

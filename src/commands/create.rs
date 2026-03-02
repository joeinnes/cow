use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

use crate::{
    apfs,
    cli::CreateArgs,
    state::{self, State, WorkspaceEntry},
    vcs::{self, Vcs},
};

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

    // APFS check
    // tarpaulin-ignore-start
    if !apfs::is_apfs(&source) {
        bail!(
            "Source filesystem is not APFS. cow requires APFS for copy-on-write clones.\n\
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
    let workspace_dir = match args.dir {
        Some(d) => d,
        None => state::default_workspace_dir()?,
    };

    // Workspace name
    let name_was_given = args.name.is_some();
    let name = match args.name {
        Some(n) => {
            validate_name(&n)?;
            n
        }
        None => state.next_agent_name(),
    };

    // Default: use workspace name as branch when the caller gave an explicit name
    // and did not pass --branch or --no-branch.
    let branch_arg = if args.branch.is_none() && !args.no_branch && name_was_given {
        Some(name.clone())
    } else {
        args.branch
    };

    // Uniqueness check
    if state.get(&name).is_some() {
        bail!("A workspace named '{}' already exists. Choose a different name.", name);
    }

    let dest = workspace_dir.join(&name);
    if dest.exists() {
        bail!(
            "Destination '{}' already exists on disk. \
             Remove it first or choose a different name.",
            dest.display()
        );
    }

    std::fs::create_dir_all(&workspace_dir)
        .with_context(|| format!("Failed to create workspace directory: {}", workspace_dir.display()))?;

    // CoW clone
    println!("Cloning {} ...", source.display());
    cow_clone(&source, &dest)?;

    // Capture HEAD SHA before any branch switching so extract has a reliable base.
    let initial_commit = if detected_vcs == Vcs::Git {
        vcs::git_head_sha(&dest)
    } else {
        // tarpaulin-ignore-start
        None
        // tarpaulin-ignore-end
    };

    // VCS post-clone setup
    let branch = match detected_vcs {
        Vcs::Git => setup_git(&dest, branch_arg.as_deref())?,
        // tarpaulin-ignore-start
        Vcs::Jj => {
            setup_jj(&dest, args.change.as_deref())?;
            None
        }
        // tarpaulin-ignore-end
    };

    // Cleanup step
    if !args.no_clean {
        cleanup_runtime_artefacts(&dest)?;
        let config_path = source.join(".cow.json");
        if config_path.exists() {
            cleanup_from_config(&dest, &config_path)?;
        }
    }

    // Persist state
    let entry = WorkspaceEntry {
        name: name.clone(),
        path: dest.clone(),
        source,
        vcs: detected_vcs,
        branch,
        initial_commit,
        created_at: chrono::Utc::now(),
    };
    state.add(entry);
    state.save()?;

    println!("Created workspace '{}' at {}", name, dest.display());
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Workspace name cannot be empty.");
    }
    if name.contains('/') || name.contains('\0') {
        bail!("Workspace name '{}' contains invalid characters.", name);
    }
    if name == "." || name == ".." {
        bail!("Workspace name '{}' is not allowed.", name);
    }
    Ok(())
}

fn cow_clone(source: &Path, dest: &Path) -> Result<()> {
    let status = Command::new("cp")
        .args(["-rc", source.to_str().unwrap(), dest.to_str().unwrap()])
        .status()
        .context("Failed to run cp")?;
    if !status.success() {
        bail!(
            "cp -rc failed when cloning '{}' to '{}'.",
            source.display(),
            dest.display()
        );
    }
    Ok(())
}

fn setup_git(workspace: &Path, branch: Option<&str>) -> Result<Option<String>> {
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
fn setup_jj(workspace: &Path, change: Option<&str>) -> Result<()> {
    // The cp -rc clone already has its own .jj/ at a different path, so it is
    // independent from the source without any extra steps.

    if let Some(change_id) = change {
        let status = Command::new("jj")
            .args(["edit", change_id])
            .current_dir(workspace)
            .status()
            .context("Failed to run jj edit")?;
        if !status.success() {
            bail!("Failed to check out change '{}' in workspace.", change_id);
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
    fn validate_name_accepts_valid() {
        assert!(validate_name("my-workspace").is_ok());
        assert!(validate_name("agent-1").is_ok());
        assert!(validate_name("abc").is_ok());
    }

    #[test]
    fn validate_name_rejects_empty() {
        assert!(validate_name("").is_err());
    }

    #[test]
    fn validate_name_rejects_slash() {
        assert!(validate_name("foo/bar").is_err());
    }

    #[test]
    fn validate_name_rejects_null_byte() {
        assert!(validate_name("foo\0bar").is_err());
    }

    #[test]
    fn validate_name_rejects_dot() {
        assert!(validate_name(".").is_err());
        assert!(validate_name("..").is_err());
    }
}

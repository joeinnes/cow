use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Vcs {
    Git,
    Jj,
}

impl std::fmt::Display for Vcs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Vcs::Git => write!(f, "git"),
            Vcs::Jj => write!(f, "jj"),
        }
    }
}

/// Detect which VCS a directory uses. jj is checked first because colocated
/// repos have both `.jj` and `.git`.
pub fn detect_vcs(path: &Path) -> Option<Vcs> {
    if path.join(".jj").is_dir() {
        return Some(Vcs::Jj);
    }
    if path.join(".git").exists() {
        return Some(Vcs::Git);
    }
    None
}

/// Returns true if `.git` is a *file* (pointing at a worktree gitdir), not a directory.
pub fn is_git_worktree(path: &Path) -> bool {
    path.join(".git").is_file()
}

/// Returns true if the path is a secondary jj workspace rather than a primary repository.
/// Secondary workspaces have `.jj/` but no `.jj/repo/` — the repo backend lives in the primary.
pub fn is_jj_secondary_workspace(path: &Path) -> bool {
    path.join(".jj").is_dir() && !path.join(".jj").join("repo").is_dir()
}

/// Return the current branch name, or None if detached / unavailable.
pub fn git_current_branch(path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(path)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

/// Returns true if the git working tree has uncommitted changes.
pub fn git_is_dirty(path: &Path) -> bool {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(path)
        .output();
    match output {
        Ok(o) => !o.stdout.is_empty(),
        Err(_) => false,
    }
}

/// Returns the short `git status` output.
pub fn git_status_short(path: &Path) -> String {
    let output = Command::new("git")
        .args(["status", "--short"])
        .current_dir(path)
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => String::new(),
    }
}

// tarpaulin-ignore-start
/// Returns true if a jj working copy has changes relative to its parent.
pub fn jj_is_dirty(path: &Path) -> bool {
    let output = Command::new("jj")
        .args(["diff", "--summary"])
        .current_dir(path)
        .output();
    match output {
        Ok(o) => !String::from_utf8_lossy(&o.stdout).trim().is_empty(),
        Err(_) => false,
    }
}
// tarpaulin-ignore-end

/// Returns true if the current branch has commits not yet pushed to its upstream.
/// Returns false when no upstream is configured or the command fails.
pub fn git_has_unpushed_commits(path: &Path) -> bool {
    let output = Command::new("git")
        .args(["log", "@{upstream}..HEAD", "--oneline"])
        .current_dir(path)
        .output();
    match output {
        Ok(o) if o.status.success() => !String::from_utf8_lossy(&o.stdout).trim().is_empty(),
        _ => false,
    }
}

/// Returns the full HEAD commit SHA, or None if unavailable.
pub fn git_head_sha(path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(path)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

// tarpaulin-ignore-start
/// Returns modified files from `jj diff --summary`.
pub fn jj_diff_summary(path: &Path) -> String {
    let output = Command::new("jj")
        .args(["diff", "--summary"])
        .current_dir(path)
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => String::new(),
    }
}
// tarpaulin-ignore-end

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        Command::new("git").args(["init", "-b", "main"]).current_dir(p).status().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.test"]).current_dir(p).status().unwrap();
        Command::new("git").args(["config", "user.name", "test"]).current_dir(p).status().unwrap();
        Command::new("git").args(["config", "commit.gpgsign", "false"]).current_dir(p).status().unwrap();
        std::fs::write(p.join("hello.txt"), "hello").unwrap();
        Command::new("git").args(["add", "."]).current_dir(p).status().unwrap();
        Command::new("git").args(["commit", "-m", "initial"]).current_dir(p).status().unwrap();
        dir
    }

    #[test]
    fn display_vcs_git() {
        assert_eq!(Vcs::Git.to_string(), "git");
    }

    #[test]
    fn display_vcs_jj() {
        assert_eq!(Vcs::Jj.to_string(), "jj");
    }

    #[test]
    fn detect_vcs_finds_git() {
        let repo = make_git_repo();
        assert_eq!(detect_vcs(repo.path()), Some(Vcs::Git));
    }

    #[test]
    fn detect_vcs_finds_jj() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".jj")).unwrap();
        assert_eq!(detect_vcs(dir.path()), Some(Vcs::Jj));
    }

    #[test]
    fn detect_vcs_returns_none() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_vcs(dir.path()), None);
    }

    #[test]
    fn is_git_worktree_false_for_dir() {
        let repo = make_git_repo();
        assert!(!is_git_worktree(repo.path()));
    }

    #[test]
    fn is_git_worktree_true_for_file() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".git"), "gitdir: /some/path").unwrap();
        assert!(is_git_worktree(dir.path()));
    }

    #[test]
    fn is_jj_secondary_workspace_false_for_non_jj() {
        let dir = TempDir::new().unwrap();
        assert!(!is_jj_secondary_workspace(dir.path()));
    }

    #[test]
    fn is_jj_secondary_workspace_false_for_primary() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".jj/repo")).unwrap();
        assert!(!is_jj_secondary_workspace(dir.path()));
    }

    #[test]
    fn is_jj_secondary_workspace_true_for_secondary() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".jj")).unwrap();
        // No .jj/repo/ → secondary workspace
        assert!(is_jj_secondary_workspace(dir.path()));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn git_current_branch_returns_main() {
        let repo = make_git_repo();
        assert_eq!(git_current_branch(repo.path()), Some("main".to_string()));
    }

    #[test]
    fn git_current_branch_returns_none_for_non_repo() {
        let dir = TempDir::new().unwrap();
        // git branch --show-current exits non-zero outside a repo → None
        assert_eq!(git_current_branch(dir.path()), None);
    }

    #[test]
    fn git_head_sha_returns_none_for_non_repo() {
        let dir = TempDir::new().unwrap();
        // git rev-parse HEAD exits non-zero outside a repo → None
        assert!(git_head_sha(dir.path()).is_none());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn git_is_dirty_clean_repo() {
        let repo = make_git_repo();
        assert!(!git_is_dirty(repo.path()));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn git_is_dirty_with_changes() {
        let repo = make_git_repo();
        std::fs::write(repo.path().join("untracked.txt"), "new").unwrap();
        assert!(git_is_dirty(repo.path()));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn git_status_short_lists_changed_file() {
        let repo = make_git_repo();
        std::fs::write(repo.path().join("changed.txt"), "changed").unwrap();
        let status = git_status_short(repo.path());
        assert!(status.contains("changed.txt"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn git_head_sha_returns_40_char_sha() {
        let repo = make_git_repo();
        let sha = git_head_sha(repo.path()).expect("should have a HEAD SHA");
        assert_eq!(sha.len(), 40, "SHA should be 40 hex characters");
    }
}

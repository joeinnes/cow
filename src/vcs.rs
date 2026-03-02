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

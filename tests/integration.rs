/// Integration tests for swt.
///
/// These tests require macOS with APFS. The macOS GitHub Actions runner uses
/// APFS by default, so CI needs no extra setup.
///
/// Each test creates an isolated HOME directory so that the state file
/// (~/.swt/state.json) does not leak between tests running in parallel.

#[cfg(target_os = "macos")]
mod tests {
    use assert_cmd::Command;
    use predicates::prelude::*;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    // ─── Helpers ───────────────────────────────────────────────────────────────

    /// An isolated environment for one test: its own HOME so the state file
    /// and default workspace directory are completely separate.
    struct Env {
        /// Kept alive for the duration of the test.
        _home: TempDir,
        pub home: PathBuf,
    }

    impl Env {
        fn new() -> Self {
            let dir = TempDir::new().expect("temp home dir");
            let home = dir.path().to_path_buf();
            Self { _home: dir, home }
        }

        /// Return an `swt` Command with HOME pointed at this environment.
        #[allow(deprecated)]
        fn swt(&self) -> Command {
            let mut cmd = Command::cargo_bin("swt").expect("swt binary");
            cmd.env("HOME", &self.home);
            cmd
        }
    }

    /// Initialise a git repo with one commit.
    fn make_git_repo() -> TempDir {
        let dir = TempDir::new().expect("temp repo");
        let path = dir.path();

        git(path, &["init", "-b", "main"]);
        git(path, &["config", "user.email", "test@swt.test"]);
        git(path, &["config", "user.name", "swt-test"]);
        git(path, &["config", "commit.gpgsign", "false"]);
        git(path, &["config", "tag.gpgsign", "false"]);

        std::fs::write(path.join("hello.txt"), "hello").unwrap();
        git(path, &["add", "."]);
        git(path, &["commit", "-m", "initial"]);

        dir
    }

    fn git(path: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(path)
            .status()
            .unwrap_or_else(|_| panic!("could not run git"));
        assert!(status.success(), "git {:?} failed in {}", args, path.display());
    }

    // ─── create ────────────────────────────────────────────────────────────────

    #[test]
    fn create_from_git_repo() {
        let env = Env::new();
        let source = make_git_repo();

        env.swt()
            .args(["create", "my-workspace", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("Created workspace 'my-workspace'"));

        let workspace = env.home.join(".swt/workspaces/my-workspace");
        assert!(workspace.exists(), "workspace directory should exist");
        assert!(workspace.join(".git").is_dir(), "workspace should be a git repo");
        assert!(workspace.join("hello.txt").exists(), "files should be cloned");
    }

    #[test]
    fn create_auto_names_are_sequential() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path().to_str().unwrap();

        env.swt()
            .args(["create", "--source", src])
            .assert()
            .success()
            .stdout(predicate::str::contains("agent-1"));

        env.swt()
            .args(["create", "--source", src])
            .assert()
            .success()
            .stdout(predicate::str::contains("agent-2"));
    }

    #[test]
    fn create_with_new_branch() {
        let env = Env::new();
        let source = make_git_repo();

        env.swt()
            .args([
                "create", "feat-ws",
                "--source", source.path().to_str().unwrap(),
                "--branch", "feat/new-thing",
            ])
            .assert()
            .success();

        let workspace = env.home.join(".swt/workspaces/feat-ws");
        let out = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "feat/new-thing"
        );
    }

    #[test]
    fn create_with_existing_branch() {
        let env = Env::new();
        let source = make_git_repo();

        git(source.path(), &["checkout", "-b", "existing-branch"]);
        git(source.path(), &["checkout", "main"]);

        env.swt()
            .args([
                "create", "existing-ws",
                "--source", source.path().to_str().unwrap(),
                "--branch", "existing-branch",
            ])
            .assert()
            .success();

        let workspace = env.home.join(".swt/workspaces/existing-ws");
        let out = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "existing-branch"
        );
    }

    #[test]
    fn create_from_git_worktree_fails() {
        let env = Env::new();
        let source = make_git_repo();
        let worktree_dir = TempDir::new().unwrap();
        let worktree_path = worktree_dir.path().join("wt");

        git(source.path(), &[
            "worktree", "add",
            worktree_path.to_str().unwrap(),
        ]);

        env.swt()
            .args(["create", "--source", worktree_path.to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("git worktree"));
    }

    #[test]
    fn create_duplicate_name_fails() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path().to_str().unwrap();

        env.swt().args(["create", "same-name", "--source", src]).assert().success();

        env.swt()
            .args(["create", "same-name", "--source", src])
            .assert()
            .failure()
            .stderr(predicate::str::contains("already exists"));
    }

    // ─── list ──────────────────────────────────────────────────────────────────

    #[test]
    fn list_shows_created_workspaces() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path().to_str().unwrap();

        env.swt().args(["create", "list-ws-1", "--source", src]).assert().success();
        env.swt().args(["create", "list-ws-2", "--source", src]).assert().success();

        let output = env.swt()
            .args(["list", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
        let names: Vec<&str> = json
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| e["name"].as_str())
            .collect();

        assert!(names.contains(&"list-ws-1"), "expected list-ws-1 in {:?}", names);
        assert!(names.contains(&"list-ws-2"), "expected list-ws-2 in {:?}", names);
    }

    // ─── remove ────────────────────────────────────────────────────────────────

    #[test]
    fn remove_clean_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        env.swt()
            .args(["create", "to-remove", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".swt/workspaces/to-remove");
        assert!(workspace.exists());

        env.swt()
            .args(["remove", "--force", "to-remove"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed workspace 'to-remove'"));

        assert!(!workspace.exists(), "workspace directory should be deleted");
    }

    #[test]
    fn remove_with_force_skips_dirty_prompt() {
        let env = Env::new();
        let source = make_git_repo();

        env.swt()
            .args(["create", "dirty-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Make the workspace dirty by staging a new file
        let workspace = env.home.join(".swt/workspaces/dirty-ws");
        std::fs::write(workspace.join("change.txt"), "modified").unwrap();
        git(&workspace, &["add", "change.txt"]);

        env.swt()
            .args(["remove", "--force", "dirty-ws"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed workspace 'dirty-ws'"));

        assert!(!workspace.exists(), "workspace should be deleted");
    }

    #[test]
    fn remove_nonexistent_workspace_prints_warning() {
        let env = Env::new();

        env.swt()
            .args(["remove", "--force", "does-not-exist"])
            .assert()
            .success()
            .stderr(predicate::str::contains("not found"));
    }
}

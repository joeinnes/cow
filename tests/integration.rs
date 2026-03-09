/// Integration tests for cow.
///
/// These tests require macOS with APFS. The macOS GitHub Actions runner uses
/// APFS by default, so CI needs no extra setup.
///
/// Each test creates an isolated HOME directory so that the state file
/// (~/.cow/state.json) does not leak between tests running in parallel.

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

            // Write minimal jj config so `jj` commands work when HOME is overridden.
            let jj_cfg = home.join(".config/jj");
            std::fs::create_dir_all(&jj_cfg).expect("create jj config dir");
            std::fs::write(
                jj_cfg.join("config.toml"),
                "[user]\nemail = \"test@cow.test\"\nname = \"cow-test\"\n",
            ).expect("write jj config");

            Self { _home: dir, home }
        }

        /// Return a `cow` Command with HOME pointed at this environment.
        #[allow(deprecated)]
        fn cow(&self) -> Command {
            let mut cmd = Command::cargo_bin("cow").expect("cow binary");
            cmd.env("HOME", &self.home);
            cmd
        }
    }

    /// Initialise a git repo with one commit.
    fn make_git_repo() -> TempDir {
        let dir = TempDir::new().expect("temp repo");
        let path = dir.path();

        git(path, &["init", "-b", "main"]);
        git(path, &["config", "user.email", "test@cow.test"]);
        git(path, &["config", "user.name", "cow-test"]);
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

        env.cow()
            .args(["create", "my-workspace", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("Created workspace 'my-workspace'"));

        let workspace = env.home.join(".cow/workspaces/my-workspace");
        assert!(workspace.exists(), "workspace directory should exist");
        assert!(workspace.join(".git").is_dir(), "workspace should be a git repo");
        assert!(workspace.join("hello.txt").exists(), "files should be cloned");
    }

    #[test]
    fn create_auto_names_are_sequential() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path().to_str().unwrap();

        env.cow()
            .args(["create", "--source", src])
            .assert()
            .success()
            .stdout(predicate::str::contains("agent-1"));

        env.cow()
            .args(["create", "--source", src])
            .assert()
            .success()
            .stdout(predicate::str::contains("agent-2"));
    }

    #[test]
    fn create_with_new_branch() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args([
                "create", "feat-ws",
                "--source", source.path().to_str().unwrap(),
                "--branch", "feat/new-thing",
            ])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/feat-ws");
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

        env.cow()
            .args([
                "create", "existing-ws",
                "--source", source.path().to_str().unwrap(),
                "--branch", "existing-branch",
            ])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/existing-ws");
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

    // ─── name-as-branch default (mai-uiz0) ────────────────────────────────────

    fn workspace_branch(home: &std::path::Path, name: &str) -> String {
        let ws = home.join(format!(".cow/workspaces/{name}"));
        let out = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&ws)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn create_name_used_as_branch_by_default() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "my-feature", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        assert_eq!(workspace_branch(&env.home, "my-feature"), "my-feature");
    }

    #[test]
    fn create_no_branch_flag_stays_on_source_branch() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "my-feature", "--source", source.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        assert_eq!(workspace_branch(&env.home, "my-feature"), "main");
    }

    #[test]
    fn create_auto_name_stays_on_source_branch() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Auto-named workspace (agent-1) should stay on source branch.
        assert_eq!(workspace_branch(&env.home, "agent-1"), "main");
    }

    #[test]
    fn create_branch_flag_overrides_name() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "my-ws", "--source", source.path().to_str().unwrap(), "--branch", "other-branch"])
            .assert()
            .success();

        assert_eq!(workspace_branch(&env.home, "my-ws"), "other-branch");
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

        env.cow()
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

        env.cow().args(["create", "same-name", "--source", src]).assert().success();

        env.cow()
            .args(["create", "same-name", "--source", src])
            .assert()
            .failure()
            .stderr(predicate::str::contains("already exists"));
    }

    #[test]
    fn create_with_custom_dir() {
        let env = Env::new();
        let source = make_git_repo();
        let custom_dir = TempDir::new().unwrap();

        env.cow()
            .args([
                "create", "dir-ws",
                "--source", source.path().to_str().unwrap(),
                "--dir", custom_dir.path().to_str().unwrap(),
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("dir-ws"));

        assert!(custom_dir.path().join("dir-ws").exists());
    }

    #[test]
    fn create_dest_exists_fails() {
        let env = Env::new();
        let source = make_git_repo();

        // Pre-create the destination to trigger the "already exists on disk" error
        let dest = env.home.join(".cow/workspaces/pre-existing");
        std::fs::create_dir_all(&dest).unwrap();

        env.cow()
            .args(["create", "pre-existing", "--source", source.path().to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("already exists on disk"));
    }

    #[test]
    fn create_invalid_name_empty() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "", "--source", source.path().to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("cannot be empty"));
    }

    #[test]
    fn create_invalid_name_slash() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "foo/bar", "--source", source.path().to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid characters"));
    }

    #[test]
    fn create_invalid_name_dot() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", ".", "--source", source.path().to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not allowed"));
    }

    #[test]
    fn create_warns_about_submodules() {
        let env = Env::new();
        let source = make_git_repo();
        std::fs::write(source.path().join(".gitmodules"), "[submodule \"lib\"]\n\tpath = lib\n").unwrap();

        env.cow()
            .args(["create", "submod-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stderr(predicate::str::contains("submodule"));
    }

    #[test]
    fn create_cleans_pid_files_by_default() {
        let env = Env::new();
        let source = make_git_repo();
        std::fs::write(source.path().join("server.pid"), "12345").unwrap();

        env.cow()
            .args(["create", "clean-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // pid file should be removed by the default cleanup
        assert!(!env.home.join(".cow/workspaces/clean-ws/server.pid").exists());
    }

    #[test]
    fn create_no_clean_preserves_pid_files() {
        let env = Env::new();
        let source = make_git_repo();
        std::fs::write(source.path().join("server.pid"), "12345").unwrap();

        env.cow()
            .args(["create", "noclean-ws", "--source", source.path().to_str().unwrap(), "--no-clean"])
            .assert()
            .success();

        // pid file should be kept because --no-clean was passed
        assert!(env.home.join(".cow/workspaces/noclean-ws/server.pid").exists());
    }

    #[test]
    fn create_cow_json_removes_file() {
        let env = Env::new();
        let source = make_git_repo();

        std::fs::write(source.path().join("to_delete.txt"), "delete me").unwrap();
        std::fs::write(
            source.path().join(".cow.json"),
            r#"{"post_clone":{"remove":["to_delete.txt"]}}"#,
        ).unwrap();

        env.cow()
            .args(["create", "config-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        assert!(!env.home.join(".cow/workspaces/config-ws/to_delete.txt").exists());
    }

    #[test]
    fn create_cow_json_removes_directory() {
        let env = Env::new();
        let source = make_git_repo();

        std::fs::create_dir(source.path().join("to_delete_dir")).unwrap();
        std::fs::write(source.path().join("to_delete_dir/file.txt"), "content").unwrap();
        std::fs::write(
            source.path().join(".cow.json"),
            r#"{"post_clone":{"remove":["to_delete_dir"]}}"#,
        ).unwrap();

        env.cow()
            .args(["create", "dir-config-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        assert!(!env.home.join(".cow/workspaces/dir-config-ws/to_delete_dir").exists());
    }

    #[test]
    fn create_cow_json_runs_commands() {
        let env = Env::new();
        let source = make_git_repo();

        std::fs::write(
            source.path().join(".cow.json"),
            r#"{"post_clone":{"run":["touch post_clone_ran.txt"]}}"#,
        ).unwrap();

        env.cow()
            .args(["create", "run-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("Running post-clone"));

        assert!(env.home.join(".cow/workspaces/run-ws/post_clone_ran.txt").exists());
    }

    // ─── .cow-context ──────────────────────────────────────────────────────────

    #[test]
    fn create_writes_cow_context_file() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "ctx-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let ctx_path = env.home.join(".cow/workspaces/ctx-ws/.cow-context");
        assert!(ctx_path.exists(), ".cow-context should be written into workspace root");

        let content = std::fs::read_to_string(&ctx_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).expect("should be valid JSON");

        assert_eq!(v["name"], "ctx-ws");
        assert_eq!(v["vcs"], "git");
        assert!(v["source"].as_str().is_some());
        assert!(v["branch"].as_str().is_some());
        assert!(v["initial_commit"].as_str().is_some());
        assert!(v["created_at"].as_str().is_some());
    }

    #[test]
    fn cow_context_excluded_from_git_status() {
        // .cow-context should not show up as an untracked file in git status.
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "ctx-clean-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/ctx-clean-ws");
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert!(output.stdout.is_empty(), ".cow-context should not make workspace dirty");
    }

    // ─── list ──────────────────────────────────────────────────────────────────

    #[test]
    fn list_shows_created_workspaces() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path().to_str().unwrap();

        env.cow().args(["create", "list-ws-1", "--source", src]).assert().success();
        env.cow().args(["create", "list-ws-2", "--source", src]).assert().success();

        let output = env.cow()
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

    #[test]
    fn list_text_table_output() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "table-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("NAME"))
            .stdout(predicate::str::contains("SOURCE"))
            .stdout(predicate::str::contains("table-ws"));
    }

    #[test]
    fn list_empty_state() {
        let env = Env::new();

        env.cow()
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("No workspaces found."));
    }

    #[test]
    fn list_source_filter() {
        let env = Env::new();
        let source1 = make_git_repo();
        let source2 = make_git_repo();

        env.cow()
            .args(["create", "from-s1", "--source", source1.path().to_str().unwrap()])
            .assert()
            .success();
        env.cow()
            .args(["create", "from-s2", "--source", source2.path().to_str().unwrap()])
            .assert()
            .success();

        let output = env.cow()
            .args(["list", "--json", "--source", source1.path().to_str().unwrap()])
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

        assert!(names.contains(&"from-s1"));
        assert!(!names.contains(&"from-s2"), "source filter should exclude from-s2");
    }

    // ─── remove ────────────────────────────────────────────────────────────────

    #[test]
    fn remove_clean_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "to-remove", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/to-remove");
        assert!(workspace.exists());

        env.cow()
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

        env.cow()
            .args(["create", "dirty-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Make the workspace dirty by staging a new file
        let workspace = env.home.join(".cow/workspaces/dirty-ws");
        std::fs::write(workspace.join("change.txt"), "modified").unwrap();
        git(&workspace, &["add", "change.txt"]);

        env.cow()
            .args(["remove", "--force", "dirty-ws"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed workspace 'dirty-ws'"));

        assert!(!workspace.exists(), "workspace should be deleted");
    }

    #[test]
    fn remove_nonexistent_workspace_prints_warning() {
        let env = Env::new();

        env.cow()
            .args(["remove", "--force", "does-not-exist"])
            .assert()
            .success()
            .stderr(predicate::str::contains("not found"));
    }

    #[test]
    fn remove_no_args_fails() {
        let env = Env::new();

        env.cow()
            .arg("remove")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Specify one or more workspace names"));
    }

    #[test]
    fn remove_all() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path().to_str().unwrap();

        env.cow().args(["create", "ws-a", "--source", src]).assert().success();
        env.cow().args(["create", "ws-b", "--source", src]).assert().success();

        env.cow()
            .args(["remove", "--all", "--force"])
            .assert()
            .success();

        assert!(!env.home.join(".cow/workspaces/ws-a").exists());
        assert!(!env.home.join(".cow/workspaces/ws-b").exists());
    }

    #[test]
    fn remove_all_source_filter() {
        let env = Env::new();
        let source1 = make_git_repo();
        let source2 = make_git_repo();

        env.cow()
            .args(["create", "from-s1", "--source", source1.path().to_str().unwrap()])
            .assert()
            .success();
        env.cow()
            .args(["create", "from-s2", "--source", source2.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["remove", "--all", "--force", "--source", source1.path().to_str().unwrap()])
            .assert()
            .success();

        assert!(!env.home.join(".cow/workspaces/from-s1").exists(), "from-s1 should be removed");
        assert!(env.home.join(".cow/workspaces/from-s2").exists(), "from-s2 should remain");
    }

    #[test]
    fn remove_all_no_match_prints_message() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["remove", "--all", "--force", "--source", "/nonexistent/path"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No workspaces to remove."));
    }

    #[test]
    fn remove_dirty_without_force_defaults_to_no() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "dirty-keep", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/dirty-keep");
        std::fs::write(workspace.join("change.txt"), "modified").unwrap();
        git(&workspace, &["add", "change.txt"]);

        // Without --force, non-TTY stdin defaults to "no" → workspace preserved
        env.cow()
            .args(["remove", "dirty-keep"])
            .assert()
            .success()
            .stderr(predicate::str::contains("Not a TTY"));

        assert!(workspace.exists(), "workspace should still exist");
    }

    #[test]
    fn list_json_includes_dirty_and_current_branch() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "list-json-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Make the workspace dirty.
        std::fs::write(env.home.join(".cow/workspaces/list-json-ws/wip.txt"), "wip").unwrap();

        let raw = env.cow()
            .args(["list", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let arr: serde_json::Value = serde_json::from_slice(&raw).expect("valid JSON");
        let ws = arr.as_array().unwrap()
            .iter()
            .find(|w| w["name"] == "list-json-ws")
            .expect("workspace should appear in list");

        assert_eq!(ws["dirty"], true, "dirty flag should be true");
        assert!(ws["current_branch"].as_str().is_some(), "current_branch should be present");
    }

    #[test]
    fn list_json_clean_workspace_not_dirty() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "list-clean-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let raw = env.cow()
            .args(["list", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let arr: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        let ws = arr.as_array().unwrap()
            .iter()
            .find(|w| w["name"] == "list-clean-ws")
            .unwrap();

        assert_eq!(ws["dirty"], false);
    }

    // ─── status ────────────────────────────────────────────────────────────────

    #[test]
    fn status_clean_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "status-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["status", "status-ws"])
            .assert()
            .success()
            .stdout(predicate::str::contains("status-ws"))
            .stdout(predicate::str::contains("clean"))
            .stdout(predicate::str::contains("Disk delta"));
    }

    #[test]
    fn status_dirty_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "dirty-status", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/dirty-status");
        std::fs::write(workspace.join("modified.txt"), "changed content").unwrap();
        git(&workspace, &["add", "modified.txt"]);

        env.cow()
            .args(["status", "dirty-status"])
            .assert()
            .success()
            .stdout(predicate::str::contains("dirty"))
            .stdout(predicate::str::contains("Modified files"))
            .stdout(predicate::str::contains("modified.txt"));
    }

    #[test]
    fn status_cwd_detection() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "cwd-status", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/cwd-status");

        // Run status with no name but from inside the workspace
        env.cow()
            .arg("status")
            .current_dir(&workspace)
            .assert()
            .success()
            .stdout(predicate::str::contains("cwd-status"));
    }

    #[test]
    fn status_not_found() {
        let env = Env::new();

        env.cow()
            .args(["status", "nonexistent"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    #[test]
    fn status_json_clean_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "json-status-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let raw = env.cow()
            .args(["status", "json-status-ws", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let v: serde_json::Value = serde_json::from_slice(&raw).expect("should be valid JSON");
        assert_eq!(v["name"], "json-status-ws");
        assert_eq!(v["vcs"], "git");
        assert_eq!(v["dirty"], false);
        assert!(v["branch"].as_str().is_some());
        assert!(v["path"].as_str().is_some());
        assert!(v["source"].as_str().is_some());
        assert!(v["created_at"].as_str().is_some());
    }

    #[test]
    fn status_json_dirty_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "json-dirty-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/json-dirty-ws");
        std::fs::write(workspace.join("wip.txt"), "work in progress").unwrap();

        let raw = env.cow()
            .args(["status", "json-dirty-ws", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let v: serde_json::Value = serde_json::from_slice(&raw).expect("should be valid JSON");
        assert_eq!(v["dirty"], true);
        let files = v["modified_files"].as_array().expect("modified_files should be array");
        assert!(files.iter().any(|f| f.as_str().unwrap_or("").contains("wip.txt")));
    }

    // ─── diff ──────────────────────────────────────────────────────────────────

    #[test]
    fn diff_clean_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "diff-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["diff", "diff-ws"])
            .assert()
            .success();
    }

    #[test]
    fn diff_cwd_detection() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "diff-cwd", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/diff-cwd");

        env.cow()
            .arg("diff")
            .current_dir(&workspace)
            .assert()
            .success();
    }

    #[test]
    fn diff_not_found() {
        let env = Env::new();

        env.cow()
            .args(["diff", "nonexistent"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    // ─── extract ───────────────────────────────────────────────────────────────

    #[test]
    fn extract_no_flags_fails() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "extract-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["extract", "extract-ws"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("--patch"));
    }

    #[test]
    fn extract_not_found() {
        let env = Env::new();
        let patch_file = env.home.join("out.patch");

        env.cow()
            .args(["extract", "nonexistent", "--patch", patch_file.to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    #[test]
    fn extract_patch_creates_file() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "patch-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/patch-ws");

        // Make a commit in the workspace so there's something to patch
        std::fs::write(workspace.join("new_feature.txt"), "feature content").unwrap();
        git(&workspace, &["add", "new_feature.txt"]);
        git(&workspace, &["commit", "-m", "add new feature"]);

        let patch_file = env.home.join("changes.patch");

        env.cow()
            .args(["extract", "patch-ws", "--patch", patch_file.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("Patch written to"));

        assert!(patch_file.exists(), "patch file should exist");
        let content = std::fs::read_to_string(&patch_file).unwrap();
        assert!(content.contains("new_feature.txt"), "patch should reference the changed file");
    }

    #[test]
    fn extract_branch_creates_local_branch() {
        // --branch should create the named branch in the SOURCE repo, not push to origin.
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "branch-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/branch-ws");

        // Commit something in the workspace so branch-ws diverges from source.
        std::fs::write(workspace.join("feature.txt"), "feature").unwrap();
        git(&workspace, &["add", "feature.txt"]);
        git(&workspace, &["commit", "-m", "add feature"]);

        env.cow()
            .args(["extract", "branch-ws", "--branch", "feat/extracted"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Branch 'feat/extracted' created in source repo"));

        // Verify the branch now exists in the source repo.
        let branch_exists = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "feat/extracted"])
            .current_dir(source.path())
            .status()
            .unwrap()
            .success();
        assert!(branch_exists, "feat/extracted should exist in source repo");
    }

    // ─── sync ──────────────────────────────────────────────────────────────────

    #[test]
    fn sync_rebases_workspace_onto_source_branch() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "sync-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/sync-ws");

        // Add a commit to source's main after workspace was created.
        std::fs::write(source.path().join("synced.txt"), "from source").unwrap();
        git(source.path(), &["add", "synced.txt"]);
        git(source.path(), &["commit", "-m", "source update"]);

        env.cow()
            .args(["sync", "main", "--name", "sync-ws"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Synced 'sync-ws'"));

        // The file from source should now be in the workspace.
        assert!(workspace.join("synced.txt").exists(), "synced.txt should exist after sync");
    }

    #[test]
    fn sync_merges_with_flag() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "merge-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/merge-ws");

        std::fs::write(source.path().join("merged.txt"), "from source").unwrap();
        git(source.path(), &["add", "merged.txt"]);
        git(source.path(), &["commit", "-m", "source merge update"]);

        env.cow()
            .args(["sync", "main", "--name", "merge-ws", "--merge"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Synced 'merge-ws'"));

        assert!(workspace.join("merged.txt").exists());
    }

    #[test]
    fn sync_refuses_when_dirty() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "dirty-sync-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/dirty-sync-ws");

        // Leave an uncommitted file in the workspace.
        std::fs::write(workspace.join("uncommitted.txt"), "wip").unwrap();

        env.cow()
            .args(["sync", "main", "--name", "dirty-sync-ws"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("uncommitted changes"));
    }

    #[test]
    fn sync_workspace_not_found() {
        let env = Env::new();

        env.cow()
            .args(["sync", "main", "--name", "no-such-ws"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    #[test]
    fn sync_jj_workspace_rebases() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-sync-rebase", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/jj-sync-rebase");

        // Advance source: add a commit and pin a bookmark to it.
        std::fs::write(source.path().join("new.txt"), "source update").unwrap();
        jj_run(&env.home, source.path(), &["describe", "-m", "source update"]);
        jj_run(&env.home, source.path(), &["bookmark", "set", "main"]);
        jj_run(&env.home, source.path(), &["new"]);

        env.cow()
            .args(["sync", "main", "--name", "jj-sync-rebase"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Synced"));

        // After rebase the source's "source update" commit is the workspace parent,
        // so new.txt is visible in the working directory.
        assert!(workspace.join("new.txt").exists());
    }

    #[test]
    fn sync_jj_refuses_when_dirty() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-sync-dirty", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Write a file without describing — workspace is now dirty.
        std::fs::write(
            env.home.join(".cow/workspaces/jj-sync-dirty/dirty.txt"),
            "uncommitted",
        )
        .unwrap();

        env.cow()
            .args(["sync", "main", "--name", "jj-sync-dirty"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("uncommitted changes"));
    }

    #[test]
    fn sync_jj_requires_source_branch() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-sync-nobranch", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // No source_branch arg — should bail with a helpful message.
        env.cow()
            .args(["sync", "--name", "jj-sync-nobranch"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("source branch explicitly"));
    }

    #[test]
    fn sync_default_branch_uses_workspace_branch() {
        // No source_branch arg → syncs with workspace's current branch (main).
        let env = Env::new();
        let source = make_git_repo();

        // --no-branch keeps workspace on main so workspace branch == source branch.
        env.cow()
            .args(["create", "nobranch-ws", "--source", source.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/nobranch-ws");

        // Advance source's main.
        std::fs::write(source.path().join("default_sync.txt"), "default").unwrap();
        git(source.path(), &["add", "default_sync.txt"]);
        git(source.path(), &["commit", "-m", "advance main"]);

        // Sync without specifying a branch — should default to workspace's current branch (main).
        env.cow()
            .args(["sync", "--name", "nobranch-ws"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Synced 'nobranch-ws'"));

        assert!(workspace.join("default_sync.txt").exists());
    }

    #[test]
    fn sync_cwd_detection() {
        // Without --name, cow sync should detect workspace from current directory.
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "cwd-sync-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/cwd-sync-ws");

        std::fs::write(source.path().join("cwd_synced.txt"), "cwd").unwrap();
        git(source.path(), &["add", "cwd_synced.txt"]);
        git(source.path(), &["commit", "-m", "advance main"]);

        // Run sync from inside the workspace with no --name.
        env.cow()
            .args(["sync", "main"])
            .current_dir(&workspace)
            .assert()
            .success()
            .stdout(predicate::str::contains("Synced 'cwd-sync-ws'"));

        assert!(workspace.join("cwd_synced.txt").exists());
    }

    #[test]
    fn sync_fetch_failure_bails() {
        // Stub git to fail on fetch; should bail with helpful message and clean up remote.
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "fetch-err-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let stub_dir = TempDir::new().expect("stub dir");
        {
            use std::os::unix::fs::PermissionsExt;
            let real_git = std::process::Command::new("which")
                .arg("git")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "/usr/bin/git".to_string());
            let script = format!(
                "#!/bin/sh\nif [ \"$1\" = \"fetch\" ]; then exit 1; fi\nexec {real_git} \"$@\"\n"
            );
            let stub_path = stub_dir.path().join("git");
            std::fs::write(&stub_path, &script).unwrap();
            std::fs::set_permissions(&stub_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        env.cow()
            .args(["sync", "main", "--name", "fetch-err-ws"])
            .env("PATH", prepend_path(stub_dir.path()))
            .assert()
            .failure()
            .stderr(predicate::str::contains("Failed to fetch branch"));
    }

    #[test]
    fn sync_rebase_failure_bails() {
        // Stub git to fail on rebase; should bail with helpful message.
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "rebase-err-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Advance source so there's something to fetch.
        std::fs::write(source.path().join("advance.txt"), "advance").unwrap();
        git(source.path(), &["add", "advance.txt"]);
        git(source.path(), &["commit", "-m", "advance"]);

        let stub_dir = TempDir::new().expect("stub dir");
        {
            use std::os::unix::fs::PermissionsExt;
            let real_git = std::process::Command::new("which")
                .arg("git")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "/usr/bin/git".to_string());
            let script = format!(
                "#!/bin/sh\nif [ \"$1\" = \"rebase\" ]; then exit 1; fi\nexec {real_git} \"$@\"\n"
            );
            let stub_path = stub_dir.path().join("git");
            std::fs::write(&stub_path, &script).unwrap();
            std::fs::set_permissions(&stub_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        env.cow()
            .args(["sync", "main", "--name", "rebase-err-ws"])
            .env("PATH", prepend_path(stub_dir.path()))
            .assert()
            .failure()
            .stderr(predicate::str::contains("Failed to rebase workspace"));
    }

    #[test]
    fn sync_conflict_aborts_and_reports() {
        // Real rebase conflict: workspace and source both modify the same line.
        // cow sync should auto-abort, leave workspace clean, and report the conflict.
        let env = Env::new();
        let source = make_git_repo(); // has hello.txt with content "hello"

        env.cow()
            .args(["create", "conflict-ws", "--source", source.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/conflict-ws");

        // Workspace modifies hello.txt and commits.
        std::fs::write(workspace.join("hello.txt"), "workspace version").unwrap();
        git(&workspace, &["add", "hello.txt"]);
        git(&workspace, &["commit", "-m", "workspace change"]);

        // Source also modifies hello.txt and commits (different content → conflict on rebase).
        std::fs::write(source.path().join("hello.txt"), "source version").unwrap();
        git(source.path(), &["add", "hello.txt"]);
        git(source.path(), &["commit", "-m", "source change"]);

        env.cow()
            .args(["sync", "main", "--name", "conflict-ws"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("conflict"));

        // Workspace must NOT be left in a rebase state.
        let rebase_dir = workspace.join(".git").join("rebase-merge");
        assert!(!rebase_dir.exists(), "rebase-merge dir should be absent after auto-abort");
    }

    // ─── cd ────────────────────────────────────────────────────────────────────

    #[test]
    fn cd_prints_workspace_path() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "cd-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let expected = env.home.join(".cow/workspaces/cd-ws");

        let raw = env.cow()
            .args(["cd", "cd-ws"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let printed = String::from_utf8_lossy(&raw).trim().to_string();
        // Canonicalise both sides to handle /var vs /private/var on macOS.
        let printed_canon = std::fs::canonicalize(&printed).unwrap_or_else(|_| printed.clone().into());
        let expected_canon = std::fs::canonicalize(&expected).unwrap_or(expected);
        assert_eq!(printed_canon, expected_canon);
    }

    #[test]
    fn cd_not_found() {
        let env = Env::new();

        env.cow()
            .args(["cd", "no-such-workspace"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    // ─── mcp ───────────────────────────────────────────────────────────────────

    #[test]
    fn mcp_initialize() {
        let env = Env::new();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let input = format!("{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let text = String::from_utf8_lossy(&raw);
        let resp: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(resp["result"]["serverInfo"]["name"], "cow");
        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    }

    #[test]
    fn mcp_tools_list() {
        let env = Env::new();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        let input = format!("{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let text = String::from_utf8_lossy(&raw);
        let resp: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 6);

        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"cow_create"));
        assert!(names.contains(&"cow_list"));
        assert!(names.contains(&"cow_remove"));
        assert!(names.contains(&"cow_status"));
        assert!(names.contains(&"cow_sync"));
        assert!(names.contains(&"cow_extract"));
    }

    #[test]
    fn mcp_unknown_method() {
        let env = Env::new();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "unknown/method",
            "params": {}
        });
        let input = format!("{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let text = String::from_utf8_lossy(&raw);
        let resp: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn mcp_notification_produces_no_output() {
        let env = Env::new();

        // A notification has no "id" field — the server must not respond
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        let input = format!("{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        assert!(raw.is_empty(), "server must not respond to notifications");
    }

    #[test]
    fn mcp_invalid_json_is_ignored() {
        let env = Env::new();

        // Invalid JSON line followed by a valid request
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let input = format!("not valid json\n{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        // Should still get the initialize response (only one line, not two)
        let text = String::from_utf8_lossy(&raw);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1, "should have exactly one response line");
        let resp: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(resp["result"]["serverInfo"]["name"], "cow");
    }

    #[test]
    fn mcp_call_cow_create() {
        let env = Env::new();
        let source = make_git_repo();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_create",
                "arguments": {
                    "name": "mcp-created",
                    "source": source.path().to_str().unwrap()
                }
            }
        });
        let input = format!("{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let text = String::from_utf8_lossy(&raw);
        let resp: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Created workspace"));
    }

    #[test]
    fn mcp_call_cow_list() {
        let env = Env::new();
        let source = make_git_repo();

        // Create a workspace via the CLI first
        env.cow()
            .args(["create", "listed-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_list",
                "arguments": {}
            }
        });
        let input = format!("{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let text = String::from_utf8_lossy(&raw);
        let resp: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("listed-ws"));
    }

    #[test]
    fn mcp_call_cow_remove() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "to-mcp-remove", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/to-mcp-remove");
        assert!(workspace.exists());

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_remove",
                "arguments": {
                    "names": ["to-mcp-remove"]
                }
            }
        });
        let input = format!("{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let text = String::from_utf8_lossy(&raw);
        let resp: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(!workspace.exists(), "workspace should have been removed");
    }

    #[test]
    fn mcp_call_cow_status() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "mcp-status-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_status",
                "arguments": {
                    "name": "mcp-status-ws"
                }
            }
        });
        let input = format!("{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let text = String::from_utf8_lossy(&raw);
        let resp: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("mcp-status-ws"));
    }

    #[test]
    fn create_from_cwd_source() {
        let env = Env::new();
        let source = make_git_repo();

        // Run create without --source: should use CWD as the source
        env.cow()
            .args(["create", "cwd-src-ws"])
            .current_dir(source.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("Created workspace 'cwd-src-ws'"));
    }

    #[test]
    fn create_cow_json_no_post_clone_section() {
        let env = Env::new();
        let source = make_git_repo();

        // A .cow.json with no post_clone key should succeed silently
        std::fs::write(source.path().join(".cow.json"), r#"{}"#).unwrap();

        env.cow()
            .args(["create", "empty-config-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();
    }

    #[test]
    fn create_cow_json_failing_command_exits_error() {
        let env = Env::new();
        let source = make_git_repo();

        std::fs::write(
            source.path().join(".cow.json"),
            r#"{"post_clone":{"run":["false"]}}"#,
        ).unwrap();

        env.cow()
            .args(["create", "fail-cmd-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Post-clone command failed"));
    }

    #[test]
    fn mcp_empty_line_is_ignored() {
        let env = Env::new();

        // Empty lines should be silently skipped; only the valid request gets a response
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let input = format!("\n\n{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let text = String::from_utf8_lossy(&raw);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1, "should produce exactly one response");
    }

    #[test]
    fn mcp_call_create_with_branch() {
        let env = Env::new();
        let source = make_git_repo();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_create",
                "arguments": {
                    "name": "branch-ws",
                    "source": source.path().to_str().unwrap(),
                    "branch": "feat/mcp-branch"
                }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(
            String::from_utf8_lossy(&raw).trim()
        ).unwrap();
        assert_eq!(resp["result"]["isError"], false);

        let workspace = env.home.join(".cow/workspaces/branch-ws");
        let out = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "feat/mcp-branch");
    }

    #[test]
    fn mcp_call_create_with_dir() {
        let env = Env::new();
        let source = make_git_repo();
        let custom_dir = TempDir::new().unwrap();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_create",
                "arguments": {
                    "name": "dir-mcp-ws",
                    "source": source.path().to_str().unwrap(),
                    "dir": custom_dir.path().to_str().unwrap()
                }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(
            String::from_utf8_lossy(&raw).trim()
        ).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(custom_dir.path().join("dir-mcp-ws").exists());
    }

    #[test]
    fn mcp_call_list_with_source_filter() {
        let env = Env::new();
        let source1 = make_git_repo();
        let source2 = make_git_repo();

        env.cow()
            .args(["create", "s1-ws", "--source", source1.path().to_str().unwrap()])
            .assert()
            .success();
        env.cow()
            .args(["create", "s2-ws", "--source", source2.path().to_str().unwrap()])
            .assert()
            .success();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_list",
                "arguments": {
                    "source": source1.path().to_str().unwrap()
                }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(
            String::from_utf8_lossy(&raw).trim()
        ).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("s1-ws"), "should include s1-ws");
        assert!(!text.contains("s2-ws"), "should exclude s2-ws");
    }

    #[test]
    fn mcp_call_remove_all() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path().to_str().unwrap();

        env.cow().args(["create", "all-a", "--source", src]).assert().success();
        env.cow().args(["create", "all-b", "--source", src]).assert().success();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_remove",
                "arguments": { "all": true }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(
            String::from_utf8_lossy(&raw).trim()
        ).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(!env.home.join(".cow/workspaces/all-a").exists());
        assert!(!env.home.join(".cow/workspaces/all-b").exists());
    }

    #[test]
    fn mcp_call_remove_with_source() {
        let env = Env::new();
        let source1 = make_git_repo();
        let source2 = make_git_repo();

        env.cow()
            .args(["create", "keep-ws", "--source", source2.path().to_str().unwrap()])
            .assert()
            .success();
        env.cow()
            .args(["create", "del-ws", "--source", source1.path().to_str().unwrap()])
            .assert()
            .success();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_remove",
                "arguments": {
                    "all": true,
                    "source": source1.path().to_str().unwrap()
                }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(
            String::from_utf8_lossy(&raw).trim()
        ).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(!env.home.join(".cow/workspaces/del-ws").exists());
        assert!(env.home.join(".cow/workspaces/keep-ws").exists());
    }

    #[test]
    fn mcp_call_result_includes_stderr() {
        let env = Env::new();
        let source = make_git_repo();

        // Add .gitmodules so cow create produces a stderr submodule warning
        std::fs::write(
            source.path().join(".gitmodules"),
            "[submodule \"lib\"]\n\tpath = lib\n",
        ).unwrap();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_create",
                "arguments": {
                    "name": "stderr-ws",
                    "source": source.path().to_str().unwrap()
                }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(
            String::from_utf8_lossy(&raw).trim()
        ).unwrap();
        // Both stdout (Created...) and stderr (submodule warning) should be merged
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Created workspace"), "should have stdout");
        assert!(text.contains("submodule"), "should have merged stderr");
    }

    // ─── remove: push offer (mai-lwfo) ─────────────────────────────────────────

    /// Create a bare repo clone of `source` and wire it as the "origin" remote
    /// of the given workspace directory.
    fn add_origin_remote(workspace: &std::path::Path, bare_dir: &std::path::Path) {
        // init bare repo
        git(bare_dir, &["init", "--bare", "-b", "main"]);
        // push initial commit from source into bare repo so origin/main exists
        git(workspace, &["remote", "add", "origin", bare_dir.to_str().unwrap()]);
        git(workspace, &["push", "-u", "origin", "HEAD"]);
    }

    #[test]
    fn remove_warns_about_unpushed_commits_with_force() {
        // --force should NOT block removal, but should warn on stderr.
        let env = Env::new();
        let source = make_git_repo();
        let bare = TempDir::new().unwrap();

        env.cow()
            .args(["create", "push-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let ws = env.home.join(".cow/workspaces/push-ws");
        add_origin_remote(&ws, bare.path());

        // Make a commit in the workspace that is NOT on origin.
        std::fs::write(ws.join("new.txt"), "new").unwrap();
        git(&ws, &["add", "."]);
        git(&ws, &["commit", "-m", "workspace commit"]);

        // --force should remove without prompting, but warn about unpushed commits.
        env.cow()
            .args(["remove", "--force", "push-ws"])
            .assert()
            .success()
            .stderr(predicate::str::contains("unpushed"))
            .stdout(predicate::str::contains("Removed workspace"));
    }

    #[test]
    fn remove_non_tty_warns_about_unpushed_and_removes() {
        // Non-TTY (no --force, no interactive prompt): warn but still remove.
        let env = Env::new();
        let source = make_git_repo();
        let bare = TempDir::new().unwrap();

        env.cow()
            .args(["create", "push-warn-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let ws = env.home.join(".cow/workspaces/push-warn-ws");
        add_origin_remote(&ws, bare.path());

        std::fs::write(ws.join("new.txt"), "new").unwrap();
        git(&ws, &["add", "."]);
        git(&ws, &["commit", "-m", "workspace commit"]);

        // Non-TTY stdin: should warn on stderr and proceed with removal.
        env.cow()
            .args(["remove", "push-warn-ws"])
            .assert()
            .success()
            .stderr(predicate::str::contains("unpushed"))
            .stdout(predicate::str::contains("Removed workspace"));
    }

    #[test]
    fn remove_no_unpushed_commits_skips_push_logic() {
        // When workspace is up to date with origin, no warning should appear.
        let env = Env::new();
        let source = make_git_repo();
        let bare = TempDir::new().unwrap();

        env.cow()
            .args(["create", "synced-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let ws = env.home.join(".cow/workspaces/synced-ws");
        add_origin_remote(&ws, bare.path());

        // Nothing committed after push → zero unpushed commits.
        env.cow()
            .args(["remove", "--force", "synced-ws"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed workspace"));
        // stderr should NOT contain "unpushed"
    }

    // ─── jj helpers ────────────────────────────────────────────────────────────

    /// Initialise a colocated jj+git repo with one committed change, leaving
    /// the working copy clean (so `jj diff --summary` returns nothing).
    fn make_jj_repo(home: &Path) -> TempDir {
        let dir = TempDir::new().expect("temp jj repo");
        let path = dir.path();
        jj_run(home, path, &["git", "init", "--colocate"]);
        std::fs::write(path.join("hello.txt"), "hello").unwrap();
        jj_run(home, path, &["describe", "-m", "initial"]);
        // Create a new empty change on top so the working copy is clean.
        jj_run(home, path, &["new"]);
        dir
    }

    fn jj_run(home: &Path, path: &Path, args: &[&str]) {
        let status = std::process::Command::new("jj")
            .args(args)
            .current_dir(path)
            .env("HOME", home)
            .status()
            .unwrap_or_else(|_| panic!("could not run jj"));
        assert!(status.success(), "jj {:?} failed in {}", args, path.display());
    }

    // ─── command-failure stub helpers ──────────────────────────────────────────

    /// Write a shell script that always exits 1.
    fn make_failing_stub(dir: &Path, name: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Write a git wrapper that passes through everything except `checkout`,
    /// which always exits 1.  This exercises the "checkout -b also fails" path.
    fn make_git_checkout_fail_stub(dir: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let real_git = std::process::Command::new("which")
            .arg("git")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "/usr/bin/git".to_string());
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"checkout\" ]; then exit 1; fi\nexec {real_git} \"$@\"\n"
        );
        let path = dir.join("git");
        std::fs::write(&path, &script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn prepend_path(extra: &Path) -> String {
        let orig = std::env::var("PATH").unwrap_or_default();
        format!("{}:{}", extra.display(), orig)
    }

    // ─── jj tests ──────────────────────────────────────────────────────────────

    #[test]
    fn create_jj_workspace() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("Created workspace 'jj-ws'"));

        assert!(env.home.join(".cow/workspaces/jj-ws").exists());
        assert!(env.home.join(".cow/workspaces/jj-ws/.jj").exists());
    }

    #[test]
    fn list_jj_workspace() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-list", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("jj-list"))
            .stdout(predicate::str::contains("jj"));
    }

    #[test]
    fn status_jj_clean() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-status", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["status", "jj-status"])
            .assert()
            .success()
            .stdout(predicate::str::contains("VCS:        jj"))
            .stdout(predicate::str::contains("Status:     clean"));
    }

    #[test]
    fn status_jj_dirty() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-dirty", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Modify a tracked file to make the working copy dirty.
        std::fs::write(
            env.home.join(".cow/workspaces/jj-dirty/hello.txt"),
            "modified content",
        )
        .unwrap();

        env.cow()
            .args(["status", "jj-dirty"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Status:     dirty"));
    }

    #[test]
    fn diff_jj_workspace() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-diff", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Modify a file so there is something to show.
        std::fs::write(
            env.home.join(".cow/workspaces/jj-diff/hello.txt"),
            "modified",
        )
        .unwrap();

        env.cow().args(["diff", "jj-diff"]).assert().success();
    }

    #[test]
    fn remove_jj_force() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-remove", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["remove", "--force", "jj-remove"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed workspace 'jj-remove'"));

        assert!(!env.home.join(".cow/workspaces/jj-remove").exists());
    }

    #[test]
    fn remove_jj_dirty_note() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-dirty-rm", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Make the workspace dirty.
        std::fs::write(
            env.home.join(".cow/workspaces/jj-dirty-rm/hello.txt"),
            "changed",
        )
        .unwrap();

        env.cow()
            .args(["remove", "--force", "jj-dirty-rm"])
            .assert()
            .success()
            .stderr(predicate::str::contains("has modifications"))
            .stdout(predicate::str::contains("Removed workspace"));
    }

    #[test]
    fn extract_jj_patch() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-patch", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Add a change so the patch is non-empty.
        std::fs::write(
            env.home.join(".cow/workspaces/jj-patch/hello.txt"),
            "patched content",
        )
        .unwrap();

        let patch_file = env.home.join("test.patch");
        env.cow()
            .args([
                "extract",
                "jj-patch",
                "--patch",
                patch_file.to_str().unwrap(),
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Patch written to"));

        assert!(patch_file.exists());
    }

    #[test]
    fn extract_jj_branch() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-branch", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/jj-branch");

        // Make a change in the workspace and commit it with jj.
        std::fs::write(workspace.join("feature.txt"), "feature content").unwrap();
        jj_run(&env.home, &workspace, &["describe", "-m", "add feature"]);
        jj_run(&env.home, &workspace, &["new"]);

        env.cow()
            .args(["extract", "jj-branch", "--branch", "my-feature"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Branch 'my-feature' created in source repo"));

        // Verify the branch exists in the source repo.
        let output = std::process::Command::new("git")
            .args(["branch"])
            .current_dir(source.path())
            .output()
            .expect("git branch");
        let branches = String::from_utf8_lossy(&output.stdout);
        assert!(branches.contains("my-feature"), "branch not found in source: {}", branches);
    }

    // ─── command-failure tests ──────────────────────────────────────────────────

    #[test]
    fn create_branch_checkout_failure() {
        let env = Env::new();
        let source = make_git_repo();

        let stub_dir = TempDir::new().expect("stub dir");
        make_git_checkout_fail_stub(stub_dir.path());

        env.cow()
            .args([
                "create",
                "branch-fail-ws",
                "--source",
                source.path().to_str().unwrap(),
                "--branch",
                "new-branch",
            ])
            .env("PATH", prepend_path(stub_dir.path()))
            .assert()
            .failure()
            .stderr(predicate::str::contains("Failed to check out branch"));
    }

    // ─── non-APFS test ─────────────────────────────────────────────────────────

    #[test]
    #[cfg(target_os = "macos")]
    fn create_non_apfs_source_gives_error() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "apfs-fail", "--source", source.path().to_str().unwrap()])
            .env("COW_TEST_NOT_APFS", "1")
            .assert()
            .failure()
            .stderr(predicate::str::contains("not APFS"));
    }

    // ─── additional coverage ───────────────────────────────────────────────────

    #[test]
    fn list_shows_dirty_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "dirty-list-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Add an untracked file to make the workspace dirty.
        std::fs::write(
            env.home.join(".cow/workspaces/dirty-list-ws/untracked.txt"),
            "new file",
        )
        .unwrap();

        env.cow()
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("dirty"));
    }

    #[test]
    fn remove_jj_without_force_defaults_to_no() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-no-force", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Without --force on a jj workspace, confirm_or_default is called.
        // Non-TTY stdin defaults to no → workspace is NOT removed.
        env.cow()
            .args(["remove", "jj-no-force"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No workspaces were removed"));

        assert!(env.home.join(".cow/workspaces/jj-no-force").exists());
    }

    #[test]
    fn create_jj_with_change() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        // Get the change ID of the parent of the working copy ("initial" change).
        let output = std::process::Command::new("jj")
            .args(["log", "--no-graph", "-r", "@-", "-T", "change_id"])
            .current_dir(source.path())
            .env("HOME", &env.home)
            .output()
            .expect("jj log failed");
        let change_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert!(!change_id.is_empty(), "could not get change ID from jj log");

        env.cow()
            .args([
                "create",
                "jj-with-change",
                "--source",
                source.path().to_str().unwrap(),
                "--change",
                &change_id,
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Created workspace 'jj-with-change'"));
    }

    #[test]
    fn create_jj_with_invalid_change_fails() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args([
                "create",
                "jj-bad-change",
                "--source",
                source.path().to_str().unwrap(),
                "--change",
                "this-is-not-a-valid-change-id",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Failed to check out change"));
    }

    #[test]
    fn diff_git_command_failure() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "diff-fail-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Stub git so `git diff` exits 1.
        let stub_dir = TempDir::new().expect("stub dir");
        {
            use std::os::unix::fs::PermissionsExt;
            let real_git = std::process::Command::new("which")
                .arg("git")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "/usr/bin/git".to_string());
            let script = format!(
                "#!/bin/sh\nif [ \"$1\" = \"diff\" ]; then exit 1; fi\nexec {real_git} \"$@\"\n"
            );
            let stub_path = stub_dir.path().join("git");
            std::fs::write(&stub_path, &script).unwrap();
            std::fs::set_permissions(&stub_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        env.cow()
            .args(["diff", "diff-fail-ws"])
            .env("PATH", prepend_path(stub_dir.path()))
            .assert()
            .failure()
            .stderr(predicate::str::contains("Diff command exited with status"));
    }

    #[test]
    fn extract_jj_patch_command_failure() {
        let env = Env::new();
        let source = make_jj_repo(&env.home);

        env.cow()
            .args(["create", "jj-patch-fail", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Stub jj so `jj diff` exits 1 → patch bail is triggered.
        let stub_dir = TempDir::new().expect("stub dir");
        {
            use std::os::unix::fs::PermissionsExt;
            let real_jj = std::process::Command::new("which")
                .arg("jj")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "/usr/local/bin/jj".to_string());
            let script = format!(
                "#!/bin/sh\nif [ \"$1\" = \"diff\" ]; then exit 1; fi\nexec {real_jj} \"$@\"\n"
            );
            let stub_path = stub_dir.path().join("jj");
            std::fs::write(&stub_path, &script).unwrap();
            std::fs::set_permissions(&stub_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let patch_file = env.home.join("fail.patch");
        env.cow()
            .args(["extract", "jj-patch-fail", "--patch", patch_file.to_str().unwrap()])
            .env("PATH", prepend_path(stub_dir.path()))
            .assert()
            .failure()
            .stderr(predicate::str::contains("Patch command failed"));
    }

    #[test]
    fn extract_branch_fails_when_fetch_errors() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "fetch-fail-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Stub git so that `git fetch` exits non-zero.
        let stub_dir = TempDir::new().expect("stub dir");
        {
            use std::os::unix::fs::PermissionsExt;
            let real_git = std::process::Command::new("which")
                .arg("git")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "/usr/bin/git".to_string());
            let script = format!(
                "#!/bin/sh\nif [ \"$1\" = \"fetch\" ]; then exit 1; fi\nexec {real_git} \"$@\"\n"
            );
            let stub_path = stub_dir.path().join("git");
            std::fs::write(&stub_path, &script).unwrap();
            std::fs::set_permissions(&stub_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        env.cow()
            .args(["extract", "fetch-fail-ws", "--branch", "feature-branch"])
            .env("PATH", prepend_path(stub_dir.path()))
            .assert()
            .failure()
            .stderr(predicate::str::contains("Failed to create branch"));
    }

    #[test]
    fn mcp_call_unknown_tool() {
        let env = Env::new();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "no_such_tool",
                "arguments": {}
            }
        });
        let input = format!("{}\n", req);

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(input.as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let text = String::from_utf8_lossy(&raw);
        let resp: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        assert!(resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Unknown tool"));
    }

    #[test]
    fn mcp_cow_sync() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "mcp-sync-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Advance source so there is something to sync.
        std::fs::write(source.path().join("mcp_synced.txt"), "synced").unwrap();
        git(source.path(), &["add", "mcp_synced.txt"]);
        git(source.path(), &["commit", "-m", "advance"]);

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_sync",
                "arguments": {
                    "name": "mcp-sync-ws",
                    "source_branch": "main"
                }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&raw).trim()).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(resp["result"]["content"][0]["text"].as_str().unwrap().contains("Synced"));

        let workspace = env.home.join(".cow/workspaces/mcp-sync-ws");
        assert!(workspace.join("mcp_synced.txt").exists());
    }

    #[test]
    fn mcp_cow_extract_branch() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "mcp-extract-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/mcp-extract-ws");
        std::fs::write(workspace.join("mcp_feature.txt"), "feature").unwrap();
        git(&workspace, &["add", "mcp_feature.txt"]);
        git(&workspace, &["commit", "-m", "add feature"]);

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_extract",
                "arguments": {
                    "name": "mcp-extract-ws",
                    "branch": "mcp-feature-branch"
                }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&raw).trim()).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(resp["result"]["content"][0]["text"].as_str().unwrap().contains("created in source repo"));

        let branch_exists = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "mcp-feature-branch"])
            .current_dir(source.path())
            .status()
            .unwrap()
            .success();
        assert!(branch_exists, "branch should exist in source repo");
    }

    #[test]
    fn mcp_cow_extract_patch() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "mcp-patch-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/mcp-patch-ws");
        std::fs::write(workspace.join("patch_feature.txt"), "patch feature").unwrap();
        git(&workspace, &["add", "patch_feature.txt"]);
        git(&workspace, &["commit", "-m", "add patch feature"]);

        let patch_path = env.home.join("mcp_out.patch");

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_extract",
                "arguments": {
                    "name": "mcp-patch-ws",
                    "patch": patch_path.to_str().unwrap()
                }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&raw).trim()).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(patch_path.exists(), "patch file should be written");
    }

    #[test]
    fn mcp_cow_sync_merge_flag() {
        // Exercises the --merge path in the MCP cow_sync dispatch.
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "mcp-merge-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        std::fs::write(source.path().join("mcp_merge.txt"), "merge").unwrap();
        git(source.path(), &["add", "mcp_merge.txt"]);
        git(source.path(), &["commit", "-m", "advance"]);

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_sync",
                "arguments": {
                    "name": "mcp-merge-ws",
                    "source_branch": "main",
                    "merge": true
                }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&raw).trim()).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert!(env.home.join(".cow/workspaces/mcp-merge-ws/mcp_merge.txt").exists());
    }

    #[test]
    fn mcp_cow_extract_no_flags_is_error() {
        // Calling cow_extract with neither branch nor patch should return isError: true.
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "mcp-noflag-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_extract",
                "arguments": { "name": "mcp-noflag-ws" }
            }
        });

        let raw = env.cow()
            .arg("mcp")
            .write_stdin(format!("{}\n", req).as_str())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let resp: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&raw).trim()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        assert!(resp["result"]["content"][0]["text"].as_str().unwrap().contains("--patch"));
    }

    // ─── migrate ───────────────────────────────────────────────────────────────

    #[test]
    fn migrate_no_candidates_prints_message() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["migrate", "--source", source.path().to_str().unwrap(), "--all"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No candidates found"));
    }

    #[test]
    fn migrate_without_all_lists_candidates() {
        let env = Env::new();
        let source = make_git_repo();

        // Add a linked worktree to the source.
        let wt_parent = TempDir::new().unwrap();
        let wt_path = wt_parent.path().join("my-feature");
        git(source.path(), &["worktree", "add", "-b", "feature", wt_path.to_str().unwrap()]);

        env.cow()
            .args(["migrate", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("my-feature"))
            .stdout(predicate::str::contains("--all"));
    }

    #[test]
    fn migrate_dry_run_makes_no_changes() {
        let env = Env::new();
        let source = make_git_repo();

        let wt_parent = TempDir::new().unwrap();
        let wt_path = wt_parent.path().join("dry-feature");
        git(source.path(), &["worktree", "add", "-b", "dry-branch", wt_path.to_str().unwrap()]);

        env.cow()
            .args([
                "migrate",
                "--source", source.path().to_str().unwrap(),
                "--all",
                "--dry-run",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("[dry-run]"));

        // Worktree still exists.
        assert!(wt_path.exists(), "worktree should still exist after dry-run");

        // Nothing registered in state.
        let state_file = env.home.join(".cow/state.json");
        assert!(!state_file.exists(), "state file should not be created by dry-run");
    }

    #[test]
    fn migrate_skips_dirty_git_worktree_without_force() {
        let env = Env::new();
        let source = make_git_repo();

        let wt_parent = TempDir::new().unwrap();
        let wt_path = wt_parent.path().join("dirty-feature");
        git(source.path(), &["worktree", "add", "-b", "dirty-branch", wt_path.to_str().unwrap()]);

        // Make the worktree dirty.
        std::fs::write(wt_path.join("untracked.txt"), "dirty").unwrap();

        env.cow()
            .args([
                "migrate",
                "--source", source.path().to_str().unwrap(),
                "--all",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Skipping 'dirty-feature'"));

        // Worktree still exists; nothing was migrated.
        assert!(wt_path.exists());
        let state_file = env.home.join(".cow/state.json");
        assert!(!state_file.exists(), "nothing should be registered");
    }

    #[test]
    fn migrate_git_worktree_creates_cow_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        let wt_parent = TempDir::new().unwrap();
        let wt_path = wt_parent.path().join("wt-feature");
        git(source.path(), &["worktree", "add", "-b", "wt-branch", wt_path.to_str().unwrap()]);
        // Add a commit in the worktree so we have something to check.
        std::fs::write(wt_path.join("wt_file.txt"), "from worktree").unwrap();
        git(&wt_path, &["add", "."]);
        git(&wt_path, &["commit", "-m", "worktree commit"]);

        env.cow()
            .args([
                "migrate",
                "--source", source.path().to_str().unwrap(),
                "--all",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Migrated 'wt-feature'"));

        // A new cow workspace should exist.
        let ws = env.home.join(".cow/workspaces/wt-feature");
        assert!(ws.exists(), "cow workspace directory should exist");
        assert!(ws.join(".git").is_dir(), "workspace should be a git repo");
        assert!(ws.join("hello.txt").exists(), "source files should be present");

        // The old worktree should have been removed.
        assert!(!wt_path.exists(), "old worktree should be removed");

        // State should have one entry.
        env.cow()
            .args(["list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("wt-feature"));
    }

    #[test]
    fn migrate_dirty_git_worktree_with_force() {
        let env = Env::new();
        let source = make_git_repo();

        let wt_parent = TempDir::new().unwrap();
        let wt_path = wt_parent.path().join("forced-feature");
        git(source.path(), &["worktree", "add", "-b", "forced-branch", wt_path.to_str().unwrap()]);
        std::fs::write(wt_path.join("dirty.txt"), "dirty").unwrap();

        env.cow()
            .args([
                "migrate",
                "--source", source.path().to_str().unwrap(),
                "--all",
                "--force",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Migrated 'forced-feature'"));

        let ws = env.home.join(".cow/workspaces/forced-feature");
        assert!(ws.exists(), "workspace should be created even when dirty with --force");
    }

    #[test]
    fn migrate_orphaned_workspace_registers_in_state() {
        let env = Env::new();
        let source = make_git_repo();
        let source_path = source.path().canonicalize().unwrap();

        // Create an orphaned workspace: in ~/.cow/workspaces but not in state.
        let ws_dir = env.home.join(".cow/workspaces");
        std::fs::create_dir_all(&ws_dir).unwrap();
        let orphan = ws_dir.join("orphaned-ws");

        // Clone the source into the orphan path.
        std::process::Command::new("git")
            .args(["clone", source_path.to_str().unwrap(), orphan.to_str().unwrap()])
            .status()
            .unwrap();

        // Write a .cow-context so migrate can identify the source.
        let ctx = serde_json::json!({
            "name": "orphaned-ws",
            "source": source_path.to_string_lossy(),
            "branch": "main",
            "vcs": "git",
        });
        std::fs::write(orphan.join(".cow-context"), serde_json::to_string_pretty(&ctx).unwrap()).unwrap();

        env.cow()
            .args([
                "migrate",
                "--source", source_path.to_str().unwrap(),
                "--all",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Migrated 'orphaned-ws'"));

        // Directory should still exist (orphaned workspaces are registered in-place).
        assert!(orphan.exists(), "orphaned workspace dir should still exist");

        // Should be listed in cow list.
        env.cow()
            .args(["list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("orphaned-ws"));
    }

    #[test]
    fn migrate_rejects_git_worktree_as_source() {
        let env = Env::new();
        let source = make_git_repo();

        let wt_parent = TempDir::new().unwrap();
        let wt_path = wt_parent.path().join("a-worktree");
        git(source.path(), &["worktree", "add", "-b", "wt-br", wt_path.to_str().unwrap()]);

        env.cow()
            .args(["migrate", "--source", wt_path.to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("git worktree"));
    }
}

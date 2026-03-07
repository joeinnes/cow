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
    fn extract_branch_pushes() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "push-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = env.home.join(".cow/workspaces/push-ws");

        // Create a bare repo as a local "origin" remote
        let remote_dir = TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init", "--bare"])
            .current_dir(remote_dir.path())
            .status()
            .unwrap();

        git(&workspace, &["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        env.cow()
            .args(["extract", "push-ws", "--branch", "feat/extracted"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Pushed to origin/feat/extracted"));
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
        assert_eq!(tools.len(), 4);

        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"cow_create"));
        assert!(names.contains(&"cow_list"));
        assert!(names.contains(&"cow_remove"));
        assert!(names.contains(&"cow_status"));
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
}

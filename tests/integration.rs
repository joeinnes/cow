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

    /// Return the basename of a source TempDir as a string slice.
    fn src_name(source: &TempDir) -> &str {
        source.path().file_name().unwrap().to_str().unwrap()
    }

    /// Build the expected pasture path: `~/.cow/pastures/<src_basename>/<name>`.
    fn ws_path(home: &Path, source: &TempDir, name: &str) -> PathBuf {
        home.join(format!(".cow/pastures/{}/{}", src_name(source), name))
    }

    /// Return the scoped workspace name: `<src_basename>/<name>`.
    fn scoped(source: &TempDir, name: &str) -> String {
        format!("{}/{}", src_name(source), name)
    }

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
            .stdout(predicate::str::contains("my-workspace"));

        let workspace = ws_path(&env.home, &source, "my-workspace");
        assert!(workspace.exists(), "workspace directory should exist");
        assert!(workspace.join(".git").is_dir(), "workspace should be a git repo");
        assert!(workspace.join("hello.txt").exists(), "files should be cloned");
    }

    #[test]
    fn create_auto_names_are_sequential() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path().to_str().unwrap();

        // Auto names are scoped: <source-basename>/agent-1 etc.
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

        let workspace = ws_path(&env.home, &source, "feat-ws");
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

        let workspace = ws_path(&env.home, &source, "existing-ws");
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

    fn workspace_branch(home: &std::path::Path, source: &TempDir, name: &str) -> String {
        let ws = ws_path(home, source, name);
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

        assert_eq!(workspace_branch(&env.home, &source, "my-feature"), "my-feature");
    }

    #[test]
    fn create_no_branch_flag_stays_on_source_branch() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "my-feature", "--source", source.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        assert_eq!(workspace_branch(&env.home, &source, "my-feature"), "main");
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
        assert_eq!(workspace_branch(&env.home, &source, "agent-1"), "main");
    }

    #[test]
    fn create_branch_flag_overrides_name() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "my-ws", "--source", source.path().to_str().unwrap(), "--branch", "other-branch"])
            .assert()
            .success();

        assert_eq!(workspace_branch(&env.home, &source, "my-ws"), "other-branch");
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

        // Both create calls use the same source, so the scoped name collides.
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

        // Pre-create the destination (scoped path) to trigger the "already exists on disk" error.
        let dest = ws_path(&env.home, &source, "pre-existing");
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
    fn create_invalid_name_multiple_slashes() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "foo/bar/baz", "--source", source.path().to_str().unwrap()])
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
        assert!(!ws_path(&env.home, &source, "clean-ws").join("server.pid").exists());
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
        assert!(ws_path(&env.home, &source, "noclean-ws").join("server.pid").exists());
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

        assert!(!ws_path(&env.home, &source, "config-ws").join("to_delete.txt").exists());
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

        assert!(!ws_path(&env.home, &source, "dir-config-ws").join("to_delete_dir").exists());
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

        assert!(ws_path(&env.home, &source, "run-ws").join("post_clone_ran.txt").exists());
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

        let ctx_path = ws_path(&env.home, &source, "ctx-ws").join(".cow-context");
        assert!(ctx_path.exists(), ".cow-context should be written into workspace root");

        let content = std::fs::read_to_string(&ctx_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).expect("should be valid JSON");

        // Name stored in context file includes the scope prefix.
        let expected_name = format!("{}/ctx-ws", src_name(&source));
        assert_eq!(v["name"], expected_name);
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

        let workspace = ws_path(&env.home, &source, "ctx-clean-ws");
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert!(output.stdout.is_empty(), ".cow-context should not make workspace dirty");
    }

    #[test]
    fn create_writes_agent_context_files() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "agent-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let ws = ws_path(&env.home, &source, "agent-ws");
        // Agent files go in the scope directory (parent of the workspace),
        // not inside the workspace itself — no risk of clobbering project files.
        let scope_dir = ws.parent().unwrap();
        assert!(scope_dir.join("AGENTS.md").exists(), "AGENTS.md should be in scope dir");
        assert!(scope_dir.join("CLAUDE.md").exists(), "CLAUDE.md should be in scope dir");
        assert!(scope_dir.join("GEMINI.md").exists(), "GEMINI.md should be in scope dir");

        // AGENTS.md should mention the source path.
        let agents = std::fs::read_to_string(scope_dir.join("AGENTS.md")).unwrap();
        assert!(
            agents.contains(source.path().to_str().unwrap()),
            "AGENTS.md should contain the source path"
        );

        // CLAUDE.md and GEMINI.md should point at AGENTS.md.
        let claude = std::fs::read_to_string(scope_dir.join("CLAUDE.md")).unwrap();
        assert!(claude.contains("AGENTS.md"), "CLAUDE.md should reference AGENTS.md");
        let gemini = std::fs::read_to_string(scope_dir.join("GEMINI.md")).unwrap();
        assert!(gemini.contains("AGENTS.md"), "GEMINI.md should reference AGENTS.md");

        // Agent files must NOT be inside the workspace (would risk clobbering).
        assert!(!ws.join("AGENTS.md").exists(), "AGENTS.md must not be inside the workspace");
        assert!(!ws.join("CLAUDE.md").exists(), "CLAUDE.md must not be inside the workspace");
        assert!(!ws.join("GEMINI.md").exists(), "GEMINI.md must not be inside the workspace");
    }

    #[test]
    fn create_second_workspace_does_not_overwrite_agents_md() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "ws-one", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let ws_one = ws_path(&env.home, &source, "ws-one");
        let scope_dir = ws_one.parent().unwrap();
        let original = std::fs::read_to_string(scope_dir.join("AGENTS.md")).unwrap();
        let original_mtime = std::fs::metadata(scope_dir.join("AGENTS.md")).unwrap().modified().unwrap();

        env.cow()
            .args(["create", "ws-two", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let after_mtime = std::fs::metadata(scope_dir.join("AGENTS.md")).unwrap().modified().unwrap();
        assert_eq!(original_mtime, after_mtime, "AGENTS.md should not be rewritten for a second workspace");
        assert_eq!(
            original,
            std::fs::read_to_string(scope_dir.join("AGENTS.md")).unwrap(),
            "AGENTS.md content should be unchanged"
        );
    }

    #[test]
    fn agent_context_files_not_inside_git_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "agent-clean-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let ws = ws_path(&env.home, &source, "agent-clean-ws");
        // Scope-dir files are outside the git repo — git status should be clean.
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&ws)
            .output()
            .unwrap();
        assert!(
            output.stdout.is_empty(),
            "workspace git status should be clean; got: {}",
            String::from_utf8_lossy(&output.stdout)
        );
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

        let base = src_name(&source);
        let expected1 = format!("{}/list-ws-1", base);
        let expected2 = format!("{}/list-ws-2", base);
        assert!(names.iter().any(|n| *n == expected1), "expected {} in {:?}", expected1, names);
        assert!(names.iter().any(|n| *n == expected2), "expected {} in {:?}", expected2, names);
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
            .stdout(predicate::str::contains("STATUS"))
            .stdout(predicate::str::contains("table-ws"));
    }

    #[test]
    fn list_empty_state() {
        let env = Env::new();

        env.cow()
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("No pastures found."));
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

        let scoped1 = format!("{}/from-s1", src_name(&source1));
        let scoped2 = format!("{}/from-s2", src_name(&source2));
        assert!(names.iter().any(|n| *n == scoped1), "expected {} in {:?}", scoped1, names);
        assert!(!names.iter().any(|n| *n == scoped2), "source filter should exclude {}", scoped2);
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

        let workspace = ws_path(&env.home, &source, "to-remove");
        assert!(workspace.exists());

        env.cow()
            .args(["remove", "--force", &scoped(&source, "to-remove")])
            .assert()
            .success()
            .stdout(predicate::str::contains("to-remove"));

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
        let workspace = ws_path(&env.home, &source, "dirty-ws");
        std::fs::write(workspace.join("change.txt"), "modified").unwrap();
        git(&workspace, &["add", "change.txt"]);

        env.cow()
            .args(["remove", "--force", &scoped(&source, "dirty-ws")])
            .assert()
            .success()
            .stdout(predicate::str::contains("dirty-ws"));

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
            .stderr(predicate::str::contains("Specify one or more pasture names"));
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

        assert!(!ws_path(&env.home, &source, "ws-a").exists());
        assert!(!ws_path(&env.home, &source, "ws-b").exists());
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

        assert!(!ws_path(&env.home, &source1, "from-s1").exists(), "from-s1 should be removed");
        assert!(ws_path(&env.home, &source2, "from-s2").exists(), "from-s2 should remain");
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
            .stdout(predicate::str::contains("No pastures to remove."));
    }

    #[test]
    fn remove_dirty_without_force_defaults_to_no() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "dirty-keep", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = ws_path(&env.home, &source, "dirty-keep");
        std::fs::write(workspace.join("change.txt"), "modified").unwrap();
        git(&workspace, &["add", "change.txt"]);

        // Without --force, non-TTY stdin defaults to "no" → workspace preserved
        env.cow()
            .args(["remove", &scoped(&source, "dirty-keep")])
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
        std::fs::write(ws_path(&env.home, &source, "list-json-ws").join("wip.txt"), "wip").unwrap();

        let raw = env.cow()
            .args(["list", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let arr: serde_json::Value = serde_json::from_slice(&raw).expect("valid JSON");
        let scoped_name = format!("{}/list-json-ws", src_name(&source));
        let ws = arr.as_array().unwrap()
            .iter()
            .find(|w| w["name"] == scoped_name)
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
        let scoped_name = format!("{}/list-clean-ws", src_name(&source));
        let ws = arr.as_array().unwrap()
            .iter()
            .find(|w| w["name"] == scoped_name)
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
            .args(["status", &scoped(&source, "status-ws")])
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

        let workspace = ws_path(&env.home, &source, "dirty-status");
        std::fs::write(workspace.join("modified.txt"), "changed content").unwrap();
        git(&workspace, &["add", "modified.txt"]);

        env.cow()
            .args(["status", &scoped(&source, "dirty-status")])
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

        let workspace = ws_path(&env.home, &source, "cwd-status");

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
            .args(["status", &scoped(&source, "json-status-ws"), "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let v: serde_json::Value = serde_json::from_slice(&raw).expect("should be valid JSON");
        assert_eq!(v["name"], scoped(&source, "json-status-ws"));
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

        let workspace = ws_path(&env.home, &source, "json-dirty-ws");
        std::fs::write(workspace.join("wip.txt"), "work in progress").unwrap();

        let raw = env.cow()
            .args(["status", &scoped(&source, "json-dirty-ws"), "--json"])
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
            .args(["diff", &scoped(&source, "diff-ws")])
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

        let workspace = ws_path(&env.home, &source, "diff-cwd");

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
            .args(["extract", &scoped(&source, "extract-ws")])
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

        let workspace = ws_path(&env.home, &source, "patch-ws");

        // Make a commit in the workspace so there's something to patch
        std::fs::write(workspace.join("new_feature.txt"), "feature content").unwrap();
        git(&workspace, &["add", "new_feature.txt"]);
        git(&workspace, &["commit", "-m", "add new feature"]);

        let patch_file = env.home.join("changes.patch");

        env.cow()
            .args(["extract", &scoped(&source, "patch-ws"), "--patch", patch_file.to_str().unwrap()])
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

        let workspace = ws_path(&env.home, &source, "branch-ws");

        // Commit something in the workspace so branch-ws diverges from source.
        std::fs::write(workspace.join("feature.txt"), "feature").unwrap();
        git(&workspace, &["add", "feature.txt"]);
        git(&workspace, &["commit", "-m", "add feature"]);

        env.cow()
            .args(["extract", &scoped(&source, "branch-ws"), "--branch", "feat/extracted"])
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

        let workspace = ws_path(&env.home, &source, "sync-ws");

        // Add a commit to source's main after workspace was created.
        std::fs::write(source.path().join("synced.txt"), "from source").unwrap();
        git(source.path(), &["add", "synced.txt"]);
        git(source.path(), &["commit", "-m", "source update"]);

        env.cow()
            .args(["sync", &scoped(&source, "sync-ws"), "--source-branch", "main"])
            .assert()
            .success()
            .stdout(predicate::str::contains("sync-ws"));

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

        let workspace = ws_path(&env.home, &source, "merge-ws");

        std::fs::write(source.path().join("merged.txt"), "from source").unwrap();
        git(source.path(), &["add", "merged.txt"]);
        git(source.path(), &["commit", "-m", "source merge update"]);

        env.cow()
            .args(["sync", &scoped(&source, "merge-ws"), "--source-branch", "main", "--merge"])
            .assert()
            .success()
            .stdout(predicate::str::contains("merge-ws"));

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

        let workspace = ws_path(&env.home, &source, "dirty-sync-ws");

        // Leave an uncommitted file in the workspace.
        std::fs::write(workspace.join("uncommitted.txt"), "wip").unwrap();

        env.cow()
            .args(["sync", &scoped(&source, "dirty-sync-ws"), "--source-branch", "main"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("uncommitted changes"));
    }

    #[test]
    fn sync_workspace_not_found() {
        let env = Env::new();

        env.cow()
            .args(["sync", "no-such-ws", "--source-branch", "main"])
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

        let workspace = ws_path(&env.home, &source, "jj-sync-rebase");

        // Advance source: add a commit and pin a bookmark to it.
        std::fs::write(source.path().join("new.txt"), "source update").unwrap();
        jj_run(&env.home, source.path(), &["describe", "-m", "source update"]);
        jj_run(&env.home, source.path(), &["bookmark", "set", "main"]);
        jj_run(&env.home, source.path(), &["new"]);

        env.cow()
            .args(["sync", &scoped(&source, "jj-sync-rebase"), "--source-branch", "main"])
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
            ws_path(&env.home, &source, "jj-sync-dirty").join("dirty.txt"),
            "uncommitted",
        )
        .unwrap();

        env.cow()
            .args(["sync", &scoped(&source, "jj-sync-dirty"), "--source-branch", "main"])
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
            .args(["sync", &scoped(&source, "jj-sync-nobranch")])
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

        let workspace = ws_path(&env.home, &source, "nobranch-ws");

        // Advance source's main.
        std::fs::write(source.path().join("default_sync.txt"), "default").unwrap();
        git(source.path(), &["add", "default_sync.txt"]);
        git(source.path(), &["commit", "-m", "advance main"]);

        // Sync without specifying a branch — should default to workspace's current branch (main).
        env.cow()
            .args(["sync", &scoped(&source, "nobranch-ws")])
            .assert()
            .success()
            .stdout(predicate::str::contains("nobranch-ws"));

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

        let workspace = ws_path(&env.home, &source, "cwd-sync-ws");

        std::fs::write(source.path().join("cwd_synced.txt"), "cwd").unwrap();
        git(source.path(), &["add", "cwd_synced.txt"]);
        git(source.path(), &["commit", "-m", "advance main"]);

        // Run sync from inside the workspace with no --name.
        env.cow()
            .args(["sync", "--source-branch", "main"])
            .current_dir(&workspace)
            .assert()
            .success()
            .stdout(predicate::str::contains(&scoped(&source, "cwd-sync-ws")));

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
            .args(["sync", &scoped(&source, "fetch-err-ws"), "--source-branch", "main"])
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
            .args(["sync", &scoped(&source, "rebase-err-ws"), "--source-branch", "main"])
            .env("PATH", prepend_path(stub_dir.path()))
            .assert()
            .failure()
            .stderr(predicate::str::contains("Failed to rebase pasture"));
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

        let workspace = ws_path(&env.home, &source, "conflict-ws");

        // Workspace modifies hello.txt and commits.
        std::fs::write(workspace.join("hello.txt"), "workspace version").unwrap();
        git(&workspace, &["add", "hello.txt"]);
        git(&workspace, &["commit", "-m", "workspace change"]);

        // Source also modifies hello.txt and commits (different content → conflict on rebase).
        std::fs::write(source.path().join("hello.txt"), "source version").unwrap();
        git(source.path(), &["add", "hello.txt"]);
        git(source.path(), &["commit", "-m", "source change"]);

        env.cow()
            .args(["sync", &scoped(&source, "conflict-ws"), "--source-branch", "main"])
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

        let expected = ws_path(&env.home, &source, "cd-ws");

        let raw = env.cow()
            .args(["cd", &scoped(&source, "cd-ws")])
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

    // ─── exec ──────────────────────────────────────────────────────────────────

    #[test]
    fn exec_runs_command_in_pasture() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "exec-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let ws = ws_path(&env.home, &source, "exec-ws");

        // `pwd` should print the pasture path.
        let out = env.cow()
            .args(["run", &scoped(&source, "exec-ws"), "pwd"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let printed = String::from_utf8_lossy(&out).trim().to_string();
        // canonicalize both sides to absorb any symlink differences
        assert_eq!(
            std::fs::canonicalize(&printed).unwrap(),
            std::fs::canonicalize(&ws).unwrap(),
        );
    }

    #[test]
    fn exec_propagates_exit_code() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "exit-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["run", &scoped(&source, "exit-ws"), "sh", "-c", "exit 42"])
            .assert()
            .code(42);
    }

    #[test]
    fn exec_unknown_pasture_fails() {
        let env = Env::new();

        env.cow()
            .args(["run", "no-such-pasture", "echo", "hi"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    #[test]
    fn run_sets_cow_env_vars() {
        // COW_PASTURE, COW_SOURCE, COW_PASTURE_PATH should be set in subprocess.
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "env-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let name = scoped(&source, "env-ws");
        let pasture = ws_path(&env.home, &source, "env-ws");

        // Check COW_PASTURE
        let out = env.cow()
            .args(["run", &name, "sh", "-c", "printf '%s' \"$COW_PASTURE\""])
            .assert().success().get_output().stdout.clone();
        assert_eq!(String::from_utf8_lossy(&out), name);

        // Check COW_PASTURE_PATH
        let out = env.cow()
            .args(["run", &name, "sh", "-c", "printf '%s' \"$COW_PASTURE_PATH\""])
            .assert().success().get_output().stdout.clone();
        let printed = std::path::PathBuf::from(String::from_utf8_lossy(&out).to_string());
        assert_eq!(
            printed.canonicalize().unwrap_or(printed),
            pasture.canonicalize().unwrap_or(pasture),
        );

        // Check COW_SOURCE
        let out = env.cow()
            .args(["run", &name, "sh", "-c", "printf '%s' \"$COW_SOURCE\""])
            .assert().success().get_output().stdout.clone();
        let printed_src = std::path::PathBuf::from(String::from_utf8_lossy(&out).to_string());
        assert_eq!(
            printed_src.canonicalize().unwrap_or(printed_src),
            source.path().canonicalize().unwrap(),
        );
    }

    #[test]
    fn run_creates_pm_shim_for_npm() {
        // When package-lock.json is present, a shim for npm should be written
        // and the shims directory should be prepended to PATH.
        let env = Env::new();
        let source = make_git_repo();
        std::fs::write(source.path().join("package-lock.json"), "{}").unwrap();

        env.cow()
            .args(["create", "npm-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let name = scoped(&source, "npm-ws");
        let shim_path = env.home.join(".cow/shims/npm");

        // Run any command to trigger shim creation.
        env.cow()
            .args(["run", &name, "true"])
            .assert()
            .success();

        assert!(shim_path.exists(), "npm shim should be created");

        // Shim should be executable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&shim_path).unwrap().permissions().mode();
            assert!(mode & 0o100 != 0, "npm shim should be executable");
        }

        // PATH should be prepended with shims dir.
        let out = env.cow()
            .args(["run", &name, "sh", "-c", "printf '%s' \"$PATH\""])
            .assert().success().get_output().stdout.clone();
        let path_str = String::from_utf8_lossy(&out);
        let shims_dir = env.home.join(".cow/shims").to_string_lossy().into_owned();
        assert!(path_str.starts_with(&shims_dir), "shims dir should be first in PATH");
    }

    #[test]
    fn run_no_pm_shim_without_lockfile() {
        // No lockfile → shims dir should NOT be prepended and no shim written.
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "no-pm-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let name = scoped(&source, "no-pm-ws");

        env.cow()
            .args(["run", &name, "true"])
            .assert()
            .success();

        let shims_dir = env.home.join(".cow/shims");
        // shims dir either doesn't exist or has no files for this PM.
        assert!(!shims_dir.join("npm").exists(), "no npm shim without lockfile");
    }

    // ─── recreate ──────────────────────────────────────────────────────────────

    #[test]
    fn recreate_produces_fresh_workspace() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "fresh-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let ws = ws_path(&env.home, &source, "fresh-ws");
        // Write a file into the workspace to prove it gets wiped.
        std::fs::write(ws.join("canary.txt"), "canary").unwrap();
        assert!(ws.join("canary.txt").exists());

        env.cow()
            .args(["recreate", &scoped(&source, "fresh-ws")])
            .assert()
            .success();

        assert!(ws.exists(), "workspace should exist after recreate");
        assert!(!ws.join("canary.txt").exists(), "workspace should be fresh (canary wiped)");
        assert!(ws.join("hello.txt").exists(), "source files should be present");

        env.cow()
            .args(["list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("fresh-ws"));
    }

    #[test]
    fn recreate_preserves_branch() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "branch-ws", "--branch", "my-branch", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["recreate", &scoped(&source, "branch-ws")])
            .assert()
            .success();

        let ws = ws_path(&env.home, &source, "branch-ws");
        let branch = String::from_utf8(
            std::process::Command::new("git")
                .args(["symbolic-ref", "--short", "HEAD"])
                .current_dir(&ws)
                .output()
                .unwrap()
                .stdout,
        ).unwrap().trim().to_string();
        assert_eq!(branch, "my-branch", "branch should be preserved on recreate");
    }

    #[test]
    fn recreate_with_branch_override() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "override-ws", "--branch", "original", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["recreate", &scoped(&source, "override-ws"), "--branch", "new-branch"])
            .assert()
            .success();

        let ws = ws_path(&env.home, &source, "override-ws");
        let branch = String::from_utf8(
            std::process::Command::new("git")
                .args(["symbolic-ref", "--short", "HEAD"])
                .current_dir(&ws)
                .output()
                .unwrap()
                .stdout,
        ).unwrap().trim().to_string();
        assert_eq!(branch, "new-branch", "--branch override should take effect");
    }

    #[test]
    fn recreate_unknown_pasture_fails() {
        let env = Env::new();

        env.cow()
            .args(["recreate", "no-such-pasture"])
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
        assert_eq!(tools.len(), 10);

        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"cow_create"));
        assert!(names.contains(&"cow_list"));
        assert!(names.contains(&"cow_remove"));
        assert!(names.contains(&"cow_status"));
        assert!(names.contains(&"cow_sync"));
        assert!(names.contains(&"cow_extract"));
        assert!(names.contains(&"cow_migrate"));
        assert!(names.contains(&"cow_materialise"));
        assert!(names.contains(&"cow_fetch_from"));
        assert!(names.contains(&"cow_run"));
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
            .contains("Created pasture"));
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

        let workspace = ws_path(&env.home, &source, "to-mcp-remove");
        assert!(workspace.exists());

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "cow_remove",
                "arguments": {
                    "names": [scoped(&source, "to-mcp-remove")]
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
                    "name": scoped(&source, "mcp-status-ws")
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
            .stdout(predicate::str::contains("cwd-src-ws"));
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

        let workspace = ws_path(&env.home, &source, "branch-ws");
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
        assert!(!ws_path(&env.home, &source, "all-a").exists());
        assert!(!ws_path(&env.home, &source, "all-b").exists());
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
        assert!(!ws_path(&env.home, &source1, "del-ws").exists());
        assert!(ws_path(&env.home, &source2, "keep-ws").exists());
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
        assert!(text.contains("Created pasture"), "should have stdout");
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

        let ws = ws_path(&env.home, &source, "push-ws");
        add_origin_remote(&ws, bare.path());

        // Make a commit in the workspace that is NOT on origin.
        std::fs::write(ws.join("new.txt"), "new").unwrap();
        git(&ws, &["add", "."]);
        git(&ws, &["commit", "-m", "workspace commit"]);

        // --force should remove without prompting, but warn about unpushed commits.
        env.cow()
            .args(["remove", "--force", &scoped(&source, "push-ws")])
            .assert()
            .success()
            .stderr(predicate::str::contains("unpushed"))
            .stdout(predicate::str::contains("Removed pasture"));
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

        let ws = ws_path(&env.home, &source, "push-warn-ws");
        add_origin_remote(&ws, bare.path());

        std::fs::write(ws.join("new.txt"), "new").unwrap();
        git(&ws, &["add", "."]);
        git(&ws, &["commit", "-m", "workspace commit"]);

        // Non-TTY stdin: should warn on stderr and proceed with removal.
        env.cow()
            .args(["remove", &scoped(&source, "push-warn-ws")])
            .assert()
            .success()
            .stderr(predicate::str::contains("unpushed"))
            .stdout(predicate::str::contains("Removed pasture"));
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

        let ws = ws_path(&env.home, &source, "synced-ws");
        add_origin_remote(&ws, bare.path());

        // Nothing committed after push → zero unpushed commits.
        env.cow()
            .args(["remove", "--force", &scoped(&source, "synced-ws")])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed pasture"));
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
            .stdout(predicate::str::contains("jj-ws"));

        assert!(ws_path(&env.home, &source, "jj-ws").exists());
        assert!(ws_path(&env.home, &source, "jj-ws").join(".jj").exists());
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
            .args(["status", &scoped(&source, "jj-status")])
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
            ws_path(&env.home, &source, "jj-dirty").join("hello.txt"),
            "modified content",
        )
        .unwrap();

        env.cow()
            .args(["status", &scoped(&source, "jj-dirty")])
            .assert()
            .success()
            .stdout(predicate::str::contains("Status:     changed (jj working copy)"));
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
            ws_path(&env.home, &source, "jj-diff").join("hello.txt"),
            "modified",
        )
        .unwrap();

        env.cow().args(["diff", &scoped(&source, "jj-diff")]).assert().success();
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
            .args(["remove", "--force", &scoped(&source, "jj-remove")])
            .assert()
            .success()
            .stdout(predicate::str::contains("jj-remove"));

        assert!(!ws_path(&env.home, &source, "jj-remove").exists());
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
            ws_path(&env.home, &source, "jj-dirty-rm").join("hello.txt"),
            "changed",
        )
        .unwrap();

        env.cow()
            .args(["remove", "--force", &scoped(&source, "jj-dirty-rm")])
            .assert()
            .success()
            .stderr(predicate::str::contains("has modifications"))
            .stdout(predicate::str::contains("Removed pasture"));
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
            ws_path(&env.home, &source, "jj-patch").join("hello.txt"),
            "patched content",
        )
        .unwrap();

        let patch_file = env.home.join("test.patch");
        env.cow()
            .args([
                "extract",
                &scoped(&source, "jj-patch"),
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

        let workspace = ws_path(&env.home, &source, "jj-branch");

        // Make a change in the workspace and commit it with jj.
        std::fs::write(workspace.join("feature.txt"), "feature content").unwrap();
        jj_run(&env.home, &workspace, &["describe", "-m", "add feature"]);
        jj_run(&env.home, &workspace, &["new"]);

        env.cow()
            .args(["extract", &scoped(&source, "jj-branch"), "--branch", "my-feature"])
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
            ws_path(&env.home, &source, "dirty-list-ws").join("untracked.txt"),
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
            .args(["remove", &scoped(&source, "jj-no-force")])
            .assert()
            .success()
            .stdout(predicate::str::contains("No pastures were removed"));

        assert!(ws_path(&env.home, &source, "jj-no-force").exists());
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
            .stdout(predicate::str::contains("jj-with-change"));
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
            .stderr(predicate::str::contains("Failed to edit change"));
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
            .args(["diff", &scoped(&source, "diff-fail-ws")])
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
            .args(["extract", &scoped(&source, "jj-patch-fail"), "--patch", patch_file.to_str().unwrap()])
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
            .args(["extract", &scoped(&source, "fetch-fail-ws"), "--branch", "feature-branch"])
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
                    "name": scoped(&source, "mcp-sync-ws"),
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

        let workspace = ws_path(&env.home, &source, "mcp-sync-ws");
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

        let workspace = ws_path(&env.home, &source, "mcp-extract-ws");
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
                    "name": scoped(&source, "mcp-extract-ws"),
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

        let workspace = ws_path(&env.home, &source, "mcp-patch-ws");
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
                    "name": scoped(&source, "mcp-patch-ws"),
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
                    "name": scoped(&source, "mcp-merge-ws"),
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
        assert!(ws_path(&env.home, &source, "mcp-merge-ws").join("mcp_merge.txt").exists());
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
                "arguments": { "name": scoped(&source, "mcp-noflag-ws") }
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

        // A new cow pasture should exist.
        let ws = env.home.join(".cow/pastures/wt-feature");
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
    fn migrate_git_worktree_clears_stale_worktree_refs() {
        let env = Env::new();
        let source = make_git_repo();

        // Add a linked worktree to the source so .git/worktrees/ is populated.
        let wt_parent = TempDir::new().unwrap();
        let wt_path = wt_parent.path().join("stale-feature");
        git(source.path(), &["worktree", "add", "-b", "stale-branch", wt_path.to_str().unwrap()]);

        env.cow()
            .args([
                "migrate",
                "--source", source.path().to_str().unwrap(),
                "--all",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Migrated 'stale-feature'"));

        // The migrated pasture must not contain stale worktree refs.
        let ws = env.home.join(".cow/pastures/stale-feature");
        assert!(ws.exists(), "cow workspace should exist");
        let worktrees_dir = ws.join(".git").join("worktrees");
        assert!(
            !worktrees_dir.exists(),
            ".git/worktrees/ should have been removed from the migrated pasture"
        );
    }

    #[test]
    fn migrate_git_worktree_checkout_failure_rolls_back() {
        let env = Env::new();
        let source = make_git_repo();

        // Create a linked worktree on a new branch.
        let wt_parent = TempDir::new().unwrap();
        let wt_path = wt_parent.path().join("missing-branch");
        git(source.path(), &["worktree", "add", "-b", "missing-branch", wt_path.to_str().unwrap()]);

        // Remove the branch ref directly so the clone won't have it,
        // causing git checkout to fail in the migrated pasture.
        std::fs::remove_file(source.path().join(".git/refs/heads/missing-branch")).unwrap();

        env.cow()
            .args([
                "migrate",
                "--source", source.path().to_str().unwrap(),
                "--all",
            ])
            .assert()
            .success();

        // The failed checkout should have triggered rollback — no directory left.
        let ws = env.home.join(".cow/pastures/missing-branch");
        assert!(!ws.exists(), "rolled-back pasture directory should not exist");

        // The workspace should not be registered in state.
        let out = env.cow()
            .args(["list"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let list = String::from_utf8_lossy(&out);
        assert!(!list.contains("missing-branch"), "failed migrate should not appear in list");
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

        let ws = env.home.join(".cow/pastures/forced-feature");
        assert!(ws.exists(), "pasture should be created even when dirty with --force");
    }

    #[test]
    fn migrate_orphaned_workspace_registers_in_state() {
        let env = Env::new();
        let source = make_git_repo();
        let source_path = source.path().canonicalize().unwrap();

        // Create an orphaned pasture: in ~/.cow/pastures but not in state.
        let ws_dir = env.home.join(".cow/pastures");
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

    // ─── create: atomic rollback ────────────────────────────────────────────

    #[test]
    fn create_rollback_removes_dest_on_post_clone_failure() {
        let env = Env::new();
        let source = make_git_repo();

        // A .cow.json whose post-clone command always fails.
        std::fs::write(
            source.path().join(".cow.json"),
            r#"{"post_clone":{"run":["false"]}}"#,
        ).unwrap();

        env.cow()
            .args(["create", "rollback-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .failure();

        // The workspace directory must not exist after failure.
        let dest = ws_path(&env.home, &source, "rollback-ws");
        assert!(!dest.exists(), "dest dir should be removed after failed create");

        // Nothing should be registered in state.
        env.cow()
            .args(["list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("rollback-ws").not());
    }

    #[test]
    fn create_rollback_does_not_affect_successful_create() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "good-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let dest = ws_path(&env.home, &source, "good-ws");
        assert!(dest.exists(), "dest dir should exist after successful create");
    }

    // ─── create: stale worktree prune ────────────────────────────────────────

    #[test]
    fn create_prunes_stale_git_worktree_refs() {
        let env = Env::new();
        let source = make_git_repo();

        // Add a git worktree to the source so .git/worktrees/ has an entry with
        // absolute paths. The clone will inherit this entry, which will be stale
        // from the clone's perspective (its .git back-link points to the source).
        let wt_dir = TempDir::new().unwrap();
        git(source.path(), &[
            "worktree", "add",
            wt_dir.path().to_str().unwrap(),
            "-b", "feature-stale",
        ]);

        env.cow()
            .args(["create", "pruned-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = ws_path(&env.home, &source, "pruned-ws");

        // git worktree list should show only 1 entry (the workspace itself).
        // Without pruning the clone inherits the stale entry, giving 2 entries.
        let output = std::process::Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&workspace)
            .output()
            .unwrap();

        let stdout = String::from_utf8_lossy(&output.stdout);
        let count = stdout.lines().filter(|l| l.starts_with("worktree ")).count();
        assert_eq!(count, 1, "workspace should have no stale worktree refs, got:\n{}", stdout);
    }

    // ─── list: path display ──────────────────────────────────────────────────

    #[test]
    fn list_source_basename_appears_in_scoped_name() {
        let env = Env::new();

        // Create the source repo inside the fake HOME with a recognisable name.
        let source = env.home.join("projects/myrepo");
        std::fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-b", "main"]);
        git(&source, &["config", "user.email", "test@cow.test"]);
        git(&source, &["config", "user.name", "cow-test"]);
        git(&source, &["config", "commit.gpgsign", "false"]);
        git(&source, &["config", "tag.gpgsign", "false"]);
        std::fs::write(source.join("hello.txt"), "hello").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "-m", "initial"]);

        env.cow()
            .args(["create", "home-ws", "--source", source.to_str().unwrap()])
            .assert()
            .success();

        // The scoped name `myrepo/home-ws` should appear in the list output.
        env.cow()
            .args(["list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("myrepo/home-ws"));
    }

    // ─── create: output messages ─────────────────────────────────────────────

    #[test]
    fn create_output_includes_remove_hint() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "hint-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            // The remove hint includes the scoped name (e.g., ".tmpXXX/hint-ws").
            .stdout(predicate::str::contains("cow remove"))
            .stdout(predicate::str::contains("hint-ws"));
    }

    #[test]
    fn create_output_shows_detected_vcs() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "vcs-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("Detected VCS: git"));
    }

    #[test]
    fn create_print_path_suppresses_vcs_line() {
        let env = Env::new();
        let source = make_git_repo();

        let out = env.cow()
            .args(["create", "quiet-ws", "--source", source.path().to_str().unwrap(), "--print-path"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let text = String::from_utf8_lossy(&out);
        assert!(!text.contains("Detected VCS"), "--print-path should suppress VCS line");
    }

    #[test]
    fn create_output_mentions_cow_json_when_absent() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "nojson-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(".cow.json"));
    }

    #[test]
    fn create_output_does_not_mention_cow_json_when_present() {
        let env = Env::new();
        let source = make_git_repo();

        std::fs::write(
            source.path().join(".cow.json"),
            r#"{"post_clone":{}}"#,
        ).unwrap();

        env.cow()
            .args(["create", "hasjson-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(".cow.json").not());
    }

    #[test]
    fn remove_yes_skips_dirty_confirm() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "dirty-yes", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = ws_path(&env.home, &source, "dirty-yes");
        std::fs::write(workspace.join("change.txt"), "modified").unwrap();
        git(&workspace, &["add", "change.txt"]);

        // --yes should skip the "remove anyway?" prompt but still print the warning
        env.cow()
            .args(["remove", "--yes", &scoped(&source, "dirty-yes")])
            .assert()
            .success()
            .stderr(predicate::str::contains("uncommitted changes"))
            .stdout(predicate::str::contains("dirty-yes"));

        assert!(!workspace.exists(), "workspace should be deleted");
    }

    #[test]
    fn create_print_path_outputs_only_path() {
        let env = Env::new();
        let source = make_git_repo();

        let output = env.cow()
            .args(["create", "path-ws", "--source", source.path().to_str().unwrap(), "--print-path"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let stdout = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = stdout.lines().collect();

        // Only one line: the path
        assert_eq!(lines.len(), 1, "expected only the path on stdout, got: {:?}", lines);
        assert!(lines[0].contains("path-ws"), "path should contain workspace name");
        assert!(std::path::Path::new(lines[0].trim()).exists(), "printed path should exist on disk");
    }

    #[test]
    fn list_text_shows_dirty_file_count() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "count-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let workspace = ws_path(&env.home, &source, "count-ws");
        std::fs::write(workspace.join("a.txt"), "a").unwrap();
        std::fs::write(workspace.join("b.txt"), "b").unwrap();
        git(&workspace, &["add", "a.txt", "b.txt"]);

        env.cow()
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("dirty (2)"));
    }

    // ─── scoped name tests ───────────────────────────────────────────────────

    #[test]
    fn create_name_is_auto_scoped() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "feature-x", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        let expected_name = scoped(&source, "feature-x");
        let dest = ws_path(&env.home, &source, "feature-x");
        assert!(dest.exists(), "scoped workspace path should exist");

        env.cow()
            .args(["list"])
            .assert()
            .success()
            .stdout(predicate::str::contains(&expected_name));
    }

    #[test]
    fn create_scoped_name_explicit() {
        let env = Env::new();
        let source = make_git_repo();

        env.cow()
            .args(["create", "other/feature-x", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();

        // Explicit scoped name used as-is — path is under "other/feature-x"
        let dest = env.home.join(".cow/pastures/other/feature-x");
        assert!(dest.exists(), "explicitly scoped pasture path should exist");

        env.cow()
            .args(["list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("other/feature-x"));
    }

    #[test]
    fn create_different_sources_same_branch_no_collision() {
        let env = Env::new();
        let source_a = make_git_repo();
        let source_b = make_git_repo();

        env.cow()
            .args(["create", "develop", "--source", source_a.path().to_str().unwrap()])
            .assert()
            .success();

        env.cow()
            .args(["create", "develop", "--source", source_b.path().to_str().unwrap()])
            .assert()
            .success();

        // Both workspaces should exist under their own scoped paths
        assert!(ws_path(&env.home, &source_a, "develop").exists(), "source_a/develop should exist");
        assert!(ws_path(&env.home, &source_b, "develop").exists(), "source_b/develop should exist");

        // List should contain both scoped names
        let out = env.cow()
            .args(["list"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let list = String::from_utf8_lossy(&out);
        assert!(list.contains(&scoped(&source_a, "develop")));
        assert!(list.contains(&scoped(&source_b, "develop")));
    }

    // ─── helpers (shared by new tests) ─────────────────────────────────────────

    fn read_state(home: &std::path::Path) -> serde_json::Value {
        let path = home.join(".cow/state.json");
        let content = std::fs::read_to_string(&path).expect("state.json should exist");
        serde_json::from_str(&content).expect("state.json should be valid JSON")
    }

    fn is_symlink(path: &std::path::Path) -> bool {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    // ─── cow-5xts: auto-symlink large dirs ─────────────────────────────────────

    #[test]
    fn create_auto_symlinks_large_dir() {
        // Non-dep dir (fixtures/) should be whole-dir symlinked.
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        // Large dir: 5 files, threshold set to 3 via .cow.json
        let fixtures = src.join("fixtures");
        std::fs::create_dir_all(&fixtures).unwrap();
        for i in 0..5 { std::fs::write(fixtures.join(format!("f{}.json", i)), "{}").unwrap(); }

        // Small dir: below threshold
        let small = src.join("src");
        std::fs::create_dir_all(&small).unwrap();
        std::fs::write(small.join("main.rs"), "fn main(){}").unwrap();

        std::fs::write(
            src.join(".cow.json"),
            r#"{"pre_clone":{"symlink_threshold":3}}"#,
        ).unwrap();

        let out = env.cow()
            .args(["create", "test", "--source", src.to_str().unwrap(), "--no-branch"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let stdout = String::from_utf8_lossy(&out);
        assert!(stdout.contains("fixtures"), "warning should name the dir");
        assert!(stdout.contains("cow materialise"), "warning should mention materialise");
        assert!(stdout.contains("writes affect source"), "whole-dir warning text");

        let pasture = ws_path(&env.home, &source, "test");
        assert!(is_symlink(&pasture.join("fixtures")), "fixtures should be a whole-dir symlink");
        assert!(!is_symlink(&pasture.join("src")), "src should NOT be a symlink");

        let state = read_state(&env.home);
        let dirs = &state["pastures"][0]["symlinked_dirs"];
        assert!(dirs.as_array().unwrap().iter().any(|v| v.as_str() == Some("fixtures")));
    }

    #[test]
    fn create_per_package_symlinks_dep_dir() {
        // Dep dir (node_modules/) should use per-package symlinks: real dir,
        // each top-level entry is a symlink. New writes go local.
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        // Simulate node_modules with packages (threshold 3, 5 packages).
        let nm = src.join("node_modules");
        std::fs::create_dir_all(nm.join("lodash")).unwrap();
        std::fs::write(nm.join("lodash/index.js"), "module.exports={}").unwrap();
        std::fs::create_dir_all(nm.join("react")).unwrap();
        std::fs::write(nm.join("react/index.js"), "module.exports={}").unwrap();
        std::fs::create_dir_all(nm.join("express")).unwrap();
        std::fs::write(nm.join("express/index.js"), "module.exports={}").unwrap();
        std::fs::create_dir_all(nm.join("axios")).unwrap();
        std::fs::create_dir_all(nm.join("vue")).unwrap();

        std::fs::write(src.join(".cow.json"), r#"{"pre_clone":{"symlink_threshold":3}}"#).unwrap();

        let out = env.cow()
            .args(["create", "test", "--source", src.to_str().unwrap(), "--no-branch"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let stdout = String::from_utf8_lossy(&out);
        assert!(stdout.contains("node_modules"), "warning should name node_modules");
        assert!(stdout.contains("per-package"), "should say per-package");
        assert!(stdout.contains("cow materialise"), "warning should mention materialise");

        let pasture = ws_path(&env.home, &source, "test");
        // node_modules/ itself should be a real dir (not a symlink).
        assert!(!is_symlink(&pasture.join("node_modules")), "node_modules should NOT be a whole-dir symlink");
        assert!(pasture.join("node_modules").is_dir(), "node_modules should be a real dir");
        // Each package entry should be a symlink.
        assert!(is_symlink(&pasture.join("node_modules/lodash")), "lodash should be a symlink");
        assert!(is_symlink(&pasture.join("node_modules/react")), "react should be a symlink");

        // State records in linked_dirs, not symlinked_dirs.
        let state = read_state(&env.home);
        let linked = &state["pastures"][0]["linked_dirs"];
        assert!(linked.as_array().unwrap().iter().any(|v| v.as_str() == Some("node_modules")));
        let symlinked = &state["pastures"][0]["symlinked_dirs"];
        assert!(symlinked.as_array().unwrap().is_empty(), "symlinked_dirs should be empty");
    }

    #[test]
    fn create_per_package_new_install_is_local() {
        // New packages written to the pasture's node_modules go local (do not
        // appear in the source repo's node_modules).
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        let nm = src.join("node_modules");
        std::fs::create_dir_all(nm.join("existing-pkg")).unwrap();
        std::fs::create_dir_all(nm.join("another-pkg")).unwrap();
        std::fs::create_dir_all(nm.join("third-pkg")).unwrap();
        std::fs::create_dir_all(nm.join("fourth-pkg")).unwrap();

        std::fs::write(src.join(".cow.json"), r#"{"pre_clone":{"symlink_threshold":3}}"#).unwrap();

        env.cow()
            .args(["create", "test", "--source", src.to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        let pasture = ws_path(&env.home, &source, "test");

        // Simulate `npm install new-package` by writing into the pasture's node_modules.
        let new_pkg = pasture.join("node_modules/new-package");
        std::fs::create_dir_all(&new_pkg).unwrap();
        std::fs::write(new_pkg.join("index.js"), "module.exports={}").unwrap();

        // The new package should NOT appear in the source.
        assert!(!src.join("node_modules/new-package").exists(),
            "new package should not appear in source node_modules");
        // Existing packages should still resolve through symlinks.
        assert!(pasture.join("node_modules/existing-pkg").exists(),
            "existing package should still be accessible");
    }

    // ─── cow-jk9a: pnpm virtual store — relative symlinks stay within pasture ──
    #[test]
    fn create_pnpm_virtual_store_symlinks_stay_local() {
        // pnpm's node_modules uses a virtual store: top-level entries are relative
        // symlinks into .pnpm/. After cow create, those symlinks should resolve
        // within the pasture (not back to the source), so tools like Turbopack
        // don't see cross-boundary symlinks.
        //
        // To trigger the dep-dir per-package symlink path, node_modules must be
        // the detected candidate (not .pnpm itself). We keep .pnpm shallow (2
        // empty dirs → total=2 ≤ threshold=3, not added) while node_modules
        // total (1 + 2 + 2 symlinks = 5 > 3) crosses the threshold.
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        let nm = src.join("node_modules");
        // .pnpm has only empty package dirs — shallow enough to stay below threshold.
        std::fs::create_dir_all(nm.join(".pnpm/next@15.0.0")).unwrap();
        std::fs::create_dir_all(nm.join(".pnpm/react@18.0.0")).unwrap();
        // Top-level entries are relative symlinks (as pnpm creates them).
        std::os::unix::fs::symlink(".pnpm/next@15.0.0", nm.join("next")).unwrap();
        std::os::unix::fs::symlink(".pnpm/react@18.0.0", nm.join("react")).unwrap();

        std::fs::write(src.join(".cow.json"), r#"{"pre_clone":{"symlink_threshold":3}}"#).unwrap();

        env.cow()
            .args(["create", "test", "--source", src.to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        let pasture = ws_path(&env.home, &source, "test");
        let p_nm = pasture.join("node_modules");

        // .pnpm itself is a whole-dir symlink to the source store.
        assert!(is_symlink(&p_nm.join(".pnpm")), ".pnpm should be a whole-dir symlink");

        // Top-level package entries should be relative symlinks that resolve
        // within the pasture — NOT absolute paths back to the source.
        let next_link = std::fs::read_link(p_nm.join("next")).expect("next should be a symlink");
        assert!(next_link.is_relative(), "next symlink target should be relative, got: {:?}", next_link);
        assert!(!next_link.to_string_lossy().contains(src.to_str().unwrap()),
            "next symlink should not reference source path, got: {:?}", next_link);
        // The relative target must resolve via the local .pnpm symlink.
        assert!(p_nm.join("next").exists(), "next should resolve to a real path via .pnpm");
    }

    #[test]
    fn create_no_symlink_skips_detection() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        let nm = src.join("node_modules");
        std::fs::create_dir_all(&nm).unwrap();
        for i in 0..5 { std::fs::write(nm.join(format!("f{}.js", i)), "x").unwrap(); }
        std::fs::write(src.join(".cow.json"), r#"{"pre_clone":{"symlink_threshold":3}}"#).unwrap();

        let out = env.cow()
            .args(["create", "test", "--source", src.to_str().unwrap(), "--no-branch", "--no-symlink"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let stdout = String::from_utf8_lossy(&out);
        assert!(!stdout.contains("symlinked"), "no warning should appear with --no-symlink");

        let pasture = ws_path(&env.home, &source, "test");
        assert!(!is_symlink(&pasture.join("node_modules")), "node_modules should be cloned, not symlinked");
    }

    #[test]
    fn materialise_replaces_symlink_with_clone() {
        // Whole-dir symlink (fixtures/) should be replaced with a real clone.
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        let fixtures = src.join("fixtures");
        std::fs::create_dir_all(&fixtures).unwrap();
        for i in 0..5 { std::fs::write(fixtures.join(format!("f{}.json", i)), "{}").unwrap(); }
        std::fs::write(src.join(".cow.json"), r#"{"pre_clone":{"symlink_threshold":3}}"#).unwrap();

        env.cow()
            .args(["create", "test", "--source", src.to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        let pasture = ws_path(&env.home, &source, "test");
        assert!(is_symlink(&pasture.join("fixtures")), "should start as a whole-dir symlink");

        env.cow()
            .args(["materialise", &scoped(&source, "test")])
            .assert()
            .success()
            .stdout(predicate::str::contains("done"));

        assert!(!is_symlink(&pasture.join("fixtures")), "should be real dir after materialise");
        assert!(pasture.join("fixtures").is_dir(), "fixtures should still exist as a dir");

        let state = read_state(&env.home);
        let dirs = &state["pastures"][0]["symlinked_dirs"];
        assert!(dirs.as_array().unwrap().is_empty(), "symlinked_dirs should be empty after materialise");
    }

    #[test]
    fn materialise_per_package_replaces_entries() {
        // Per-package symlinks (linked_dirs): materialise should clonefile each
        // top-level entry so it becomes a real copy in the pasture.
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        let nm = src.join("node_modules");
        std::fs::create_dir_all(nm.join("lodash")).unwrap();
        std::fs::write(nm.join("lodash/index.js"), "module.exports={}").unwrap();
        std::fs::create_dir_all(nm.join("react")).unwrap();
        std::fs::write(nm.join("react/index.js"), "module.exports={}").unwrap();
        std::fs::create_dir_all(nm.join("express")).unwrap();
        std::fs::create_dir_all(nm.join("axios")).unwrap();

        std::fs::write(src.join(".cow.json"), r#"{"pre_clone":{"symlink_threshold":3}}"#).unwrap();

        env.cow()
            .args(["create", "test", "--source", src.to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        let pasture = ws_path(&env.home, &source, "test");
        assert!(is_symlink(&pasture.join("node_modules/lodash")), "lodash should start as a symlink");

        env.cow()
            .args(["materialise", &scoped(&source, "test")])
            .assert()
            .success();

        // After materialise, entries should be real dirs, not symlinks.
        assert!(!is_symlink(&pasture.join("node_modules/lodash")), "lodash should no longer be a symlink");
        assert!(pasture.join("node_modules/lodash").is_dir(), "lodash should still exist");
        assert!(pasture.join("node_modules/lodash/index.js").exists(), "lodash/index.js should exist");

        let state = read_state(&env.home);
        let linked = &state["pastures"][0]["linked_dirs"];
        assert!(linked.as_array().unwrap().is_empty(), "linked_dirs should be empty after materialise");
    }

    #[test]
    fn materialise_is_idempotent_when_already_cloned() {
        // Uses a non-dep dir (fixtures/) so whole-dir symlink semantics apply.
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        let fixtures = src.join("fixtures");
        std::fs::create_dir_all(&fixtures).unwrap();
        for i in 0..5 { std::fs::write(fixtures.join(format!("f{}.json", i)), "{}").unwrap(); }
        std::fs::write(src.join(".cow.json"), r#"{"pre_clone":{"symlink_threshold":3}}"#).unwrap();

        env.cow()
            .args(["create", "test", "--source", src.to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        // materialise twice — second run should succeed gracefully
        env.cow().args(["materialise", &scoped(&source, "test")]).assert().success();
        env.cow().args(["materialise", &scoped(&source, "test")]).assert().success()
            .stdout(predicate::str::contains("no symlinked"));
    }

    // ─── cow-re21: --worktree mode ──────────────────────────────────────────────

    #[test]
    fn create_worktree_creates_linked_worktree() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        env.cow()
            .args(["create", "feat", "--source", src.to_str().unwrap(), "--worktree"])
            .assert()
            .success()
            .stdout(predicate::str::contains("worktree"));

        let pasture = ws_path(&env.home, &source, "feat");
        assert!(pasture.exists(), "pasture directory should exist");
        // A linked worktree has .git as a FILE, not a directory
        assert!(pasture.join(".git").is_file(), ".git should be a file in a linked worktree");

        let state = read_state(&env.home);
        assert_eq!(state["pastures"][0]["is_worktree"], serde_json::Value::Bool(true));
    }

    #[test]
    fn create_worktree_fails_for_non_git_source() {
        let env = Env::new();
        let tmp = tempfile::TempDir::new().unwrap();
        // Fake a jj primary workspace: .jj/repo/ present
        std::fs::create_dir_all(tmp.path().join(".jj/repo")).unwrap();

        env.cow()
            .args(["create", "test", "--source", tmp.path().to_str().unwrap(), "--worktree", "--no-branch"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("only supported for git"));
    }

    #[test]
    fn create_worktree_duplicate_branch_fails() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        // First worktree creates branch "feat"
        env.cow()
            .args(["create", "feat", "--source", src.to_str().unwrap(), "--worktree"])
            .assert()
            .success();

        // Second worktree trying the same branch should fail
        env.cow()
            .args(["create", "feat2", "--source", src.to_str().unwrap(),
                   "--worktree", "--branch", "feat"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("already checked out"));
    }

    #[test]
    fn remove_worktree_pasture_cleans_up_link() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        env.cow()
            .args(["create", "feat", "--source", src.to_str().unwrap(), "--worktree"])
            .assert()
            .success();

        let pasture = ws_path(&env.home, &source, "feat");
        assert!(pasture.exists());

        env.cow()
            .args(["remove", &scoped(&source, "feat"), "--yes"])
            .assert()
            .success();

        assert!(!pasture.exists(), "pasture directory should be gone after remove");

        // git worktree list should no longer show the removed worktree
        let out = std::process::Command::new("git")
            .args(["worktree", "list"])
            .current_dir(src)
            .output()
            .unwrap();
        let list = String::from_utf8_lossy(&out.stdout);
        assert!(!list.contains(pasture.to_str().unwrap()), "worktree should be removed from source");
    }

    #[test]
    fn create_worktree_commits_visible_across_pastures() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        // Create two worktrees
        env.cow()
            .args(["create", "wt-a", "--source", src.to_str().unwrap(), "--worktree"])
            .assert()
            .success();
        env.cow()
            .args(["create", "wt-b", "--source", src.to_str().unwrap(), "--worktree"])
            .assert()
            .success();

        let wt_a = ws_path(&env.home, &source, "wt-a");
        let wt_b = ws_path(&env.home, &source, "wt-b");

        // Make a commit in wt-a
        std::fs::write(wt_a.join("new.txt"), "new content").unwrap();
        git(&wt_a, &["config", "user.email", "test@cow.test"]);
        git(&wt_a, &["config", "user.name", "cow-test"]);
        git(&wt_a, &["config", "commit.gpgsign", "false"]);
        git(&wt_a, &["add", "new.txt"]);
        git(&wt_a, &["commit", "-m", "commit-in-wt-a"]);

        // The commit should be visible from wt-b without any fetch
        // (worktrees share .git/objects/)
        let log = std::process::Command::new("git")
            .args(["log", "--oneline", "--all"])
            .current_dir(&wt_b)
            .output()
            .unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        assert!(log_str.contains("commit-in-wt-a"), "commit from wt-a should be visible in wt-b");
    }

    // ─── cow-pfq7: cow fetch-from ───────────────────────────────────────────────

    #[test]
    fn fetch_from_fetches_refs_from_named_pasture() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        // Create two pastures (regular clones)
        env.cow()
            .args(["create", "pa", "--source", src.to_str().unwrap(), "--no-branch"])
            .assert()
            .success();
        env.cow()
            .args(["create", "pb", "--source", src.to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        let pa = ws_path(&env.home, &source, "pa");
        let pb = ws_path(&env.home, &source, "pb");

        // Make a commit in pa that pb doesn't know about
        std::fs::write(pa.join("new.txt"), "from pa").unwrap();
        git(&pa, &["config", "user.email", "test@cow.test"]);
        git(&pa, &["config", "user.name", "cow-test"]);
        git(&pa, &["config", "commit.gpgsign", "false"]);
        git(&pa, &["add", "new.txt"]);
        git(&pa, &["commit", "-m", "commit-in-pa"]);

        let pa_name = scoped(&source, "pa");
        let pb_name = scoped(&source, "pb");

        // fetch-from pa into pb
        env.cow()
            .args(["fetch-from", &pa_name, "--name", &pb_name])
            .assert()
            .success()
            .stdout(predicate::str::contains("refs/cow/"));

        // Verify the ref exists in pb
        let refs_out = std::process::Command::new("git")
            .args(["show-ref"])
            .current_dir(&pb)
            .output()
            .unwrap();
        let refs_str = String::from_utf8_lossy(&refs_out.stdout);
        assert!(refs_str.contains("refs/cow/"), "fetched refs should appear in pb");
    }

    #[test]
    fn fetch_from_unknown_pasture_fails() {
        let env = Env::new();
        let source = make_git_repo();
        let src = source.path();

        env.cow()
            .args(["create", "pa", "--source", src.to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        env.cow()
            .args(["fetch-from", "does-not-exist", "--name", &scoped(&source, "pa")])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }

    #[test]
    fn fetch_from_cross_source_requires_force() {
        let env = Env::new();
        let source_a = make_git_repo();
        let source_b = make_git_repo();

        env.cow()
            .args(["create", "pa", "--source", source_a.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();
        env.cow()
            .args(["create", "pb", "--source", source_b.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();

        let pa_name = scoped(&source_a, "pa");
        let pb_name = scoped(&source_b, "pb");

        // Without --force: should fail
        env.cow()
            .args(["fetch-from", &pa_name, "--name", &pb_name])
            .assert()
            .failure()
            .stderr(predicate::str::contains("different sources"));
    }
    // ─── helpers ───────────────────────────────────────────────────────────────

    /// Create a git repo with a bare "origin" remote and push main to it.
    fn make_git_repo_with_remote() -> (TempDir, TempDir) {
        let source = make_git_repo();
        let bare = TempDir::new().expect("bare repo");
        git(bare.path(), &["init", "--bare", "-b", "main"]);
        git(source.path(), &["remote", "add", "origin", bare.path().to_str().unwrap()]);
        git(source.path(), &["push", "origin", "main"]);
        (source, bare)
    }

    // ─── gc ────────────────────────────────────────────────────────────────────

    #[test]
    fn gc_no_candidates_without_remote() {
        let env = Env::new();
        let source = make_git_repo();
        env.cow()
            .args(["create", "gc-noop", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();
        env.cow()
            .args(["gc", "--yes"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No pastures with branches pushed to origin."));
    }

    #[test]
    fn gc_removes_pasture_with_pushed_branch() {
        let env = Env::new();
        let (source, _bare) = make_git_repo_with_remote();
        env.cow()
            .args(["create", "gc-pushed", "--source", source.path().to_str().unwrap(), "--branch", "main"])
            .assert()
            .success();
        let path = ws_path(&env.home, &source, "gc-pushed");
        env.cow()
            .args(["gc", "--yes"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed pasture"));
        assert!(!path.exists(), "pasture should be removed after gc");
    }

    #[test]
    fn gc_dry_run_does_not_remove() {
        let env = Env::new();
        let (source, _bare) = make_git_repo_with_remote();
        env.cow()
            .args(["create", "gc-dry", "--source", source.path().to_str().unwrap(), "--branch", "main"])
            .assert()
            .success();
        let path = ws_path(&env.home, &source, "gc-dry");
        env.cow()
            .args(["gc", "--dry-run"])
            .assert()
            .success()
            .stdout(predicate::str::contains("dry-run"));
        assert!(path.exists(), "dry-run must not remove the pasture");
    }

    #[test]
    fn gc_merged_skips_unmerged_branch() {
        let env = Env::new();
        let (source, _bare) = make_git_repo_with_remote();
        // Create a feature branch, push it, but do NOT merge it.
        git(source.path(), &["checkout", "-b", "feature-unmerged"]);
        std::fs::write(source.path().join("feat.txt"), "feat").unwrap();
        git(source.path(), &["add", "feat.txt"]);
        git(source.path(), &["commit", "-m", "feat"]);
        git(source.path(), &["push", "origin", "feature-unmerged"]);
        env.cow()
            .args(["create", "gc-unmerged", "--source", source.path().to_str().unwrap(), "--branch", "feature-unmerged"])
            .assert()
            .success();
        env.cow()
            .args(["gc", "--merged", "--yes"])
            .assert()
            .success()
            .stdout(predicate::str::contains("No pastures with branches merged to origin."));
    }

    #[test]
    fn gc_merged_removes_merged_branch() {
        let env = Env::new();
        let (source, _bare) = make_git_repo_with_remote();
        // Create feature branch, push it.
        git(source.path(), &["checkout", "-b", "feature-merged"]);
        std::fs::write(source.path().join("feat2.txt"), "feat2").unwrap();
        git(source.path(), &["add", "feat2.txt"]);
        git(source.path(), &["commit", "-m", "feat2"]);
        git(source.path(), &["push", "origin", "feature-merged"]);
        // Create pasture on the feature branch.
        env.cow()
            .args(["create", "gc-merged-ws", "--source", source.path().to_str().unwrap(), "--branch", "feature-merged"])
            .assert()
            .success();
        let path = ws_path(&env.home, &source, "gc-merged-ws");
        // Merge feature into main and push.
        git(source.path(), &["checkout", "main"]);
        git(source.path(), &["merge", "--no-ff", "feature-merged", "-m", "merge feat2"]);
        git(source.path(), &["push", "origin", "main"]);
        env.cow()
            .args(["gc", "--merged", "--yes"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed pasture"));
        assert!(!path.exists(), "merged pasture should be removed");
    }

    #[test]
    fn gc_shows_dirty_warning() {
        let env = Env::new();
        let (source, _bare) = make_git_repo_with_remote();
        env.cow()
            .args(["create", "gc-dirty-warn", "--source", source.path().to_str().unwrap(), "--branch", "main"])
            .assert()
            .success();
        let path = ws_path(&env.home, &source, "gc-dirty-warn");
        std::fs::write(path.join("dirty.txt"), "wip").unwrap();
        // --yes without --force: warns about dirty state, then removes.
        env.cow()
            .args(["gc", "--yes"])
            .assert()
            .success()
            .stderr(predicate::str::contains("uncommitted changes"));
    }

    #[test]
    fn gc_force_removes_dirty_pasture() {
        let env = Env::new();
        let (source, _bare) = make_git_repo_with_remote();
        env.cow()
            .args(["create", "gc-dirty-force", "--source", source.path().to_str().unwrap(), "--branch", "main"])
            .assert()
            .success();
        let path = ws_path(&env.home, &source, "gc-dirty-force");
        std::fs::write(path.join("dirty.txt"), "wip").unwrap();
        env.cow()
            .args(["gc", "--force"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed pasture"));
        assert!(!path.exists(), "--force should remove dirty pasture without prompt");
    }

    // ─── stats ─────────────────────────────────────────────────────────────────

    #[test]
    fn stats_no_pastures() {
        let env = Env::new();
        env.cow()
            .arg("stats")
            .assert()
            .success()
            .stdout(predicate::str::contains("No pastures found."));
    }

    #[test]
    fn stats_with_pastures() {
        let env = Env::new();
        let source = make_git_repo();
        env.cow()
            .args(["create", "stats-ws", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();
        env.cow()
            .arg("stats")
            .assert()
            .success()
            .stdout(predicate::str::contains("Source"))
            .stdout(predicate::str::contains("Pastures"))
            .stdout(predicate::str::contains("On disk"));
    }

    // ─── fetch-from cwd detection ──────────────────────────────────────────────

    #[test]
    fn fetch_from_cwd_detection() {
        let env = Env::new();
        let source = make_git_repo();
        env.cow()
            .args(["create", "ff-dest", "--source", source.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();
        env.cow()
            .args(["create", "ff-src", "--source", source.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();
        let dest_path = ws_path(&env.home, &source, "ff-dest");
        let ff_src = scoped(&source, "ff-src");
        // Run without --name — should detect destination from CWD.
        env.cow()
            .args(["fetch-from", &ff_src])
            .current_dir(&dest_path)
            .assert()
            .success();
    }

    #[test]
    fn fetch_from_cwd_not_in_pasture_fails() {
        let env = Env::new();
        let source = make_git_repo();
        env.cow()
            .args(["create", "ff-src2", "--source", source.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();
        let ff_src2 = scoped(&source, "ff-src2");
        // Run from outside any pasture with no --name — should fail helpfully.
        env.cow()
            .args(["fetch-from", &ff_src2])
            .current_dir(source.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("Not inside a cow pasture"));
    }

    #[test]
    fn gc_fetch_flag_runs_without_error() {
        let env = Env::new();
        let (source, _bare) = make_git_repo_with_remote();
        env.cow()
            .args(["create", "gc-fetch-ws", "--source", source.path().to_str().unwrap(), "--branch", "main"])
            .assert()
            .success();
        // --fetch runs git fetch origin on each source repo before checking candidates.
        env.cow()
            .args(["gc", "--fetch", "--yes"])
            .assert()
            .success();
    }

    #[test]
    fn gc_merged_with_remote_head_set() {
        // Exercises default_branch() taking the non-fallback path (refs/remotes/origin/HEAD set).
        let env = Env::new();
        let (source, _bare) = make_git_repo_with_remote();
        // Explicitly set origin/HEAD so git symbolic-ref returns a result.
        git(source.path(), &["remote", "set-head", "origin", "main"]);
        // Create feature branch, push, merge, push main.
        git(source.path(), &["checkout", "-b", "feature-head-test"]);
        std::fs::write(source.path().join("fht.txt"), "fht").unwrap();
        git(source.path(), &["add", "fht.txt"]);
        git(source.path(), &["commit", "-m", "fht"]);
        git(source.path(), &["push", "origin", "feature-head-test"]);
        env.cow()
            .args(["create", "gc-head-ws", "--source", source.path().to_str().unwrap(), "--branch", "feature-head-test"])
            .assert()
            .success();
        let path = ws_path(&env.home, &source, "gc-head-ws");
        git(source.path(), &["checkout", "main"]);
        git(source.path(), &["merge", "--no-ff", "feature-head-test", "-m", "merge fht"]);
        git(source.path(), &["push", "origin", "main"]);
        env.cow()
            .args(["gc", "--merged", "--yes"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Removed pasture"));
        assert!(!path.exists());
    }

    #[test]
    fn fetch_from_jj_destination_fails() {
        // Exercises fetch_from.rs line 36: bail when destination pasture is not git.
        let env = Env::new();
        let jj_source = make_jj_repo(&env.home);
        let git_source = make_git_repo();
        env.cow()
            .args(["create", "ff-jj-dest", "--source", jj_source.path().to_str().unwrap()])
            .assert()
            .success();
        env.cow()
            .args(["create", "ff-git-src", "--source", git_source.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();
        let jj_name = format!("{}/ff-jj-dest", jj_source.path().file_name().unwrap().to_str().unwrap());
        let git_name = scoped(&git_source, "ff-git-src");
        env.cow()
            .args(["fetch-from", &git_name, "--name", &jj_name])
            .assert()
            .failure()
            .stderr(predicate::str::contains("only supports git pastures"));
    }

    #[test]
    fn fetch_from_jj_source_fails() {
        // Exercises fetch_from.rs line 46: bail when source (from) pasture is not git.
        let env = Env::new();
        let jj_source = make_jj_repo(&env.home);
        let git_source = make_git_repo();
        env.cow()
            .args(["create", "ff-jj-src", "--source", jj_source.path().to_str().unwrap()])
            .assert()
            .success();
        env.cow()
            .args(["create", "ff-git-dest2", "--source", git_source.path().to_str().unwrap(), "--no-branch"])
            .assert()
            .success();
        let jj_name = format!("{}/ff-jj-src", jj_source.path().file_name().unwrap().to_str().unwrap());
        let git_name = scoped(&git_source, "ff-git-dest2");
        env.cow()
            .args(["fetch-from", &jj_name, "--name", &git_name])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not a git pasture"));
    }

    // ─── test helpers for materialise and migrate ──────────────────────────────

    /// Edit a specific pasture entry in state.json.
    fn patch_pasture_state(home: &std::path::Path, name: &str, f: impl Fn(&mut serde_json::Value)) {
        let state_path = home.join(".cow/state.json");
        let raw = std::fs::read_to_string(&state_path).unwrap();
        let mut state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        for p in state["pastures"].as_array_mut().unwrap().iter_mut() {
            if p["name"].as_str() == Some(name) {
                f(p);
                break;
            }
        }
        std::fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap()).unwrap();
    }

    /// Remove a pasture entry from state.json by name.
    fn remove_pasture_from_state(home: &std::path::Path, name: &str) {
        let state_path = home.join(".cow/state.json");
        let raw = std::fs::read_to_string(&state_path).unwrap();
        let mut state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let pastures = state["pastures"].as_array_mut().unwrap();
        pastures.retain(|p| p["name"].as_str() != Some(name));
        std::fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap()).unwrap();
    }

    // ─── materialise ───────────────────────────────────────────────────────────

    #[test]
    fn materialise_no_symlinked_dirs() {
        let env = Env::new();
        let source = make_git_repo();
        env.cow()
            .args(["create", "mat-empty", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();
        let name = scoped(&source, "mat-empty");
        env.cow()
            .args(["materialise", &name])
            .assert()
            .success()
            .stdout(predicate::str::contains("no symlinked directories"));
    }

    #[test]
    fn materialise_whole_dir_symlink() {
        let env = Env::new();
        let source = make_git_repo();
        // Add a vendor dir to source.
        let vendor_src = source.path().join("vendor");
        std::fs::create_dir(&vendor_src).unwrap();
        std::fs::write(vendor_src.join("lib.txt"), "lib").unwrap();
        git(source.path(), &["add", "."]);
        git(source.path(), &["commit", "-m", "add vendor"]);

        env.cow()
            .args(["create", "mat-whole", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();
        let pasture = ws_path(&env.home, &source, "mat-whole");
        let name = scoped(&source, "mat-whole");

        // Replace the cloned vendor dir with a symlink to source's vendor.
        std::fs::remove_dir_all(pasture.join("vendor")).unwrap();
        std::os::unix::fs::symlink(&vendor_src, pasture.join("vendor")).unwrap();
        assert!(is_symlink(&pasture.join("vendor")));

        // Patch state to record the symlink.
        patch_pasture_state(&env.home, &name, |p| {
            p["symlinked_dirs"] = serde_json::json!(["vendor"]);
        });

        env.cow().args(["materialise", &name]).assert().success();

        assert!(!is_symlink(&pasture.join("vendor")), "symlink should be replaced with real dir");
        assert!(pasture.join("vendor").is_dir());
        assert!(pasture.join("vendor/lib.txt").exists());

        // State should clear symlinked_dirs.
        let state = read_state(&env.home);
        let entry = state["pastures"].as_array().unwrap()
            .iter().find(|p| p["name"].as_str() == Some(&name)).unwrap();
        assert_eq!(entry["symlinked_dirs"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn materialise_whole_dir_already_real() {
        let env = Env::new();
        let source = make_git_repo();
        std::fs::create_dir(source.path().join("vendor")).unwrap();
        std::fs::write(source.path().join("vendor/lib.txt"), "lib").unwrap();
        git(source.path(), &["add", "."]);
        git(source.path(), &["commit", "-m", "add vendor"]);

        env.cow()
            .args(["create", "mat-real", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();
        let name = scoped(&source, "mat-real");

        // Pasture already has a real vendor dir from the clone.
        // Patch state to say it's symlinked.
        patch_pasture_state(&env.home, &name, |p| {
            p["symlinked_dirs"] = serde_json::json!(["vendor"]);
        });

        env.cow()
            .args(["materialise", &name])
            .assert()
            .success()
            .stdout(predicate::str::contains("already a real directory"));
    }

    #[test]
    fn materialise_whole_dir_dst_gone() {
        // dst doesn't exist at all — should silently remove from list.
        let env = Env::new();
        let source = make_git_repo();
        env.cow()
            .args(["create", "mat-gone", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();
        let name = scoped(&source, "mat-gone");

        patch_pasture_state(&env.home, &name, |p| {
            p["symlinked_dirs"] = serde_json::json!(["vendor"]);
        });

        env.cow().args(["materialise", &name]).assert().success();

        let state = read_state(&env.home);
        let entry = state["pastures"].as_array().unwrap()
            .iter().find(|p| p["name"].as_str() == Some(&name)).unwrap();
        assert_eq!(entry["symlinked_dirs"].as_array().unwrap().len(), 0,
            "ghost entry should be cleared from symlinked_dirs");
    }

    #[test]
    fn materialise_whole_dir_src_missing() {
        // Symlink exists in pasture but source dir was deleted.
        let env = Env::new();
        let source = make_git_repo();
        let vendor_src = source.path().join("vendor");
        std::fs::create_dir(&vendor_src).unwrap();
        std::fs::write(vendor_src.join("lib.txt"), "lib").unwrap();
        git(source.path(), &["add", "."]);
        git(source.path(), &["commit", "-m", "add vendor"]);

        env.cow()
            .args(["create", "mat-srcgone", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();
        let pasture = ws_path(&env.home, &source, "mat-srcgone");
        let name = scoped(&source, "mat-srcgone");

        std::fs::remove_dir_all(pasture.join("vendor")).unwrap();
        std::os::unix::fs::symlink(&vendor_src, pasture.join("vendor")).unwrap();
        std::fs::remove_dir_all(&vendor_src).unwrap();

        patch_pasture_state(&env.home, &name, |p| {
            p["symlinked_dirs"] = serde_json::json!(["vendor"]);
        });

        env.cow()
            .args(["materialise", &name])
            .assert()
            .success()
            .stderr(predicate::str::contains("no longer exists"));
    }

    #[test]
    fn materialise_per_package_symlinks() {
        let env = Env::new();
        let source = make_git_repo();
        let nm_src = source.path().join("node_modules");
        std::fs::create_dir(&nm_src).unwrap();
        std::fs::create_dir(nm_src.join("pkg-a")).unwrap();
        std::fs::write(nm_src.join("pkg-a/index.js"), "a").unwrap();
        std::fs::create_dir(nm_src.join("pkg-b")).unwrap();
        std::fs::write(nm_src.join("pkg-b/index.js"), "b").unwrap();

        env.cow()
            .args(["create", "mat-pkg", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();
        let pasture = ws_path(&env.home, &source, "mat-pkg");
        let name = scoped(&source, "mat-pkg");

        // Replace cloned node_modules with per-package symlinks.
        std::fs::remove_dir_all(pasture.join("node_modules")).unwrap();
        std::fs::create_dir(pasture.join("node_modules")).unwrap();
        std::os::unix::fs::symlink(nm_src.join("pkg-a"), pasture.join("node_modules/pkg-a")).unwrap();
        std::os::unix::fs::symlink(nm_src.join("pkg-b"), pasture.join("node_modules/pkg-b")).unwrap();

        patch_pasture_state(&env.home, &name, |p| {
            p["linked_dirs"] = serde_json::json!(["node_modules"]);
        });

        env.cow().args(["materialise", &name]).assert().success();

        assert!(!is_symlink(&pasture.join("node_modules/pkg-a")));
        assert!(pasture.join("node_modules/pkg-a").is_dir());
        assert!(pasture.join("node_modules/pkg-a/index.js").exists());
        assert!(!is_symlink(&pasture.join("node_modules/pkg-b")));
        assert!(pasture.join("node_modules/pkg-b").is_dir());
    }

    #[test]
    fn materialise_per_package_new_package_in_source() {
        // A package exists in source but not in the pasture — should be cloned in.
        let env = Env::new();
        let source = make_git_repo();
        let nm_src = source.path().join("node_modules");
        std::fs::create_dir(&nm_src).unwrap();
        std::fs::create_dir(nm_src.join("pkg-a")).unwrap();
        std::fs::write(nm_src.join("pkg-a/index.js"), "a").unwrap();

        env.cow()
            .args(["create", "mat-newpkg", "--source", source.path().to_str().unwrap()])
            .assert()
            .success();
        let pasture = ws_path(&env.home, &source, "mat-newpkg");
        let name = scoped(&source, "mat-newpkg");

        // Create pasture node_modules with only pkg-a (as a real dir).
        // Then add pkg-b to source after the pasture was created.
        std::fs::create_dir(nm_src.join("pkg-b")).unwrap();
        std::fs::write(nm_src.join("pkg-b/index.js"), "b").unwrap();

        // Pasture has an empty node_modules (no pkg-b yet).
        std::fs::remove_dir_all(pasture.join("node_modules")).unwrap();
        std::fs::create_dir(pasture.join("node_modules")).unwrap();
        // pkg-a as a symlink so the per-package loop runs.
        std::os::unix::fs::symlink(nm_src.join("pkg-a"), pasture.join("node_modules/pkg-a")).unwrap();

        patch_pasture_state(&env.home, &name, |p| {
            p["linked_dirs"] = serde_json::json!(["node_modules"]);
        });

        env.cow().args(["materialise", &name]).assert().success()
            .stdout(predicate::str::contains("new"));

        // pkg-b should have been cloned into pasture.
        assert!(pasture.join("node_modules/pkg-b").exists());
        assert!(pasture.join("node_modules/pkg-b/index.js").exists());
    }

    // ─── migrate ───────────────────────────────────────────────────────────────

    #[test]
    fn migrate_no_candidates() {
        let env = Env::new();
        let source = make_git_repo();
        env.cow()
            .args(["migrate", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("No candidates found"));
    }

    #[test]
    fn migrate_shows_candidates_without_all() {
        let env = Env::new();
        let source = make_git_repo();
        let wt_base = tempfile::TempDir::new().unwrap();
        let wt_path = wt_base.path().join("feature-wt");
        git(source.path(), &["worktree", "add", wt_path.to_str().unwrap(), "-b", "feature-wt"]);
        env.cow()
            .args(["migrate", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("Found"))
            .stdout(predicate::str::contains("--all"));
    }

    #[test]
    fn migrate_git_worktree_all() {
        let env = Env::new();
        let source = make_git_repo();
        let wt_base = tempfile::TempDir::new().unwrap();
        let wt_path = wt_base.path().join("migrate-branch");
        git(source.path(), &["worktree", "add", wt_path.to_str().unwrap(), "-b", "migrate-branch"]);

        env.cow()
            .args(["migrate", "--source", source.path().to_str().unwrap(), "--all"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Migrated"));

        // The old worktree directory should have been removed.
        assert!(!wt_path.exists(), "old worktree should be removed after migrate");

        // A new pasture should exist in the cow pasture dir.
        let state = read_state(&env.home);
        let pastures = state["pastures"].as_array().unwrap();
        assert!(pastures.iter().any(|p| p["name"].as_str() == Some("migrate-branch")),
            "migrated pasture should appear in state");
    }

    #[test]
    fn migrate_dry_run() {
        let env = Env::new();
        let source = make_git_repo();
        let wt_base = tempfile::TempDir::new().unwrap();
        let wt_path = wt_base.path().join("dry-branch");
        git(source.path(), &["worktree", "add", wt_path.to_str().unwrap(), "-b", "dry-branch"]);

        env.cow()
            .args(["migrate", "--source", source.path().to_str().unwrap(), "--all", "--dry-run"])
            .assert()
            .success()
            .stdout(predicate::str::contains("[dry-run]"));

        // Worktree should still exist after dry-run — nothing was migrated.
        assert!(wt_path.exists());
    }

    #[test]
    fn migrate_skips_dirty_without_force() {
        let env = Env::new();
        let source = make_git_repo();
        let wt_base = tempfile::TempDir::new().unwrap();
        let wt_path = wt_base.path().join("dirty-wt");
        git(source.path(), &["worktree", "add", wt_path.to_str().unwrap(), "-b", "dirty-wt"]);
        std::fs::write(wt_path.join("untracked.txt"), "wip").unwrap();

        env.cow()
            .args(["migrate", "--source", source.path().to_str().unwrap(), "--all"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Skipping"));

        // Nothing migrated — worktree still at its original location.
        assert!(wt_path.exists());
    }

    #[test]
    fn migrate_force_migrates_dirty() {
        let env = Env::new();
        let source = make_git_repo();
        let wt_base = tempfile::TempDir::new().unwrap();
        let wt_path = wt_base.path().join("force-wt");
        git(source.path(), &["worktree", "add", wt_path.to_str().unwrap(), "-b", "force-wt"]);
        std::fs::write(wt_path.join("untracked.txt"), "wip").unwrap();

        env.cow()
            .args(["migrate", "--source", source.path().to_str().unwrap(), "--all", "--force"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Migrated"));

        let state = read_state(&env.home);
        assert!(state["pastures"].as_array().unwrap().iter().any(|p| p["name"].as_str() == Some("force-wt")));
    }

    #[test]
    fn migrate_orphaned_directory() {
        let env = Env::new();
        let source = make_git_repo();
        let pasture_dir = env.home.join(".cow/pastures");

        // Create a pasture directly in the flat pastures dir (not scoped).
        env.cow()
            .args([
                "create", "orphan-test",
                "--source", source.path().to_str().unwrap(),
                "--dir", pasture_dir.to_str().unwrap(),
            ])
            .assert()
            .success();

        let scoped_name = scoped(&source, "orphan-test");
        remove_pasture_from_state(&env.home, &scoped_name);

        // Pasture dir still exists on disk — now it's orphaned.
        assert!(pasture_dir.join("orphan-test").exists());

        env.cow()
            .args(["migrate", "--source", source.path().to_str().unwrap(), "--all"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Migrated"));

        let state = read_state(&env.home);
        assert!(
            state["pastures"].as_array().unwrap().iter().any(|p| p["name"].as_str() == Some("orphan-test")),
            "orphaned pasture should be re-registered in state"
        );
    }

    #[test]
    fn migrate_source_is_worktree_fails() {
        let env = Env::new();
        let source = make_git_repo();
        let wt_base = tempfile::TempDir::new().unwrap();
        let wt_path = wt_base.path().join("src-wt");
        git(source.path(), &["worktree", "add", wt_path.to_str().unwrap(), "-b", "src-wt"]);

        // Using the linked worktree as the source should be rejected.
        env.cow()
            .args(["migrate", "--source", wt_path.to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("git worktree"));
    }

    #[test]
    fn migrate_cwd_detection() {
        // Run cow migrate from inside the source repo with no --source flag.
        let env = Env::new();
        let source = make_git_repo();
        env.cow()
            .args(["migrate"])
            .current_dir(source.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("No candidates found"));
    }

    #[test]
    fn migrate_already_registered_worktree_skipped() {
        // A worktree that is already registered in state should not appear as a candidate.
        let env = Env::new();
        let source = make_git_repo();
        let wt_base = tempfile::TempDir::new().unwrap();
        let wt_path = wt_base.path().join("existing-wt");
        git(source.path(), &["worktree", "add", wt_path.to_str().unwrap(), "-b", "existing-wt"]);

        // Register the worktree path directly in state so discover_git_worktrees skips it.
        // Canonicalise so the path matches what git worktree list outputs (macOS symlink resolution).
        let canonical_wt = wt_path.canonicalize().unwrap_or_else(|_| wt_path.clone());
        let state_path = env.home.join(".cow/state.json");
        let initial_state = serde_json::json!({
            "pastures": [{
                "name": "existing-wt",
                "path": canonical_wt.to_str().unwrap(),
                "source": source.path().to_str().unwrap(),
                "vcs": "git",
                "branch": "existing-wt",
                "initial_commit": null,
                "created_at": "2026-01-01T00:00:00Z",
                "symlinked_dirs": [],
                "linked_dirs": [],
                "is_worktree": false
            }]
        });
        std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        std::fs::write(&state_path, serde_json::to_string_pretty(&initial_state).unwrap()).unwrap();

        env.cow()
            .args(["migrate", "--source", source.path().to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains("No candidates found"));
    }

}

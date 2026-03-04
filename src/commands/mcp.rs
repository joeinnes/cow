use anyhow::Result;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

pub fn run() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(resp) = handle(&req) {
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
        }
    }

    Ok(())
}

fn handle(req: &Value) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = req["method"].as_str().unwrap_or("");

    // Notifications have no id — don't respond.
    if id.is_none() {
        return None;
    }

    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "cow",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
        "tools/list" => json!({ "tools": tools_list() }),
        "tools/call" => {
            let name = req["params"]["name"].as_str().unwrap_or("");
            let args = &req["params"]["arguments"];
            match call_tool(name, args) {
                Ok(text) => json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": false
                }),
                Err(e) => json!({
                    "content": [{ "type": "text", "text": e.to_string() }],
                    "isError": true
                }),
            }
        }
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "Method not found" }
            }));
        }
    };

    Some(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    }))
}

fn tools_list() -> Value {
    json!([
        {
            "name": "cow_create",
            "description": "Create a new cow pasture (copy-on-write workspace) from a git or jj repository using APFS clonefile(2). Near-instant and near-zero disk overhead. 'create a cow pasture' → use this tool.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name for the pasture (auto-generates agent-1, agent-2, … if omitted)"
                    },
                    "source": {
                        "type": "string",
                        "description": "Absolute path to the source repository"
                    },
                    "branch": {
                        "type": "string",
                        "description": "Git branch to check out or create in the pasture"
                    },
                    "dir": {
                        "type": "string",
                        "description": "Parent directory for pastures (defaults to ~/.cow/pastures/)"
                    }
                },
                "required": ["source"]
            }
        },
        {
            "name": "cow_list",
            "description": "List all active cow pastures, returning JSON with name, path, source, branch, vcs, and created_at.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Filter to only pastures created from this source repository path"
                    }
                }
            }
        },
        {
            "name": "cow_remove",
            "description": "Remove one or more pastures, deleting their directories. Always runs non-interactively.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "names": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Pasture names to remove"
                    },
                    "all": {
                        "type": "boolean",
                        "description": "Remove all pastures (can be combined with source)"
                    },
                    "source": {
                        "type": "string",
                        "description": "Only remove pastures from this source repository path"
                    }
                }
            }
        },
        {
            "name": "cow_status",
            "description": "Show detailed status of a pasture as JSON: path, branch, VCS dirty/clean state, modified files, initial_commit, and created_at.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Pasture name"
                    }
                },
                "required": ["name"]
            }
        },
        {
            "name": "cow_sync",
            "description": "Fetch the latest commits from the source repository and rebase (or merge) the pasture onto them. No network access required.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Pasture name"
                    },
                    "source_branch": {
                        "type": "string",
                        "description": "Branch in the source repo to sync from (defaults to pasture's current branch)"
                    },
                    "merge": {
                        "type": "boolean",
                        "description": "Use merge instead of rebase"
                    }
                },
                "required": ["name"]
            }
        },
        {
            "name": "cow_extract",
            "description": "Extract changes from a pasture. Use --branch to create a local branch in the source repo, or --patch to write a patch file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Pasture name"
                    },
                    "branch": {
                        "type": "string",
                        "description": "Create this branch in the source repo at pasture HEAD"
                    },
                    "patch": {
                        "type": "string",
                        "description": "Write changes as a patch file at this path"
                    }
                },
                "required": ["name"]
            }
        },
        {
            "name": "cow_migrate",
            "description": "Discover existing git worktrees or jj workspaces in a source repository and register them as cow pastures, moving their directories under ~/.cow/pastures/.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Absolute path to the source repository to scan for worktrees/workspaces"
                    },
                    "all": {
                        "type": "boolean",
                        "description": "Migrate all discovered candidates without prompting"
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Skip dirty-state checks and migrate anyway"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "Show what would be done without making any changes"
                    }
                },
                "required": ["source"]
            }
        },
        {
            "name": "cow_materialise",
            "description": "Replace symlinked dependency directories in a pasture with real APFS clonefiles, making the pasture fully independent of the source. Use when you need to install packages locally without affecting the source.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Pasture name"
                    }
                },
                "required": ["name"]
            }
        },
        {
            "name": "cow_fetch_from",
            "description": "Fetch refs from another pasture into this one, enabling cross-pasture rebase without touching any remote. Useful for rebasing one agent's work on top of another's.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": {
                        "type": "string",
                        "description": "Name of the pasture to fetch refs from"
                    },
                    "name": {
                        "type": "string",
                        "description": "Name of the pasture to fetch into (defaults to current directory)"
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Allow fetching from a pasture with a different source repo"
                    }
                },
                "required": ["from"]
            }
        },
        {
            "name": "cow_run",
            "description": "Run a command inside a pasture's working directory. Automatically detects the package manager (npm/pnpm/yarn/bun) from lockfiles and injects shims so install subcommands write to the pasture-local node_modules. Sets COW_PASTURE, COW_SOURCE, and COW_PASTURE_PATH env vars.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Pasture name"
                    },
                    "command": {
                        "type": "string",
                        "description": "Command and arguments to run (e.g. 'npm install', 'pnpm test', 'cargo build')"
                    }
                },
                "required": ["name", "command"]
            }
        }
    ])
}

fn call_tool(name: &str, args: &Value) -> Result<String> {
    let exe = std::env::current_exe()?;

    let output = match name {
        "cow_create" => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg("create");
            if let Some(n) = args["name"].as_str() {
                cmd.arg(n);
            }
            if let Some(s) = args["source"].as_str() {
                cmd.args(["--source", s]);
            }
            if let Some(b) = args["branch"].as_str() {
                cmd.args(["--branch", b]);
            }
            if let Some(d) = args["dir"].as_str() {
                cmd.args(["--dir", d]);
            }
            cmd.output()?
        }
        "cow_list" => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.args(["list", "--json"]);
            if let Some(s) = args["source"].as_str() {
                cmd.args(["--source", s]);
            }
            cmd.output()?
        }
        "cow_remove" => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg("remove");
            cmd.arg("--force"); // MCP calls are always non-interactive.
            if args["all"].as_bool().unwrap_or(false) {
                cmd.arg("--all");
            }
            if let Some(s) = args["source"].as_str() {
                cmd.args(["--source", s]);
            }
            if let Some(names) = args["names"].as_array() {
                for n in names {
                    if let Some(s) = n.as_str() {
                        cmd.arg(s);
                    }
                }
            }
            cmd.output()?
        }
        "cow_status" => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.args(["status", "--json"]);
            if let Some(n) = args["name"].as_str() {
                cmd.arg(n);
            }
            cmd.output()?
        }
        "cow_sync" => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg("sync");
            if let Some(b) = args["source_branch"].as_str() {
                cmd.arg(b);
            }
            if let Some(n) = args["name"].as_str() {
                cmd.args(["--name", n]);
            }
            if args["merge"].as_bool().unwrap_or(false) {
                cmd.arg("--merge");
            }
            cmd.output()?
        }
        "cow_extract" => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg("extract");
            if let Some(n) = args["name"].as_str() {
                cmd.arg(n);
            }
            if let Some(b) = args["branch"].as_str() {
                cmd.args(["--branch", b]);
            }
            if let Some(p) = args["patch"].as_str() {
                cmd.args(["--patch", p]);
            }
            cmd.output()?
        }
        "cow_migrate" => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg("migrate");
            if let Some(s) = args["source"].as_str() {
                cmd.args(["--source", s]);
            }
            if args["all"].as_bool().unwrap_or(false) {
                cmd.arg("--all");
            }
            if args["force"].as_bool().unwrap_or(false) {
                cmd.arg("--force");
            }
            if args["dry_run"].as_bool().unwrap_or(false) {
                cmd.arg("--dry-run");
            }
            cmd.output()?
        }
        "cow_materialise" => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg("materialise");
            if let Some(n) = args["name"].as_str() {
                cmd.arg(n);
            }
            cmd.output()?
        }
        "cow_fetch_from" => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg("fetch-from");
            if let Some(f) = args["from"].as_str() {
                cmd.arg(f);
            }
            if let Some(n) = args["name"].as_str() {
                cmd.args(["--name", n]);
            }
            if args["force"].as_bool().unwrap_or(false) {
                cmd.arg("--force");
            }
            cmd.output()?
        }
        "cow_run" => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg("run");
            if let Some(n) = args["name"].as_str() {
                cmd.arg(n);
            }
            if let Some(command) = args["command"].as_str() {
                // Split the command string into tokens for the trailing var-arg.
                for token in command.split_whitespace() {
                    cmd.arg(token);
                }
            }
            cmd.output()?
        }
        _ => anyhow::bail!("Unknown tool: {}", name),
    };

    let mut result = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if !output.status.success() {
        anyhow::bail!("{}", result.trim());
    }
    Ok(result)
}

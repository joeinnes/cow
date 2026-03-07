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
            "description": "Create a new copy-on-write workspace from a git or jj repository using APFS clonefile(2). Near-instant and near-zero disk overhead.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name for the workspace (auto-generates agent-1, agent-2, … if omitted)"
                    },
                    "source": {
                        "type": "string",
                        "description": "Absolute path to the source repository"
                    },
                    "branch": {
                        "type": "string",
                        "description": "Git branch to check out or create in the workspace"
                    },
                    "dir": {
                        "type": "string",
                        "description": "Parent directory for workspaces (defaults to ~/.cow/workspaces/)"
                    }
                },
                "required": ["source"]
            }
        },
        {
            "name": "cow_list",
            "description": "List all active cow workspaces, returning JSON with name, path, source, branch, vcs, and created_at.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Filter to only workspaces created from this source repository path"
                    }
                }
            }
        },
        {
            "name": "cow_remove",
            "description": "Remove one or more workspaces, deleting their directories. Always runs non-interactively.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "names": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Workspace names to remove"
                    },
                    "all": {
                        "type": "boolean",
                        "description": "Remove all workspaces (can be combined with source)"
                    },
                    "source": {
                        "type": "string",
                        "description": "Only remove workspaces from this source repository path"
                    }
                }
            }
        },
        {
            "name": "cow_status",
            "description": "Show detailed status of a workspace: path, branch, VCS dirty/clean state, and list of changed files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Workspace name"
                    }
                },
                "required": ["name"]
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
            cmd.arg("status");
            if let Some(n) = args["name"].as_str() {
                cmd.arg(n);
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
    Ok(result)
}

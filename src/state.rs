use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::vcs::Vcs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub name: String,
    pub path: PathBuf,
    pub source: PathBuf,
    pub vcs: Vcs,
    /// The branch that was active at creation time (git only).
    pub branch: Option<String>,
    /// The HEAD SHA at the moment the workspace was cloned (git only).
    /// Used by `cow extract --patch` to produce a complete patch regardless
    /// of how many commits have been made in the workspace since.
    #[serde(default)]
    pub initial_commit: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct State {
    pub workspaces: Vec<WorkspaceEntry>,
}

impl State {
    pub fn load() -> Result<Self> {
        let path = state_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read state file: {}", path.display()))?;
        serde_json::from_str(&content).with_context(|| "Failed to parse state file")
    }

    pub fn save(&self) -> Result<()> {
        let path = state_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write state file: {}", path.display()))
    }

    pub fn add(&mut self, entry: WorkspaceEntry) {
        self.workspaces.push(entry);
    }

    /// Remove a workspace by name. Returns true if it was present.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.workspaces.len();
        self.workspaces.retain(|w| w.name != name);
        self.workspaces.len() < before
    }

    pub fn get(&self, name: &str) -> Option<&WorkspaceEntry> {
        self.workspaces.iter().find(|w| w.name == name)
    }

    /// Remove entries whose workspace directory no longer exists on disk.
    pub fn prune_deleted(&mut self) {
        self.workspaces.retain(|w| w.path.exists());
    }

    /// Generate the next unused `agent-N` name.
    pub fn next_agent_name(&self) -> String {
        for i in 1u32.. {
            let name = format!("agent-{}", i);
            if !self.workspaces.iter().any(|w| w.name == name) {
                return name;
            }
        }
        unreachable!()
    }
}

pub fn state_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".cow").join("state.json"))
}

pub fn default_workspace_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".cow").join("workspaces"))
}

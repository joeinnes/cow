use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::vcs::Vcs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastureEntry {
    pub name: String,
    pub path: PathBuf,
    pub source: PathBuf,
    pub vcs: Vcs,
    /// The branch that was active at creation time (git only).
    pub branch: Option<String>,
    /// The HEAD SHA at the moment the pasture was cloned (git only).
    /// Used by `cow extract --patch` to produce a complete patch regardless
    /// of how many commits have been made in the pasture since.
    #[serde(default)]
    pub initial_commit: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Directories (relative paths) that were symlinked whole instead of cloned.
    /// Set by `cow create` when large non-dependency dirs are auto-detected.
    /// Clear with `cow materialise <name>`.
    #[serde(default)]
    pub symlinked_dirs: Vec<String>,
    /// Dependency directories (node_modules, vendor, etc.) where each top-level
    /// entry is symlinked individually (per-package) rather than the whole dir.
    /// Set by `cow create`. Clear with `cow materialise <name>`.
    #[serde(default)]
    pub linked_dirs: Vec<String>,
    /// True when this pasture was created as a git linked worktree rather
    /// than a CoW clone. `cow remove` must call `git worktree remove`
    /// instead of `rm -rf` so the back-link in the source repo is cleaned up.
    #[serde(default)]
    pub is_worktree: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct State {
    pub pastures: Vec<PastureEntry>,
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

    pub fn add(&mut self, entry: PastureEntry) {
        self.pastures.push(entry);
    }

    /// Remove a pasture by name. Returns true if it was present.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.pastures.len();
        self.pastures.retain(|w| w.name != name);
        self.pastures.len() < before
    }

    pub fn get(&self, name: &str) -> Option<&PastureEntry> {
        self.pastures.iter().find(|w| w.name == name)
    }

    /// Remove entries whose pasture directory no longer exists on disk.
    pub fn prune_deleted(&mut self) {
        self.pastures.retain(|w| w.path.exists());
    }

    /// Generate the next unused `{prefix}/agent-N` name.
    pub fn next_scoped_name(&self, prefix: &str) -> String {
        for i in 1u32.. {
            let name = format!("{}/agent-{}", prefix, i);
            if !self.pastures.iter().any(|w| w.name == name) {
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

pub fn default_pasture_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".cow").join("pastures"))
}

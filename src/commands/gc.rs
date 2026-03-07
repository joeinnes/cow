use anyhow::Result;
use colored::Colorize;
use dialoguer::Confirm;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::{cli::GcArgs, state::{PastureEntry, State}, vcs::{self, Vcs}};

pub fn run(args: GcArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    // Optionally fetch origin for each unique source repo first.
    if args.fetch {
        let sources: BTreeSet<_> = state.pastures.iter()
            .filter(|w| w.vcs == Vcs::Git)
            .map(|w| w.source.clone())
            .collect();
        for source in &sources {
            print!("Fetching origin in {} ... ", source.display());
            let ok = Command::new("git")
                .args(["-C", source.to_str().unwrap_or(""), "fetch", "origin"])
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            println!("{}", if ok { "done" } else { "failed (skipping)" });
        }
        println!();
    }

    // Find git pastures whose branch exists on origin (and optionally is merged).
    let candidates: Vec<PastureEntry> = state.pastures.iter()
        .filter(|w| w.vcs == Vcs::Git)
        .filter(|w| {
            let Some(branch) = &w.branch else { return false; };
            if !branch_on_origin(&w.source, branch) { return false; }
            if args.merged { branch_merged(&w.source, branch) } else { true }
        })
        .cloned()
        .collect();

    if candidates.is_empty() {
        let qualifier = if args.merged { "merged" } else { "pushed" };
        println!("No pastures with branches {} to origin.", qualifier);
        return Ok(());
    }

    let reason = if args.merged { "merged to origin" } else { "pushed to origin" };
    println!(
        "Found {} pasture{} whose branch has been {}:",
        candidates.len(),
        if candidates.len() == 1 { "" } else { "s" },
        reason,
    );
    for w in &candidates {
        println!("  {} ({})", w.name, w.branch.as_deref().unwrap_or("-"));
    }
    println!();

    if args.dry_run {
        println!("--dry-run: no changes made.");
        return Ok(());
    }

    let mut removed = 0usize;

    for entry in &candidates {
        let name = &entry.name;

        if !args.force && vcs::git_is_dirty(&entry.path) {
            let short = vcs::git_status_short(&entry.path);
            eprintln!("{} Pasture '{}' has uncommitted changes:", "⚠".yellow(), name);
            for line in short.lines() {
                eprintln!("  {}", line);
            }
        }

        let confirmed = args.force || args.yes || {
            match Confirm::new()
                .with_prompt(format!("Remove pasture '{}'?", name))
                .default(false)
                .interact_opt()
            {
                Ok(Some(true)) => true,
                _ => false,
            }
        };

        if confirmed {
            super::remove::remove_pasture_dir(entry, args.force)?;
            state.remove(name);
            println!("🐄 Removed pasture '{}'", name);
            removed += 1;
        }
    }

    state.save()?;

    if removed == 0 {
        println!("No pastures were removed.");
    }

    Ok(())
}

/// Returns true if `branch` exists in origin's remote-tracking refs.
/// Uses cached refs — no network call needed.
fn branch_on_origin(source: &Path, branch: &str) -> bool {
    let Ok(out) = Command::new("git")
        .args(["-C", source.to_str().unwrap_or(""),
               "branch", "-r", "--list", &format!("origin/{}", branch)])
        .stderr(Stdio::null())
        .output()
    else { return false; };
    !String::from_utf8_lossy(&out.stdout).trim().is_empty()
}

/// Returns true if `origin/<branch>` is an ancestor of the default branch
/// (i.e. fully merged). Uses `git merge-base --is-ancestor`.
fn branch_merged(source: &Path, branch: &str) -> bool {
    let default = default_branch(source);
    let status = Command::new("git")
        .args(["-C", source.to_str().unwrap_or(""),
               "merge-base", "--is-ancestor",
               &format!("origin/{}", branch),
               &format!("origin/{}", default)])
        .stderr(Stdio::null())
        .status();
    matches!(status, Ok(s) if s.success())
}

/// Detect origin's default branch via `refs/remotes/origin/HEAD`.
/// Falls back to "main" if unset.
fn default_branch(source: &Path) -> String {
    let Ok(out) = Command::new("git")
        .args(["-C", source.to_str().unwrap_or(""),
               "symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .stderr(Stdio::null())
        .output()
    else { return "main".to_string(); };
    let s = String::from_utf8_lossy(&out.stdout);
    // Returns "origin/main" — strip the prefix.
    s.trim().strip_prefix("origin/").unwrap_or("main").to_string()
}

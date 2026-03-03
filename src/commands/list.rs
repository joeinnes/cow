use anyhow::Result;
use chrono::Utc;
use colored::Colorize;

use crate::{cli::ListArgs, state::State, vcs::{self, Vcs}};

pub fn run(args: ListArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();
    state.save()?;

    let mut workspaces = state.workspaces.clone();

    if let Some(source) = args.source {
        let source = source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf());
        workspaces.retain(|w| w.source == source);
    }

    if args.json {
        let out: Vec<serde_json::Value> = workspaces.iter().map(|w| {
            let current_branch = match w.vcs {
                Vcs::Git => vcs::git_current_branch(&w.path)
                    .unwrap_or_else(|| w.branch.clone().unwrap_or_else(|| "-".to_string())),
                // tarpaulin-ignore-start
                Vcs::Jj => w.branch.clone().unwrap_or_else(|| "-".to_string()),
                // tarpaulin-ignore-end
            };
            let dirty = match w.vcs {
                Vcs::Git => vcs::git_is_dirty(&w.path),
                // tarpaulin-ignore-start
                Vcs::Jj => vcs::jj_is_dirty(&w.path),
                // tarpaulin-ignore-end
            };
            let mut v = serde_json::to_value(w).unwrap();
            v["dirty"] = serde_json::json!(dirty);
            v["current_branch"] = serde_json::json!(current_branch);
            v
        }).collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    if workspaces.is_empty() {
        println!("No workspaces found.");
        return Ok(());
    }

    // Column widths
    const W_NAME: usize = 14;
    const W_SOURCE: usize = 36;
    const W_BRANCH: usize = 20;
    const W_STATUS: usize = 7;

    println!(
        "{:<W_NAME$} {:<W_SOURCE$} {:<W_BRANCH$} {:<W_STATUS$} {}",
        "NAME", "SOURCE", "BRANCH", "STATUS", "CREATED"
    );
    println!("{}", "─".repeat(W_NAME + W_SOURCE + W_BRANCH + W_STATUS + 3 + 15));

    for w in &workspaces {
        // Fetch current branch dynamically (may differ from stored branch)
        let branch = match w.vcs {
            Vcs::Git => vcs::git_current_branch(&w.path)
                .unwrap_or_else(|| w.branch.clone().unwrap_or_else(|| "-".to_string())),
            // tarpaulin-ignore-start
            Vcs::Jj => w.branch.clone().unwrap_or_else(|| "-".to_string()),
            // tarpaulin-ignore-end
        };

        let dirty = match w.vcs {
            Vcs::Git => vcs::git_is_dirty(&w.path),
            // tarpaulin-ignore-start
            Vcs::Jj => vcs::jj_is_dirty(&w.path),
            // tarpaulin-ignore-end
        };

        let status_str = if dirty {
            "dirty".yellow().to_string()
        } else {
            "clean".green().to_string()
        };

        // Pad status manually since ANSI codes bloat the string length
        let status_padded = format!("{}{}", status_str, " ".repeat(W_STATUS.saturating_sub(5)));

        let source_str = truncate_path(&contract_home(&w.source.display().to_string()), W_SOURCE - 1);
        let ago = time_ago(w.created_at);

        println!(
            "{:<W_NAME$} {:<W_SOURCE$} {:<W_BRANCH$} {} {}",
            w.name, source_str, branch, status_padded, ago
        );
    }

    Ok(())
}

/// Replace the home directory prefix with `~` so paths stay readable.
fn contract_home(s: &str) -> String {
    // Prefer HOME env-var over dirs::home_dir(): on macOS dirs uses
    // NSHomeDirectory() which ignores the HOME env, so tests that override HOME
    // would not see their fake home reflected.
    let home = std::env::var("HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(dirs::home_dir);
    if let Some(home_path) = home {
        // Canonicalise to resolve /var → /private/var on macOS, matching the
        // canonicalised paths stored in workspace state.
        let home_str = home_path
            .canonicalize()
            .unwrap_or(home_path)
            .display()
            .to_string();
        if let Some(rest) = s.strip_prefix(&home_str) {
            return format!("~{}", rest);
        }
    }
    s.to_string()
}

fn truncate_path(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("…{}", &s[s.len().saturating_sub(max - 1)..])
    }
}

fn time_ago(dt: chrono::DateTime<Utc>) -> String {
    let secs = (Utc::now() - dt).num_seconds();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        let m = (Utc::now() - dt).num_minutes();
        format!("{} min{} ago", m, if m == 1 { "" } else { "s" })
    } else if secs < 86400 {
        let h = (Utc::now() - dt).num_hours();
        format!("{} hour{} ago", h, if h == 1 { "" } else { "s" })
    } else {
        let d = (Utc::now() - dt).num_days();
        if d < 7 {
            format!("{} day{} ago", d, if d == 1 { "" } else { "s" })
        } else {
            dt.format("%Y-%m-%d").to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn contract_home_replaces_home_prefix_with_tilde() {
        // Build a path that starts with the current process's resolved home.
        let home = std::env::var("HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(dirs::home_dir)
            .unwrap();
        let home_canonical = home.canonicalize().unwrap_or(home);
        let path = home_canonical.join("projects/foo").display().to_string();
        assert_eq!(contract_home(&path), "~/projects/foo");
    }

    #[test]
    fn contract_home_unchanged_when_outside_home() {
        // /private/tmp is guaranteed not to be under HOME on macOS.
        let result = contract_home("/private/tmp/outside/path");
        assert_eq!(result, "/private/tmp/outside/path");
    }

    #[test]
    fn contract_home_home_dir_itself_becomes_tilde() {
        let home = std::env::var("HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(dirs::home_dir)
            .unwrap();
        let home_canonical = home.canonicalize().unwrap_or(home).display().to_string();
        assert_eq!(contract_home(&home_canonical), "~");
    }

    #[test]
    fn truncate_path_short_unchanged() {
        assert_eq!(truncate_path("short", 20), "short");
    }

    #[test]
    fn truncate_path_long_gets_ellipsis() {
        let long = "a".repeat(40);
        let result = truncate_path(&long, 10);
        assert!(result.starts_with('…'));
        assert!(result.len() <= 10 + "…".len()); // ellipsis is multi-byte
    }

    #[test]
    fn time_ago_seconds() {
        let dt = Utc::now() - Duration::seconds(30);
        assert_eq!(time_ago(dt), "just now");
    }

    #[test]
    fn time_ago_minutes() {
        let dt = Utc::now() - Duration::minutes(5);
        assert_eq!(time_ago(dt), "5 mins ago");
    }

    #[test]
    fn time_ago_one_minute() {
        let dt = Utc::now() - Duration::minutes(1);
        assert_eq!(time_ago(dt), "1 min ago");
    }

    #[test]
    fn time_ago_hours() {
        let dt = Utc::now() - Duration::hours(3);
        assert_eq!(time_ago(dt), "3 hours ago");
    }

    #[test]
    fn time_ago_one_hour() {
        let dt = Utc::now() - Duration::hours(1);
        assert_eq!(time_ago(dt), "1 hour ago");
    }

    #[test]
    fn time_ago_days() {
        let dt = Utc::now() - Duration::days(3);
        assert_eq!(time_ago(dt), "3 days ago");
    }

    #[test]
    fn time_ago_one_day() {
        let dt = Utc::now() - Duration::days(1);
        assert_eq!(time_ago(dt), "1 day ago");
    }

    #[test]
    fn time_ago_old_shows_date() {
        let dt = Utc::now() - Duration::days(10);
        let result = time_ago(dt);
        // Should be a date like "2026-02-20"
        assert!(result.contains('-'), "expected date format, got: {}", result);
    }
}

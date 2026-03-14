use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use rayon::prelude::*;

use crate::{cli::ListArgs, state::{PastureEntry, State}, vcs::{self, Vcs}};

struct PastureInfo {
    branch:  String,
    dirty:   bool,
    count:   usize,
}

fn compute_info(w: &PastureEntry) -> PastureInfo {
    let branch = match w.vcs {
        Vcs::Git => vcs::git_current_branch(&w.path)
            .unwrap_or_else(|| w.branch.clone().unwrap_or_default()),
        // tarpaulin-ignore-start
        Vcs::Jj => w.branch.clone().unwrap_or_default(),
        // tarpaulin-ignore-end
    };
    let dirty = match w.vcs {
        Vcs::Git => vcs::git_is_dirty(&w.path),
        // tarpaulin-ignore-start
        Vcs::Jj => vcs::jj_is_dirty(&w.path),
        // tarpaulin-ignore-end
    };
    let count = if dirty {
        match w.vcs {
            Vcs::Git => vcs::git_status_short(&w.path).lines().count(),
            // tarpaulin-ignore-start
            Vcs::Jj => vcs::jj_diff_summary(&w.path).lines().filter(|l| !l.is_empty()).count(),
            // tarpaulin-ignore-end
        }
    } else {
        0
    };
    PastureInfo { branch, dirty, count }
}

pub fn run(args: ListArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();
    state.save()?;

    let mut workspaces = state.pastures.clone();

    if let Some(source) = args.source {
        let source = source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf());
        workspaces.retain(|w| w.source == source);
    }

    workspaces.sort_by(|a, b| a.name.cmp(&b.name));

    // Parallel: compute all git info up-front before any printing.
    let infos: Vec<PastureInfo> = workspaces.par_iter().map(compute_info).collect();

    if args.json {
        let out: Vec<serde_json::Value> = workspaces.iter().zip(infos.iter()).map(|(w, info)| {
            let mut v = serde_json::to_value(w).unwrap();
            v["dirty"]          = serde_json::json!(info.dirty);
            v["current_branch"] = serde_json::json!(info.branch);
            v
        }).collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    if workspaces.is_empty() {
        println!("No pastures found.");
        return Ok(());
    }

    // Column widths
    const W_NAME:   usize = 38;
    const W_STATUS: usize = 12;
    const W_BRANCH: usize = 20;

    let any_branch = workspaces.iter().zip(infos.iter()).any(|(w, info)| {
        let suffix = w.name.rsplit('/').next().unwrap_or(&w.name);
        !info.branch.is_empty() && info.branch != suffix
    });

    if any_branch {
        if args.paths {
            println!("{:<W_NAME$} {:<W_STATUS$} {:<W_BRANCH$} {:<15} {}", "NAME", "STATUS", "BRANCH", "CREATED", "PATH");
        } else {
            println!("{:<W_NAME$} {:<W_STATUS$} {:<W_BRANCH$} {}", "NAME", "STATUS", "BRANCH", "CREATED");
        }
        println!("{}", "─".repeat(W_NAME + W_STATUS + W_BRANCH + 3 + 15));
    } else if args.paths {
        println!("{:<W_NAME$} {:<W_STATUS$} {:<15} {}", "NAME", "STATUS", "CREATED", "PATH");
        println!("{}", "─".repeat(W_NAME + W_STATUS + 1 + 15));
    } else {
        println!("{:<W_NAME$} {:<W_STATUS$} {}", "NAME", "STATUS", "CREATED");
        println!("{}", "─".repeat(W_NAME + W_STATUS + 1 + 15));
    }

    for (w, info) in workspaces.iter().zip(infos.iter()) {
        let (status_raw_len, status_str) = if info.dirty {
            let raw = match w.vcs {
                Vcs::Git => format!("dirty ({})", info.count),
                // tarpaulin-ignore-start
                Vcs::Jj => format!("changed ({})", info.count),
                // tarpaulin-ignore-end
            };
            let len = raw.len();
            (len, raw.yellow().to_string())
        } else {
            ("clean".len(), "clean".green().to_string())
        };

        let status_padded = format!("{}{}", status_str, " ".repeat(W_STATUS.saturating_sub(status_raw_len)));

        let name_display = truncate_name(&w.name, W_NAME - 1);
        let ago = time_ago(w.created_at);

        let name_suffix = w.name.rsplit('/').next().unwrap_or(&w.name);
        let branch_display = if !info.branch.is_empty() && info.branch != name_suffix {
            info.branch.as_str()
        } else {
            ""
        };

        if any_branch {
            if args.paths {
                println!("{:<W_NAME$} {} {:<W_BRANCH$} {:<15} {}", name_display, status_padded, branch_display, ago, w.path.display());
            } else {
                println!("{:<W_NAME$} {} {:<W_BRANCH$} {}", name_display, status_padded, branch_display, ago);
            }
        } else if args.paths {
            println!("{:<W_NAME$} {} {:<15} {}", name_display, status_padded, ago, w.path.display());
        } else {
            println!("{:<W_NAME$} {} {}", name_display, status_padded, ago);
        }
    }

    Ok(())
}


/// Truncate a name from the right, appending `…` if it exceeds `max` bytes.
/// The project prefix (before `/`) is more useful than the suffix, so we keep
/// the beginning.
fn truncate_name(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let ellipsis = "…";
        let cut = max.saturating_sub(ellipsis.len());
        format!("{}{}", &s[..cut], ellipsis)
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
    fn truncate_name_short_unchanged() {
        assert_eq!(truncate_name("short", 20), "short");
    }

    #[test]
    fn truncate_name_exact_unchanged() {
        let s = "a".repeat(20);
        assert_eq!(truncate_name(&s, 20), s);
    }

    #[test]
    fn truncate_name_long_gets_ellipsis_at_right() {
        let long = "brightblur/very-long-branch-name-here";
        let result = truncate_name(long, 20);
        assert!(result.ends_with('…'), "should end with ellipsis, got: {}", result);
        assert!(result.starts_with("brightblur"), "should keep prefix, got: {}", result);
        // byte length should be at most 20 + len("…") - len("…") = 20
        assert!(result.len() <= 20, "result too long: {}", result);
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

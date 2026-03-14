use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::state::State;

pub fn run() -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    if state.pastures.is_empty() {
        println!("No pastures found.");
        return Ok(());
    }

    // Group pastures by source (BTreeMap → sorted by source path).
    let mut by_source: BTreeMap<PathBuf, Vec<&_>> = BTreeMap::new();
    for w in &state.pastures {
        by_source.entry(w.source.clone()).or_default().push(w);
    }

    // Collect rows first so we can size columns dynamically.
    struct Row {
        source:     String,
        count:      usize,
        disk:       String,
        delta:      String,
        npm_saved:  String,
        pnpm_saved: String,
    }

    let mut rows: Vec<Row> = Vec::new();
    let mut grand_npm_saved:  i64 = 0;
    let mut grand_pnpm_saved: i64 = 0;
    let mut grand_delta:      u64 = 0;
    let mut grand_disk:       u64 = 0;
    let mut grand_pastures:   usize = 0;

    for (i, (source, pastures)) in by_source.iter().enumerate() {
        let count = pastures.len();

        let total_bytes  = du_bytes(source);
        let nm_bytes     = du_bytes(&source.join("node_modules"));
        let target_bytes = du_bytes(&source.join("target"));

        let npm_clone  = total_bytes.saturating_sub(target_bytes) as i64;
        let pnpm_clone = total_bytes.saturating_sub(target_bytes).saturating_sub(nm_bytes) as i64;

        let delta: u64 = pastures.iter().map(|w| pasture_delta_bytes(&w.path)).sum();

        let npm_saved  = npm_clone  * count as i64 - delta as i64;
        let pnpm_saved = pnpm_clone * count as i64 - delta as i64;

        grand_npm_saved  += npm_saved;
        grand_pnpm_saved += pnpm_saved;
        grand_delta      += delta;
        grand_disk       += total_bytes;
        grand_pastures   += count;

        rows.push(Row {
            source:     format!("Project {}", i + 1),
            count,
            disk:       fmt(total_bytes),
            delta:      fmt(delta),
            npm_saved:  fmt_saved(npm_saved),
            pnpm_saved: fmt_saved(pnpm_saved),
        });
    }

    let npm_pct  = pct(grand_npm_saved,  grand_npm_saved  + grand_delta as i64);
    let pnpm_pct = pct(grand_pnpm_saved, grand_pnpm_saved + grand_delta as i64);

    let footer_npm  = format!("{} ({}%)", fmt_saved(grand_npm_saved),  npm_pct);
    let footer_pnpm = format!("{} ({}%)", fmt_saved(grand_pnpm_saved), pnpm_pct);

    // Column widths — fit content, respect minimums.
    let w_src  = rows.iter().map(|r| r.source.len()).max().unwrap_or(6)
                    .max("Source".len());
    let w_num  = rows.iter().map(|r| digits(r.count)).max().unwrap_or(1)
                    .max("Pastures".len());
    let w_disk = rows.iter().map(|r| r.disk.len()).max().unwrap_or(4)
                    .max(fmt(grand_disk).len()).max("On disk".len());
    let w_dlt  = rows.iter().map(|r| r.delta.len()).max().unwrap_or(3)
                    .max(fmt(grand_delta).len()).max("Delta".len());
    let w_npm  = rows.iter().map(|r| r.npm_saved.len()).max().unwrap_or(4)
                    .max(footer_npm.len()).max("Saved (npm)".len());
    let w_pnpm = rows.iter().map(|r| r.pnpm_saved.len()).max().unwrap_or(4)
                    .max(footer_pnpm.len()).max("Saved (pnpm)".len());

    // Box-drawing helpers.
    let top = format!(
        "┌{}┬{}┬{}┬{}┬{}┬{}┐",
        "─".repeat(w_src  + 2), "─".repeat(w_num  + 2),
        "─".repeat(w_disk + 2), "─".repeat(w_dlt  + 2),
        "─".repeat(w_npm  + 2), "─".repeat(w_pnpm + 2),
    );
    let mid = top.replace('┌', "├").replace('┬', "┼").replace('┐', "┤").replace('─', "─");
    let bot = top.replace('┌', "└").replace('┬', "┴").replace('┐', "┘");

    let row = |src: &str, num: &str, disk: &str, dlt: &str, npm: &str, pnpm: &str| {
        format!(
            "│ {:<w_src$} │ {:>w_num$} │ {:>w_disk$} │ {:>w_dlt$} │ {:>w_npm$} │ {:>w_pnpm$} │",
            src, num, disk, dlt, npm, pnpm,
        )
    };

    println!("{}", top);
    println!("{}", row("Source", "Pastures", "On disk", "Delta", "Saved (npm)", "Saved (pnpm)"));
    println!("{}", mid);
    for r in &rows {
        println!("{}", row(&r.source, &r.count.to_string(), &r.disk, &r.delta, &r.npm_saved, &r.pnpm_saved));
    }
    println!("{}", mid);
    println!("{}", row("Total", &grand_pastures.to_string(), &fmt(grand_disk), &fmt(grand_delta), &footer_npm, &footer_pnpm));
    println!("{}", bot);

    println!();
    println!("Excludes build outputs (target/, .next/, dist/) — assumes fresh clones.");
    println!("npm/yarn-classic: full copies per workspace. pnpm/bun: file content hardlinked from global cache.");

    Ok(())
}

fn du_bytes(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let Ok(out) = Command::new("du")
        .args(["-sk", path.to_str().unwrap_or("")])
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return 0;
    };
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        * 1024
}

fn pasture_delta_bytes(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut files: Vec<String> = Vec::new();

    if let Ok(out) = Command::new("git")
        .args(["-C", path.to_str().unwrap_or(""), "diff", "HEAD", "--name-only"])
        .stderr(std::process::Stdio::null())
        .output()
    {
        files.extend(String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned));
    }
    if let Ok(out) = Command::new("git")
        .args(["-C", path.to_str().unwrap_or(""), "ls-files", "--others", "--exclude-standard"])
        .stderr(std::process::Stdio::null())
        .output()
    {
        files.extend(String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned));
    }

    files.iter()
        .filter(|f| !f.is_empty())
        .filter_map(|f| std::fs::metadata(path.join(f)).ok())
        .map(|m| m.len())
        .sum()
}

fn fmt(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn fmt_saved(bytes: i64) -> String {
    if bytes <= 0 { "—".to_string() } else { format!("~{}", fmt(bytes as u64)) }
}

fn pct(saved: i64, total: i64) -> u64 {
    if saved <= 0 || total <= 0 { return 0; }
    (saved as u64 * 100) / total as u64
}

fn digits(n: usize) -> usize {
    if n == 0 { 1 } else { n.ilog10() as usize + 1 }
}

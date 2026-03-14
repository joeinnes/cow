use anyhow::{bail, Context, Result};

use crate::{cli::MaterialiseArgs, state::State};

pub fn run(args: MaterialiseArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    let entry = state
        .get(&args.name)
        .cloned()
        .with_context(|| format!("Pasture '{}' not found.", args.name))?;

    if entry.symlinked_dirs.is_empty() && entry.linked_dirs.is_empty() {
        println!("Pasture '{}' has no symlinked directories.", args.name);
        return Ok(());
    }

    let total = entry.symlinked_dirs.len() + entry.linked_dirs.len();
    println!(
        "🐄 Materialising {} director{} in pasture '{}' ...",
        total,
        if total == 1 { "y" } else { "ies" },
        args.name
    );

    // --- Whole-dir symlinks (symlinked_dirs) ---
    let mut remaining_whole: Vec<String> = entry.symlinked_dirs.clone();

    for rel_str in &entry.symlinked_dirs {
        let src = entry.source.join(rel_str);
        let dst = entry.path.join(rel_str);

        if !dst.exists() && !is_symlink(&dst) {
            remaining_whole.retain(|p| p != rel_str);
            continue;
        }

        if !src.exists() {
            eprintln!(
                "  {} — source '{}' no longer exists, skipping.",
                rel_str,
                src.display()
            );
            continue;
        }

        print!("  {} ... ", rel_str);

        if is_symlink(&dst) {
            std::fs::remove_file(&dst)
                .with_context(|| format!("Failed to remove symlink: {}", dst.display()))?;
        } else if dst.is_dir() {
            println!("already a real directory, skipping.");
            remaining_whole.retain(|p| p != rel_str);
            continue;
        }

        clone_entry(&src, &dst)
            .with_context(|| format!("Failed to materialise '{}'", rel_str))?;

        println!("done");
        remaining_whole.retain(|p| p != rel_str);
    }

    // --- Per-package symlinks (linked_dirs) ---
    let mut remaining_linked: Vec<String> = entry.linked_dirs.clone();

    for rel_str in &entry.linked_dirs {
        let src = entry.source.join(rel_str);
        let dst = entry.path.join(rel_str);

        if !src.exists() {
            eprintln!(
                "  {} — source '{}' no longer exists, skipping.",
                rel_str,
                src.display()
            );
            continue;
        }

        if !dst.exists() {
            // Already gone — remove from list silently.
            remaining_linked.retain(|p| p != rel_str);
            continue;
        }

        println!("  {} (per-package) ...", rel_str);

        // Iterate top-level entries in the real dir: each should be a symlink.
        for child in std::fs::read_dir(&dst)
            .with_context(|| format!("Failed to read '{}'", dst.display()))?
        {
            let child = child?;
            let child_dst = child.path();
            let child_name = child.file_name();
            let child_src = src.join(&child_name);

            if !child_src.exists() {
                // Package no longer in source — leave as-is.
                continue;
            }

            print!("    {} ... ", child_name.to_string_lossy());

            if is_symlink(&child_dst) {
                std::fs::remove_file(&child_dst)
                    .with_context(|| format!("Failed to remove symlink: {}", child_dst.display()))?;
                clone_entry(&child_src, &child_dst)
                    .with_context(|| format!("Failed to materialise '{}'", child_dst.display()))?;
                println!("done");
            } else {
                println!("already materialised, skipping.");
            }
        }

        // Also clone any packages that exist in source but not yet in dst
        // (packages added to source after the pasture was created).
        for child in std::fs::read_dir(&src)
            .with_context(|| format!("Failed to read source '{}'", src.display()))?
        {
            let child = child?;
            let child_dst = dst.join(child.file_name());
            if !child_dst.exists() {
                print!("    {} (new) ... ", child.file_name().to_string_lossy());
                clone_entry(&child.path(), &child_dst)
                    .with_context(|| format!("Failed to clone new package '{}'", child.path().display()))?;
                println!("done");
            }
        }

        remaining_linked.retain(|p| p != rel_str);
    }

    // Update state.
    let entry_mut = state
        .pastures
        .iter_mut()
        .find(|p| p.name == args.name)
        .ok_or_else(|| anyhow::anyhow!("Pasture '{}' disappeared from state.", args.name))?;
    entry_mut.symlinked_dirs = remaining_whole;
    entry_mut.linked_dirs = remaining_linked;
    state.save()?;

    println!("Done.");
    Ok(())
}

fn is_symlink(path: &std::path::Path) -> bool {
    path.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Clone `src` directory to `dst` using the best available method.
fn clone_entry(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::ffi::CString;
        let src_c = CString::new(src.to_str().context("Source path is not UTF-8")?)
            .context("Source path contains a null byte")?;
        let dst_c = CString::new(dst.to_str().context("Dest path is not UTF-8")?)
            .context("Dest path contains a null byte")?;
        let ret = unsafe { libc::clonefile(src_c.as_ptr(), dst_c.as_ptr(), 0) };
        if ret != 0 {
            bail!(
                "clonefile failed '{}' → '{}': {}",
                src.display(),
                dst.display(),
                std::io::Error::last_os_error()
            );
        }
        return Ok(());
    }

    // tarpaulin-ignore-start
    #[cfg(not(target_os = "macos"))]
    {
        // Linux: attempt copy-on-write via cp --reflink=always (btrfs, xfs).
        // Fall back to a regular copy with a warning if the filesystem does not support it.
        let reflink_status = std::process::Command::new("cp")
            .args(["--reflink=always", "-R", src.to_str().unwrap(), dst.to_str().unwrap()])
            .status();

        match reflink_status {
            Ok(s) if s.success() => return Ok(()),
            _ => {
                eprintln!(
                    "Warning: filesystem does not support reflinks (btrfs/xfs required). \
                     Falling back to a regular copy — disk overhead will be higher."
                );
                let _ = std::fs::remove_dir_all(dst);
                let status = std::process::Command::new("cp")
                    .args(["-R", src.to_str().unwrap(), dst.to_str().unwrap()])
                    .status()
                    .context("Failed to run cp")?;
                if !status.success() {
                    bail!("cp -R failed for '{}'", src.display());
                }
                Ok(())
            }
        }
    }
    // tarpaulin-ignore-end
}

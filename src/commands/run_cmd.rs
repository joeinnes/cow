use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{cli::RunArgs, state::State};

pub fn run(args: RunArgs) -> Result<()> {
    let mut state = State::load()?;
    state.prune_deleted();

    let entry = state
        .get(&args.name)
        .cloned()
        .with_context(|| format!("Pasture '{}' not found.", args.name))?;

    if !entry.path.exists() {
        bail!(
            "Pasture '{}' no longer exists at '{}'.",
            args.name,
            entry.path.display()
        );
    }

    let (program, rest) = args
        .command
        .split_first()
        .context("No command specified.")?;

    // Detect package manager and write shims if needed.
    let shims = shims_dir()?;
    let pm = detect_pm(&entry.path);
    if let Some(pm_name) = pm {
        ensure_shim(pm_name, &shims)?;
    }

    // Build PATH: prepend shims dir when a PM is detected.
    let path_env = {
        let current = std::env::var("PATH").unwrap_or_default();
        if pm.is_some() {
            format!("{}:{}", shims.to_string_lossy(), current)
        } else {
            current
        }
    };

    let status = Command::new(program)
        .args(rest)
        .current_dir(&entry.path)
        .env("PATH", &path_env)
        .env("COW_PASTURE", &entry.name)
        .env("COW_SOURCE", entry.source.to_string_lossy().as_ref())
        .env("COW_PASTURE_PATH", entry.path.to_string_lossy().as_ref())
        .status()
        .with_context(|| format!("Failed to spawn '{}'.", program))?;

    std::process::exit(status.code().unwrap_or(1));
}

fn shims_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".cow").join("shims"))
}

/// Detect the package manager from lockfiles in `path`.
fn detect_pm(path: &Path) -> Option<&'static str> {
    if path.join("pnpm-lock.yaml").exists() {
        return Some("pnpm");
    }
    if path.join("yarn.lock").exists() {
        return Some("yarn");
    }
    if path.join("bun.lockb").exists() || path.join("bun.lock").exists() {
        return Some("bun");
    }
    if path.join("package-lock.json").exists() {
        return Some("npm");
    }
    None
}

/// Write a shim script for `pm` into `shims_dir` if it does not already exist
/// or its content has changed.
fn ensure_shim(pm: &str, shims_dir: &Path) -> Result<()> {
    let content = match shim_content(pm) {
        Some(c) => c,
        None => return Ok(()),
    };

    std::fs::create_dir_all(shims_dir).context("Failed to create shims directory")?;

    let shim_path = shims_dir.join(pm);
    if shim_path.exists() {
        let existing = std::fs::read_to_string(&shim_path).unwrap_or_default();
        if existing == content {
            return Ok(());
        }
    }

    std::fs::write(&shim_path, content)
        .with_context(|| format!("Failed to write shim for {}", pm))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&shim_path)
            .with_context(|| format!("Failed to read shim metadata for {}", pm))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&shim_path, perms)
            .with_context(|| format!("Failed to set permissions on shim for {}", pm))?;
    }

    Ok(())
}

fn shim_content(pm: &str) -> Option<&'static str> {
    match pm {
        "npm" => Some(
            "#!/bin/sh\n\
             case \"$1\" in\n\
             \x20 install|i|add|remove|uninstall|rm|r|ci)\n\
             \x20   exec /usr/bin/env npm --prefix \"$COW_PASTURE_PATH\" \"$@\"\n\
             \x20   ;;\n\
             \x20 *)\n\
             \x20   exec /usr/bin/env npm \"$@\"\n\
             \x20   ;;\n\
             esac\n",
        ),
        "pnpm" => Some(
            "#!/bin/sh\n\
             case \"$1\" in\n\
             \x20 install|i|add|remove|rm|uninstall|up|update|prune)\n\
             \x20   exec /usr/bin/env pnpm --dir \"$COW_PASTURE_PATH\" \"$@\"\n\
             \x20   ;;\n\
             \x20 *)\n\
             \x20   exec /usr/bin/env pnpm \"$@\"\n\
             \x20   ;;\n\
             esac\n",
        ),
        "yarn" => Some(
            "#!/bin/sh\n\
             # Yarn v1: redirect install subcommands with --modules-folder.\n\
             # Yarn Berry (v2+): installs to cwd/node_modules natively — pass through.\n\
             if [ -f \".yarnrc.yml\" ]; then\n\
             \x20 exec /usr/bin/env yarn \"$@\"\n\
             fi\n\
             case \"$1\" in\n\
             \x20 install|add|remove)\n\
             \x20   exec /usr/bin/env yarn --modules-folder \"$COW_PASTURE_PATH/node_modules\" \"$@\"\n\
             \x20   ;;\n\
             \x20 *)\n\
             \x20   exec /usr/bin/env yarn \"$@\"\n\
             \x20   ;;\n\
             esac\n",
        ),
        "bun" => Some(
            "#!/bin/sh\n\
             # bun uses the nearest package.json directory (cwd = pasture) — pass through.\n\
             exec /usr/bin/env bun \"$@\"\n",
        ),
        _ => None,
    }
}

use anyhow::{bail, Result};
use std::path::PathBuf;

const ZSH_SNIPPET: &str = r#"
# cow shell integration
autoload -Uz compinit && compinit
function cowcd() { cd "$(cow cd "$1")"; }
function _cowcd() { compadd $(cow list --json 2>/dev/null | jq -r '.[].name' 2>/dev/null); }
(( ${+functions[compdef]} )) && compdef _cowcd cowcd
"#;

const BASH_SNIPPET: &str = r#"
# cow shell integration
function cowcd() { cd "$(cow cd "$1")"; }
function _cowcd() { COMPREPLY=($(compgen -W "$(cow list --json 2>/dev/null | jq -r '.[].name' 2>/dev/null)" -- "${COMP_WORDS[1]}")); }
complete -F _cowcd cowcd
"#;

pub fn run() -> Result<()> {
    let shell = std::env::var("SHELL").unwrap_or_default();

    let (rc_path, snippet) = if shell.ends_with("zsh") {
        let path = dirs::home_dir()
            .map(|h| h.join(".zshrc"))
            .unwrap_or_else(|| PathBuf::from("~/.zshrc"));
        (path, ZSH_SNIPPET)
    } else if shell.ends_with("bash") {
        let path = dirs::home_dir()
            .map(|h| h.join(".bashrc"))
            .unwrap_or_else(|| PathBuf::from("~/.bashrc"));
        (path, BASH_SNIPPET)
    } else if shell.is_empty() {
        bail!("Cannot detect shell. Set $SHELL or add the snippet to your shell config manually.");
    } else {
        bail!(
            "Shell '{}' is not supported. Add the snippet to your shell config manually:\n{}",
            shell, ZSH_SNIPPET
        );
    };

    let existing = std::fs::read_to_string(&rc_path).unwrap_or_default();
    if existing.contains("cowcd") {
        println!(
            "cow shell integration is already installed in {}.",
            rc_path.display()
        );
        return Ok(());
    }

    std::fs::write(&rc_path, format!("{}{}", existing, snippet))?;

    println!("Added cow shell integration to {}.", rc_path.display());
    println!("Run 'source {}' to activate.", rc_path.display());
    Ok(())
}

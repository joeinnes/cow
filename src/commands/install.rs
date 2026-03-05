use anyhow::{bail, Result};
use std::path::PathBuf;

const ZSH_SNIPPET: &str = r#"
# cow shell integration (v2)
autoload -Uz compinit && compinit
function cowcd() { cd "$(cow cd "$1")"; }
function _cow_pasture_names() { cow list --json 2>/dev/null | jq -r '.[].name' 2>/dev/null; }
function _cow() {
  local subcmds=(create list status diff extract remove sync cd run materialise fetch-from recreate migrate install mcp)
  local name_cmds=(status diff extract remove cd run materialise fetch-from recreate)
  if (( CURRENT == 2 )); then
    compadd -- $subcmds
  elif (( CURRENT == 3 )) && [[ ${words[2]} == (${(j:|:)name_cmds}) ]]; then
    compadd -- $(_cow_pasture_names)
  fi
}
compdef _cow cow
function _cowcd() { compadd -- $(_cow_pasture_names); }
compdef _cowcd cowcd
"#;

const BASH_SNIPPET: &str = r#"
# cow shell integration (v2)
function cowcd() { cd "$(cow cd "$1")"; }
_cow_pasture_names() { cow list --json 2>/dev/null | jq -r '.[].name' 2>/dev/null; }
function _cow() {
  local subcmds="create list status diff extract remove sync cd run materialise fetch-from recreate migrate install mcp"
  local name_cmds="status diff extract remove cd run materialise fetch-from recreate"
  if (( COMP_CWORD == 1 )); then
    COMPREPLY=($(compgen -W "$subcmds" -- "${COMP_WORDS[1]}"))
  elif (( COMP_CWORD == 2 )) && [[ " $name_cmds " == *" ${COMP_WORDS[1]} "* ]]; then
    COMPREPLY=($(compgen -W "$(_cow_pasture_names)" -- "${COMP_WORDS[2]}"))
  fi
}
complete -F _cow cow
function _cowcd() { COMPREPLY=($(compgen -W "$(_cow_pasture_names)" -- "${COMP_WORDS[1]}")); }
complete -F _cowcd cowcd
"#;

// Marker unique to v2 snippet — used to detect whether the current version is installed.
const INSTALLED_MARKER: &str = "# cow shell integration (v2)";

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
    if existing.contains(INSTALLED_MARKER) {
        println!(
            "cow shell integration is already installed in {}.",
            rc_path.display()
        );
        return Ok(());
    }

    if existing.contains("cow shell integration") {
        println!(
            "An older version of cow shell integration is in {}.",
            rc_path.display()
        );
        println!("Remove the old '# cow shell integration' block and re-run to upgrade.");
        return Ok(());
    }

    std::fs::write(&rc_path, format!("{}{}", existing, snippet))?;

    println!("Added cow shell integration to {}.", rc_path.display());
    println!("Run 'source {}' to activate.", rc_path.display());
    Ok(())
}

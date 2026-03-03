mod app;
mod config;
mod editor;
mod file_tree;
mod highlight;
mod ui;

use std::path::PathBuf;
use anyhow::Result;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

fn print_help() {
    println!(
        "macro {VERSION} — {DESCRIPTION}

Usage:
  macro [OPTIONS] [PATH]

Arguments:
  PATH    File or directory to open.
          File   → opens it in editor with file tree rooted at its parent.
          Dir    → opens the directory as the file tree root.
          (none) → opens the current working directory.

Options:
  -h, --help       Print this help message and exit.
  -v, --version    Print version and exit.

Keybindings:
  Ctrl+S           Save current file
  Ctrl+Q           Close active file / quit if only tree is open
  Ctrl+Shift+Q     Force close without saving
  Ctrl+C / Ctrl+X  Copy / Cut selection
  Ctrl+V           Paste
  Ctrl+A           Select all
  Tab              Switch focus between file tree and editor
  Enter (tree)     Open file / expand directory
  Mouse            Click to navigate, scroll wheel to scroll

Config: ~/.config/macro/config.toml"
    );
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    for arg in &args {
        match arg.as_str() {
            "-h" | "--help" => { print_help(); return Ok(()); }
            "-v" | "--version" => { println!("macro {VERSION}"); return Ok(()); }
            _ => {}
        }
    }

    let path = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let mut app = app::App::new(path)?;
    app.run()
}

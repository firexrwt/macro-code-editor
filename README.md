# macro

A fast terminal text editor written in Rust. Designed as a replacement for `micro` with better performance and a built-in file tree.

## Features

- Integrated file tree — full screen when no file is open, split view when a file is open
- Syntax highlighting for all major languages (via syntect, pure Rust)
- Mouse support — click to position cursor, scroll wheel, click to navigate tree
- Line numbers
- Config file at `~/.config/macro/config.toml`

## Usage

```
macro [PATH]
```

- `macro` — open current directory as file tree
- `macro /path/to/file` — open file in editor
- `macro /path/to/dir` — open directory as file tree root
- `macro --help` — show help
- `macro --version` — print version

## Keybindings

| Key | Action |
|---|---|
| `Ctrl+S` | Save current file |
| `Ctrl+Q` | Close active file / quit if only tree is open |
| `Ctrl+Q` × 2 | Force close unsaved file (second press confirms discard) |
| `Ctrl+C` | Copy selection |
| `Ctrl+X` | Cut selection |
| `Ctrl+V` | Paste |
| `Ctrl+A` | Select all |
| `Tab` | Switch focus between file tree and editor |
| `Shift+Arrow` | Extend selection |
| `Home` / `End` | Move to start / end of line |
| `Page Up/Down` | Scroll by 20 lines |
| `Enter` (tree) | Open file / expand-collapse directory |
| `Arrow keys` | Navigate |

## Installation

### From source

```bash
git clone https://github.com/firexrwt/macro-code-editor.git
cd macro-code-editor
cargo build --release
sudo install -Dm755 target/release/macro /usr/local/bin/macro
```

### Fedora (COPR)

```bash
sudo dnf copr enable firexrwt/macro-editor
sudo dnf install macro-editor
```

## Configuration

Create `~/.config/macro/config.toml`:

```toml
theme = "base16-ocean.dark"
tab_size = 4
line_numbers = true
tree_width = 30
```

Available themes: `base16-ocean.dark`, `base16-eighties.dark`, `base16-mocha.dark`, `base16-ocean.light`, `InspiredGitHub`, `Solarized (dark)`, `Solarized (light)`.

## Building for COPR

See `packaging/README-copr.md`.

## License

MIT

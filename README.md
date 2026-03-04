# macro

A fast terminal text editor written in Rust. Designed as a replacement for `micro` with better performance, a built-in file tree, and LSP autocomplete.

## Features

- Integrated file tree — full screen when no file is open, split view when a file is open
- Syntax highlighting for all major languages (via syntect, pure Rust)
- LSP autocomplete — triggers automatically while typing, uses system language servers
- Tabs — open up to 8 files simultaneously, switch by clicking
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
| `Ctrl+Q` × 2 | Force close unsaved file |
| `Ctrl+C` | Copy selection |
| `Ctrl+X` | Cut selection |
| `Ctrl+V` | Paste |
| `Ctrl+A` | Select all |
| `Tab` | Apply completion / insert indent if no popup |
| `Esc` | Close completion popup / return focus to file tree |
| `↑ ↓` | Navigate completion popup |
| `Shift+Arrow` | Extend selection |
| `Home` / `End` | Move to start / end of line |
| `Page Up/Down` | Scroll by 20 lines |
| `Enter` (tree) | Open file / expand-collapse directory |
| Mouse click (tab bar) | Switch between open files |

## LSP Autocomplete

Completions appear automatically while typing. The editor uses system-installed language servers — install what you need:

| Language | Server | Install |
|---|---|---|
| Rust | `rust-analyzer` | `dnf install rust-analyzer` |
| Python | `pylsp` | `pip install python-lsp-server` |
| C / C++ | `clangd` | `dnf install clang-tools-extra` |
| Go | `gopls` | `go install golang.org/x/tools/gopls@latest` |
| JS / TS | `typescript-language-server` | `npm i -g typescript-language-server typescript` |

If a language server is not installed, the editor works normally without completions.

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
auto_complete = true
```

Available themes: `base16-ocean.dark`, `base16-eighties.dark`, `base16-mocha.dark`, `base16-ocean.light`, `InspiredGitHub`, `Solarized (dark)`, `Solarized (light)`.

## License

MIT

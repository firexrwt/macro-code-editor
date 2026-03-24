# AGENTS.md — AI Agent Guide for macro-editor

This file gives AI coding agents (Claude Code, Codex, Cursor, etc.) the context
needed to contribute effectively to this project.

## Project overview

`macro` is a fast terminal text editor written in Rust, built as a replacement
for `micro`. It uses **ratatui + crossterm** for TUI rendering and **syntect**
for incremental syntax highlighting.

Binary name: `macro`
Config: `~/.config/macro/config.toml`

## How to build and test

```sh
cargo build            # debug build
cargo build --release  # optimised release build
cargo test             # run all unit tests (29 tests in src/editor.rs)
```

Install to `~/.local/bin`:

```sh
install -Dm755 target/release/macro ~/.local/bin/macro
```

## Repository structure

| Path | Purpose |
|------|---------|
| `src/main.rs` | Entry point, `--help` / `--version` |
| `src/app.rs` | Main event loop, keyboard + mouse handling, LSP glue |
| `src/editor.rs` | Text buffer, cursor, selection, clipboard — **all unit tests live here** |
| `src/ui.rs` | ratatui rendering |
| `src/highlight.rs` | syntect wrapper, `HighlightCache` for incremental highlighting |
| `src/file_tree.rs` | File tree widget |
| `src/config.rs` | Config loading/saving |
| `src/lsp.rs` | LSP client (stdio JSON-RPC) |
| `packaging/macro-editor.spec` | RPM spec for Fedora COPR |

## Code conventions

- **All comments must be in English.** No other language, no exceptions.
- Tests live in `src/editor.rs` under `#[cfg(test)]`. There is no separate
  integration test crate (no `lib.rs` — only a binary).
- Add tests for any new `Editor` logic. Run `cargo test` before opening a PR.
- Do not add `unwrap()` in non-test code without a comment explaining why it
  cannot panic.
- Keep `syntect` with `default-features = false, features = ["default-fancy"]`
  — pure Rust, no C dependencies, required for clean COPR/Koji builds.
- Version bumps go in both `Cargo.toml` and `packaging/macro-editor.spec`
  (including a changelog entry).

## Architecture notes

### Incremental syntax highlighting
`HighlightCache` stores per-line `ParseState` + `HighlightState` snapshots.
When a line is edited, `Editor::mark_dirty(line)` sets `dirty_from_line`.
On each frame, `ui.rs` calls `Highlighter::highlight_from()` which resumes
from the dirty line and exits early once the `ParseState` converges with the
cached state (meaning later lines are unaffected).

### Mouse handling
Mouse events are handled in `App::handle_mouse()`:
- `Down(Left)` → move cursor / click tab / click tree row
- `Drag(Left)` → `Editor::drag()` — extends selection from click anchor
- `ScrollUp/Down` → move cursor 3 lines
- `ScrollLeft/Right` → `Editor::scroll_left/right_cols(3)` — move cursor by columns

### Selection model
`Selection { anchor: Pos, cursor: Pos }` — anchor is fixed at selection start,
cursor moves. `Selection::normalized()` always returns `(start, end)` in
document order. Shift+arrows, Ctrl+A, and mouse drag all produce selections.

## What to avoid

- Do not use Python scripts or shell one-liners to edit source files — use
  proper Rust code changes.
- Do not add C dependencies (breaks COPR builds).
- Do not introduce `unsafe` without discussion.
- Do not change the binary name from `macro`.

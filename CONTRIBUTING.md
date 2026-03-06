# Contributing to macro

Thank you for your interest in contributing. Please read this document before opening a PR.

---

## Getting started

```bash
git clone https://github.com/firexrwt/macro-code-editor.git
cd macro-code-editor
cargo build --release
```

The binary ends up at `target/release/macro`. Install it locally with:

```bash
install -Dm755 target/release/macro ~/.local/bin/macro
```

---

## Branches

- `master` - stable, always buildable. Direct pushes only for maintainers.
- Feature branches - name them `feature/short-description` or `fix/short-description`.
- Open a PR against `master` when ready.

---

## Commit messages

Keep them short and in English:

```
add incremental syntax highlighting
fix: completion popup dismissed on backspace
bump version to 0.3.1
```

No issue references required for small fixes. For larger changes, briefly explain *why* in the commit body.

---

## Code style

- Standard `rustfmt` formatting. Run `cargo fmt` before committing.
- No `clippy` warnings - run `cargo clippy` and fix what it flags.
- Match the existing comment style: short, in Russian or English, only where the logic isn't obvious.
- No dead code, no unused imports, no commented-out blocks.

---

## Performance rules

This project exists as a faster alternative to `micro`. Performance regressions are treated as bugs.

- **No full-file recomputes on single-line edits.** The highlight cache exists for a reason.
- **No per-frame allocations in the render path** (`ui.rs`). Reuse, cache, or pre-compute.
- **No redundant redraws.** The event loop runs at 16 ms ticks - keep the draw pass cheap.
- If your change touches `ui.rs`, `highlight.rs`, or `editor.rs`, explain in the PR why it doesn't make things slower.

---

## Testing

There are unit tests in `src/editor.rs`. Run them with:

```bash
cargo test
```

- Don't break existing tests.
- If you add new editor logic, add a test for it.
- Manual testing is mandatory: open a real file, use the feature, try to break it.

---

## What belongs in a PR

- One feature or one fix per PR.
- No speculative refactoring unrelated to the PR goal.
- No extra abstractions "for the future".
- No formatting-only changes mixed with logic changes.

---

## AI-assisted code

Contributions written with AI tools (Claude Code, Opencode, Codex, Copilot, Cursor, or any other agent or assistant) are **allowed**, but are held to a higher review standard.

If your PR contains AI-generated or AI-assisted code, you **must**:

1. **Read every line yourself.** You are responsible for the code, not the model.
2. **Understand what it does.** If you can't explain a section of code in plain words, don't submit it.
3. **Verify correctness.** AI produces plausible-looking but subtly broken logic more often than it seems.
4. **Check for regressions.** Run the editor, use the affected feature, make sure unrelated features still work.
5. **Check performance.** AI tends to add allocations, redundant computations, and unnecessary abstractions. Review against the performance rules above.
6. **Strip the bloat.** Remove anything the PR doesn't actually need - extra error handling for impossible cases, helper functions used once, over-engineered generics.
7. **Label it.** Add `AI-assisted` to the PR description.

PRs where AI-generated code has clearly not been reviewed will be closed without merge.

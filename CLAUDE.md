# Agent notes

## Commands

Keep output short. Use these exact commands, never plain `cargo build` or `cargo test`:

- Check: `cargo check --all-targets --message-format=short 2>&1 | head -40`
- Lint: `cargo clippy --all-targets --message-format=short`
- Test: `RUST_BACKTRACE=0 cargo nextest run --profile agent --hide-progress-bar --cargo-quiet [filter]`
- Long files: `scripts/long-files.sh` (lists files over 400 lines; split them when you work in them)

Filter tests to the module you touched (e.g. `engine::`) unless the change is broad.
A hook runs `cargo check` after each edit to a `.rs` file and reports only failures.

## Rules

- Keep clippy at zero warnings. Fix new warnings rather than allowing them; use
  `#[expect(..., reason = "...")]` only where the lint is wrong for that code.
  Functions marked "predates the size lints" are a baseline: shrink them when you touch them.
- Don't open `Cargo.lock` or anything under `target/`.

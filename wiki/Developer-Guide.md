# Developer guide

This page is a summary. The full architecture notes and design documents are in the repository, in [`docs/development/`](https://github.com/marcinkoza0922/padwight/tree/main/docs/development):

- [architecture.md](https://github.com/marcinkoza0922/padwight/blob/main/docs/development/architecture.md): processes, data flow, module map, and where to start reading.
- [specs.md](https://github.com/marcinkoza0922/padwight/blob/main/docs/development/specs.md): games and packs.
- [layers.md](https://github.com/marcinkoza0922/padwight/blob/main/docs/development/layers.md), [sticks.md](https://github.com/marcinkoza0922/padwight/blob/main/docs/development/sticks.md), [guide.md](https://github.com/marcinkoza0922/padwight/blob/main/docs/development/guide.md): design notes for individual features.

## Processes

One binary, `padwight`, runs as several processes:

- **Settings window** (`padwight`, `padwight gui`). Talks to the daemon over IPC. If no daemon is running, it edits the config file directly.
- **Daemon** (`padwight daemon`). The only process that grabs controllers and writes to uinput.
- **Overlay** (`padwight overlay`). Started by the daemon, and kept running invisibly between uses.
- **One-shot commands** (`padwight status`, `profile`, `reload`, …). Send one request and exit.

The GUI and the CLI talk to the daemon through `$XDG_RUNTIME_DIR/padwight.sock`.

## Where things are

- `src/input.rs`: physical controller input, normalized.
- `src/daemon.rs`: routing. Decides whether an event goes to an open menu, the overlay, or the mapping engine.
- `src/engine.rs` and `src/engine/`: mapping for the active profile, including sticks, flick stick and touchpad.
- `src/output.rs`: virtual uinput devices.
- `src/focus/`: desktop focus tracking.
- `src/config.rs` and `src/config/`: the config types, defaults and migration.
- `src/pack.rs`, `src/library.rs`: packs and the built-in library.
- `src/gui.rs` and `src/gui/`: the settings window.

## Adding a pack to the library

Add a `.padpack` file to `packs/`. `build.rs` embeds it, and `cargo test` checks it. The checks require a unique ID and name, no dangling references, and at least one profile that needs nothing beyond a plain pad. See [`packs/README.md`](https://github.com/marcinkoza0922/padwight/blob/main/packs/README.md).

## Checks

| Command | What it does |
|---|---|
| `cargo check --all-targets` | Compiles everything, tests included. |
| `cargo clippy --all-targets` | Lints. Clippy is kept at zero warnings. |
| `cargo test` | All unit tests, including the library pack checks. |
| `cargo nextest run` | The same tests under nextest. |
| `cargo test -- --ignored rumble` | End-to-end rumble check. Needs `/dev/uinput`, so it's opt-in. |

Format only the code you change. The codebase isn't rustfmt-formatted as a whole, so running `cargo fmt` rewrites many unrelated files.

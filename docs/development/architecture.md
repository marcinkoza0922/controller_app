# Architecture

This is for people working on the code. For what the app does, see the [README](../../README.md). The user-facing docs are the [pack format reference](../pack-format.md) and the [pack tutorial](../tutorial-deus-ex-pack.md).

## Processes

One binary, `padwight`, runs as several processes. `main.rs` picks the role from the first argument:

| Command | Role | Notes |
|---|---|---|
| `padwight` / `padwight gui` | Settings window (iced) | Talks to the daemon over IPC. If no daemon is running, it edits `config.toml` directly. |
| `padwight daemon` | The long-running service | The only process that grabs controllers and writes to uinput. The systemd unit in `dist/padwight.service` runs this. |
| `padwight overlay` | On-screen overlay (layer-shell surface) | Spawned by the daemon (`daemon.rs`, `current_exe()` + `overlay`). It stays running invisibly between uses so menus appear instantly. |
| `padwight status`, `profile <name>`, `reload`, `menu <name>`, … | One-shot CLI | Sends one request to the daemon and exits. |

Everything the daemon knows about the desktop comes from focus trackers (see below). Everything the GUI shows comes from the daemon or from the config file.

## Data flow

```
 evdev (physical pad)                              uinput (virtual devices)
        │                                                  ▲
        ▼                                                  │
  input.rs ── normalized InputEvent ──► daemon.rs ── OutEvent ──► output.rs
  (+ input/touchpad.rs)                  │    ▲                (Xbox 360-style pad,
                                         │    │                 keyboard, mouse)
                       ┌─────────────────┤    │
                       ▼                 ▼    │
                 menu.rs / offer.rs   engine.rs (per device)
                 overlay.rs           ├─ engine/stick.rs   sticks, zones, mouse/scroll ticks
                 (route input while   ├─ engine/flick.rs  flick stick
                  a menu/overlay      └─ engine/touchpad.rs
                  is up)
                       ▲
                       │ focus changes
        focus/ (kwin, sway, hyprland, identify.rs → /proc)

 gui.rs ◄──── IPC: $XDG_RUNTIME_DIR/padwight.sock (ipc.rs, one JSON line per request)
   │
   └──── ~/.config/padwight/config.toml  (daemon is the only writer while it runs)
```

1. **Input.** `input.rs` reads each physical pad's evdev node and turns raw events into normalized buttons (following the Linux gamepad spec), sticks in −1..1, and triggers in 0..1. Touchpads have their own node (`input/touchpad.rs`) and reach the engine like buttons.
2. **Daemon.** `daemon.rs` grabs the physical devices so nothing else sees them, keeps one `engine` state per controller, and decides who gets each event: an open menu (`menu.rs`), the on-screen keyboard or numpad (`overlay.rs`), a library-game offer (`offer.rs`), or the mapping engine.
3. **Engine.** `engine.rs` turns normalized input into output for the active profile: buttons, gestures (double/triple tap, long press), combos, toggles, turbo, macros, layers, and zones. `engine/stick.rs` handles the sticks' continuous output (mouse, scroll, rings), `engine/flick.rs` the flick stick, and `engine/touchpad.rs` the touchpad.
4. **Output.** `output.rs` writes to virtual uinput devices: an Xbox 360-style pad, a keyboard and a mouse. `rumble.rs` mirrors force-feedback uploads from games back to the physical pad.
5. **Focus.** `focus/` finds the focused window and its game, and the daemon picks the profile from the game's rules (see [Focus tracking](#focus-tracking)).

Stick output and timers (tap windows, long presses, turbo) run on the daemon's tick, not on input events alone, so a held stick keeps moving without new events.

## Module map

Line counts are approximate and change; `scripts/long-files.sh` lists the files over 400 lines.

### Core

| Path | Responsibility |
|---|---|
| `main.rs` | Command-line dispatch. |
| `config.rs` (and `config/`) | The config types: buttons, actions, profiles, games, rules, layers, menus, overlays, gyro, and the load/save/migration code. `config/summary.rs` writes short descriptions for the UI and logs. `config/touchpad.rs`, `config/log.rs` hold the touchpad and log-overlay settings. |
| `ipc.rs` | The `Request` and `Response` enums and the socket client/server helpers. |
| `daemon.rs` (and `daemon/`) | The service: device discovery and grabbing, the main loop, routing, and output dispatch. `daemon/logs.rs` records the input log; `daemon/touchpad.rs` runs touchpad readers. |
| `monitor.rs` | The one-line live status shown when the daemon runs in a terminal. |

### Input and mapping

| Path | Responsibility |
|---|---|
| `input.rs`, `input/touchpad.rs` | evdev → normalized input. Handles xpad's swapped X/Y labels. |
| `engine.rs` | Per-device mapping state for the active profile. One of the largest files; its tests are in the same file. |
| `engine/stick.rs` | Sticks: gamepad feeds, zones, mouse and scroll output, rings. |
| `engine/flick.rs` | Flick stick. |
| `engine/touchpad.rs` | Touchpad click and finger-to-mouse movement. |
| `menu.rs` | On-screen menus: all controller input while one is open. |
| `offer.rs` | The "add this game from the library?" panel when Guide is tapped over an unknown game. |
| `system_menu.rs` | The Guide + Start menu over the game: Quick Settings and Edit Controls. See [in-game-editor.md](in-game-editor.md). |
| `layout_editor.rs` | The Edit Controls page of that menu: button actions of the active profile. |
| `rumble.rs` | Forwards rumble from the virtual pad to the physical one. |
| `output.rs` | The uinput devices. |
| `quit.rs` | ForceQuit: ends the focused game's process tree, never the desktop. |
| `capture.rs`, `record.rs` | Screenshot and screen recording, using whichever tool the desktop has. |
| `media.rs` | MPRIS media controls on a worker thread, so a stuck player can't stall the daemon. |
| `keyboard.rs` | The on-screen key picker used in the GUI. |

### Focus

| Path | Responsibility |
|---|---|
| `focus/mod.rs` | Picks the active game from the focus event and the rules. Falls back to matching running processes when no tracker exists. |
| `focus/kwin.rs`, `focus/dbus.rs` | KDE Plasma: a KWin script reports focus changes over D-Bus. |
| `focus/sway.rs` | Sway (and i3-IPC compositors) over `$SWAYSOCK`. |
| `focus/hyprland.rs` | Hyprland's event and command sockets. |
| `focus/identify.rs` | Identifies a window's executable, Steam App ID and class from `/proc`, including Wine/Proton `.exe` names. |
| `launchers.rs` | Which games are installed (Steam, Heroic, Lutris) or running, for the library picker. |

### Overlays and display

| Path | Responsibility |
|---|---|
| `overlay.rs` (and `overlay/draw/`) | The overlay process: keyboard, numpad, menus, info and log panels. `OverlayController` owns the state inside the daemon. |
| `info.rs` | Info overlays: `{token}` expansion into button glyphs and live values, and the generated bindings sheet for layers. |
| `font.rs` | Bundled fonts (`assets/fonts`) and installed families by name. |
| `style.rs` | Shared widget styles for the GUI. |
| `pad_svg.rs` | The SVG drawing of a live controller. Text is drawn by the GUI on top. |
| `inputlog.rs`, `inputlog/` | The input log: each press and what it did, grouped into sequences for log overlays. |

### Packs and library

| Path | Responsibility |
|---|---|
| `pack.rs` | `.padpack` parse, check, export, import planning and apply, versioning (`FORMAT`). Includes most of the pack tests. |
| `library.rs` | The built-in library, embedded at build time, and library update detection. Its checks run in `cargo test`. |
| `build.rs` | Embeds every `packs/*.padpack` into the binary. |
| `packs/` | The shipped library packs. |

### GUI (`gui.rs` and `gui/`)

| Path | Responsibility |
|---|---|
| `gui.rs` | The iced application shell: state, messages, and the daemon connection. The sidebar and game pages are in `gui/games.rs`. |
| `gui/profile.rs` | The profile editor (buttons, sticks and triggers, combos, gyro) and the controller drawing. Shared with the Layers tab. |
| `gui/actions.rs` | The action editor, including nested Multi, Toggle and Turbo. |
| `gui/layers.rs` | The Layers tab. |
| `gui/items.rs` | Macros, menus and info overlays: cards and editors. |
| `gui/games.rs` | Sidebar, a game's page and Details, Settings, controller list. |
| `gui/packs.rs` | Library picker, import preview, export, delete, copy-from-another-game. |
| `gui/checks.rs` | What "Save & apply" checks before writing: names, references, and rules. |
| `gui/widgets.rs`, `gui/tracking.rs`, `gui/overlays.rs`, `gui/ring_preview.rs`, `gui/logs.rs` | Shared widgets and smaller tabs. |

## Configuration and state

- **File.** `~/.config/padwight/config.toml` (`Config::path()`). The shape is the in-memory `Config`: app-wide settings, the built-in **General** game, **Shared** items, and the list of **games**.
- **Single writer.** While the daemon runs, it is the only process that writes the file. The GUI sends `SetConfig` over IPC, and the daemon saves. After a hand edit, `padwight reload` tells the daemon to re-read it.
- **Unsaved edits.** The GUI keeps edits in memory until **Save & apply**. Revert drops them.
- **Migration.** Old configs are converted on first load; the original is kept as `config.toml.old` (or `.old.2`, …). `Config::ensure_guide_layer` adds the Guide layer to games that lack one.
- **Names.** Items are unique by kind within a game. Game items can't reuse a Shared item's name. See [specs.md](specs.md).

## Packs

A pack is a game in a file: TOML in the same shapes as the config, extension `.padpack`. `pack.rs` owns it:

- `FORMAT` is the version this build writes. Newer formats are refused with a message; older formats are upgraded on load. Bump it only for breaking shape changes, and document what changed in the comment above it.
- `parse` is strict (`deny_unknown_fields` on the top level). A malformed pack fails with the line and error, never half-imports.
- `check` rejects dangling references (macros, menus, info overlays, layers, profiles named by rules).
- `export` copies in the Shared items a game uses, and keeps the pack ID on re-export.
- `plan` and `apply` do an import: they report name clashes and rule clashes, and `apply` writes the chosen result into the config.

Library packs are read at build time (`build.rs`) and checked by `cargo test` (`library.rs`). Adding a file to `packs/` is enough for it to appear in the picker; the tests say what it must satisfy.

## Testing and checks

| Command | What it does |
|---|---|
| `cargo check --all-targets` | Compiles everything, tests included. |
| `cargo clippy --all-targets` | Lints. The crate sets extra lints in `Cargo.toml` (`too_many_lines`, `cognitive_complexity`, and others). |
| `cargo test` | All unit tests, including every library pack check (`library.rs`, `pack.rs`). |
| `cargo nextest run` | The same tests under nextest. `.config/nextest.toml` has a quiet `agent` profile. |
| `cargo test -- --ignored rumble` | The end-to-end rumble check. It needs `/dev/uinput`, so it is opt-in. |

A few conventions to know:

- **Formatting.** The code is not rustfmt-formatted as a whole, so running `cargo fmt` rewrites many unrelated files. Format only what you change, by hand.
- **File size.** Files over about 400 lines are candidates for splitting. `scripts/long-files.sh` lists them. Several of the largest (`config.rs`, `daemon.rs`, `engine.rs`, `gui/profile.rs`) are already over that, so put new code in a submodule rather than adding to them.
- **Warnings.** Clippy is kept at zero warnings. Fix a new warning rather than allowing it; use `#[expect(..., reason = "...")]` only where the lint is wrong for that code.
- **Agents.** `CLAUDE.md` holds the commands and rules that automated agents in this repo follow.

## Focus tracking

The daemon picks the profile from the focused window. The tracker is chosen by the desktop:

1. **KDE Plasma** (`kwin.rs`): a small KWin script sends each focus change to the daemon's D-Bus service (`dbus.rs`).
2. **Sway** (`sway.rs`): subscribes to window events on `$SWAYSOCK`.
3. **Hyprland** (`hyprland.rs`): follows the event socket and asks the command socket for details.
4. **Fallback**: no tracker. Rules apply while a matching process runs.

`identify.rs` resolves a window to an executable name, Steam App ID (from `SteamAppId`/`STEAM_COMPAT_APP_ID` or Proton's `steam_app_<id>` class) and window class. Wine/Proton games are matched by their `.exe`, not the Wine loader. GNOME is unsupported: Mutter has no layer-shell, so the overlays can't be shown.

## Where to start reading

- To follow one button press: `input.rs` → `daemon.rs` (routing) → `engine.rs` (button, gesture and combo handling) → `output.rs`.
- To change what a profile can do: `config.rs` (types and defaults), then `engine.rs`, then the editor in `gui/profile.rs`, then `pack.rs` if packs need it.
- To add a built-in action: `ButtonAction` in `config.rs`, its summary in `config/summary.rs`, the engine, the action picker in `gui/actions.rs`, and the pack tests in `pack.rs`.
- Design notes for larger features are in this folder: [specs.md](specs.md) for games and packs, [layers.md](layers.md), [sticks.md](sticks.md) and [guide.md](guide.md).

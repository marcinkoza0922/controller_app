# GUI polish

From a click-through of the GUI on the light theme (dark mode not checked).

## Clarity

- [x] **Enabled grey buttons look disabled.** `button::secondary` (53 uses) is a flat mid-grey with dark text, so "Duplicate", "Find by pressing", "+ Add game", "Test rumble" and "+ Rule" look like the disabled "Revert". Give it a lighter fill with a border, like the dropdowns in `src/style.rs`.
- [x] **Labels on the controller picture cross each other.** The right column in `src/pad_svg.rs` (`RIGHT_LABELS`) lists West before East and Start after South, and `place_labels` stacks labels in list order, so the X, Start and right-stick lines cross other buttons. Sort each column by the anchor's y.
- [x] **Long labels wrap on the controller picture.** "Menu “Augmentat…" wraps to two lines and runs into "Next profile". `place_labels` counts characters, but the box is narrower than that. Cut shorter or stop the box wrapping.
- [x] **"—" for a button that does nothing is cryptic**, especially "— +" on the picture and "— · Double tap: …" in summaries (`actions.rs`, `profile.rs`, `items.rs`). Use "Disabled", or "(nothing)" on the picture.
- [x] **Single-character keys are hard to spot** in button summaries (`;`, `]`, `'`). Show key names as keycap chips or in quotes. Also write "Keypad PLUS" / "Keypad SLASH" in normal case.
- [x] **Remapped buttons don't stand out from unchanged ones.** Rows that just pass the button through ("Pad A" on A) look like real changes. Dim them.

## Polish

- [x] **A red "Delete" on every closed card** (menus, macros, info overlays: `src/gui/items.rs`). Move it inside the open card, or make it a quiet ✕ that turns red on hover.
- [x] **"Recently focused" list formatting gaps** on the Details tab (`src/gui/games.rs`): a window with no title shows only its grey details line, an empty executable leaves a leading " · class …", and the app lists its own window.
- [x] **Empty tabs aren't consistent.** Layers has a bare button plus "This game has no layers."; Macros has a card with a one-line explanation and no empty-state text. Pick one pattern.
- [x] **The header and controller picture take a lot of height.** The title, remapping switch and profile picker repeat on every page, and the full controller picture (~330 px) sits above the button editor (and is also on Overview). Allow collapsing the picture on the Profiles tab.
- [x] **The Save & apply bar has its own left border** next to the sidebar's divider, making a double line. Start it flush with the content area.

# Performance

Measured on `perf-optimizations`; numbers and tools are in `docs/performance.md`. Ordered by
expected impact. Re-measure with `scripts/bench.sh` before and after each change.

## GUI

- [ ] **Every live input snapshot rebuilds the whole GUI.** `watch_input` (`src/gui.rs`) forwards each daemon snapshot as `Message::LiveInput`, and iced re-runs `view()` for every message, on any page. Measured: `view()` on the game page costs about 60 µs before layout or drawing, so the cost scales with the snapshot rate. Throttle the stream (about 30 fps is enough for the picture), or drop snapshots the current page doesn't draw.
- [ ] **Each edit re-serializes every item.** `pack::edited_items` (`src/pack.rs`) TOML-serializes and hashes every profile, macro, menu and overlay of the game, and it runs after each edit (`src/gui.rs`, the `self.edited = ...` line). Measured: about 74 µs for one game with four profiles. Hash only the changed item, or cache the hashes and invalidate them on edit.
- [ ] **`view_footer` deep-compares the config on every view.** `self.config != self.saved` (`src/gui.rs`) runs on each render, and `view_header` rebuilds the profile list each time. Not measured on its own. Keep a dirty flag that changes only in `update`, and cache the list.
- [ ] **Allocation dominates the view build.** About 35% of the view benchmark is `malloc`/`free`, and `str::to_lowercase` is about 6% (`scripts/profile.sh bench gui`). Find the `to_lowercase` callers, and avoid rebuilding strings and `KeyCode` parses (`ButtonAction::summary`, `src/gui/actions.rs`) for every view.
- [ ] **The controller picture: iced re-rasterization is the open question.** `pad_svg::render` itself is cheap (about 4.5 µs). But each live message builds a new `svg::Handle`, which gets a new cache id, so iced may re-parse and re-rasterize the SVG while a stick moves. Measure in the running app before changing it. If confirmed, reuse the handle unless the pressed buttons or quantized stick values change.
- [ ] **The 1 s status poll clones the full status.** `Request::Status` (`src/daemon.rs`, `status()`) copies `recent_windows` and the device list every second. Small; unmeasured.
- [ ] **The settings window is the biggest idle cost.** At rest it uses 1.2% CPU, 8.9 wakeups/s and 216 MB RSS (`docs/performance.md`). Find out what wakes it (the 1 s poll, the `watch_input` retry loop, or iced); each wakeup costs a redraw.

## Overlay process

- [ ] **The overlay process uses 186 MB while idle.** The daemon starts it at startup so menus appear instantly (`src/daemon.rs`, `ensure_overlay_process`). Check whether a smaller idle state, such as starting it on first use, is fast enough.

## Daemon

- [ ] **Process-scan fallback reads every user process every 2 s.** `scan_processes` → `focus::running_processes` (`src/focus/identify.rs`) reads `cmdline`, `exe` and `environ` for all of the user's processes. It runs only on the `ProcessScan` backend. Unmeasured. Cache per pid (keyed by start time from `/proc/<pid>/stat`), or read `environ` only for candidates that match a rule.
- [ ] **Overlay frames are rebuilt on every change.** `broadcast_overlay` → `overlay_frame` (`src/daemon.rs`) rebuilds the whole frame on each fade step (33 ms) and then compares it with `last_frame`. Unmeasured. Build the frame only when its inputs change, or send only what changed (for example, opacity during a fade).

## Measured cheap (low priority)

- Engine per event: 135–361 ns. Engine tick when idle: 64 ns. Tick with a stick held: 137 ns.
- `config.active()` with 40 games: 105 ns. It is called every 4 ms tick, but at 250 ticks per second that is about 26 µs of work a second, under 0.01% of a core.
- `log_input` → `thresholds` (`src/daemon/logs.rs`) runs per event batch. Not benchmarked; probably cheap like the engine.
- Daemon idle: 0.10% CPU, 0.5 wakeups/s.

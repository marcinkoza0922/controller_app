# Performance

How to measure the app, and what the current numbers are. Measured on branch
`perf-optimizations`, release build, Linux (Fedora 44, NVIDIA driver 595, X11/Wayland
session), on a machine with no gamepad connected.

## Tools

| Command | What it does |
| --- | --- |
| `scripts/bench.sh [filter]` | Timing benchmarks, release build, one line per case. `filter` is `gui` for the settings window only. |
| `scripts/profile.sh bench [filter]` | `perf` sample of the benchmarks, with the hottest symbols. The benchmark binary alone, so cargo and nextest are not in the report. |
| `scripts/profile.sh pid <pid> [s]` | `perf` call-sampled profile of a running process (default 10 s). |
| `scripts/profile.sh stats <pid> [s]` | CPU %, wakeups per second, RSS and threads over a window. Needs no perf. |

Benchmarks live in `src/bench.rs` (engine, config, pack hashing, controller picture) and
`src/gui/bench.rs` (the settings window's update and view). They are `#[ignore]`d, so the
normal `cargo nextest run` skips them. Profiling output goes to `target/profile/`.

Find a pid with `pgrep -a controller_app`. The processes are `controller_app daemon`,
`controller_app overlay` (spawned by the daemon) and `controller_app` (the settings window).

### What the benchmarks do and don't cover

- They time the code on one thread with no real device, display or compositor.
- `gui: view()` builds the iced widget tree. It does not include layout, rasterization or
  drawing on the GPU, so the real cost of a frame is higher. Those costs can only be
  measured by running the app with a controller attached.
- Nothing yet covers `overlay_frame`, `status()` or the process scan, because they need a
  running `Daemon`. Those are measured only by the idle figures below.

## Current numbers

### Benchmarks (release, per iteration)

| Case | Time |
| --- | --- |
| engine: button press + release | 135 ns |
| engine: stick move (one event) | 361 ns |
| engine: tick, idle (daemon ticks every 4 ms) | 64 ns |
| engine: tick, stick held | 137 ns |
| config: `active()` with 40 games | 105 ns |
| config: `active_ref()` with 40 games | 170 ns |
| pack: `item_hashes` for one game (4 profiles) | 74 µs |
| pad_svg: render, no input | 4.3 µs |
| pad_svg: render, live input | 4.5 µs |
| gui: `update(LiveInput)` | 25 ns |
| gui: `view()`, game page (Profiles tab) | 57–66 µs |
| gui: `view()`, overview page | 36–40 µs |
| gui: `update(LiveInput)` + `view()` | 55–61 µs |

Reading these: the daemon's per-event and per-tick work is well under a microsecond.
Building the settings window's view is the expensive part, at roughly 40–65 µs per frame
before any layout or drawing. Each live controller update re-runs the view, so 1 000
updates a second would spend about 6% of a core on view construction alone.

### Hottest symbols in the GUI view benchmark

From `scripts/profile.sh bench gui`:

- About 35% of the time is `malloc` and `free`, mostly from building the widget tree.
- `str::to_lowercase` is about 6%. Its callers are not pinned down yet.
- Float formatting (`format_shortest_opt`, `f32` Display) is about 5%.
- `pad_svg::fit_label`, `place_labels` and `controller_drawing` together are about 5%.
- `evdev` `KeyCode::from_str` is about 1.3%. It is called when a button's summary is
  built, and from validation in `src/gui/actions.rs`.

### Idle processes (no controller, no input)

| Process | CPU | Wakeups/s | RSS | Threads |
| --- | --- | --- | --- | --- |
| `controller_app daemon` | 0.10% | 0.5 | 14 MB | 7 |
| `controller_app overlay` | 0.30% | 0.0 | 186 MB | 32 |
| `controller_app` (settings window) | 1.20% | 8.9 | 216 MB | 34 |

Notes:

- The daemon is almost idle. Its 4 ms tick runs only when an engine needs it (`needs_tick`:
  a stick is deflected or a timer is pending), so the loop costs nothing at rest.
- The overlay process uses 186 MB of memory while idle. The daemon starts it at startup on
  purpose ("so menus appear instantly; it idles invisibly", `src/daemon.rs`). Its memory is
  the cost of that choice; a lighter idle state is untested.
- The settings window is the biggest idle cost. The 1 s status poll and the live-input
  reconnect loop (`watch_input`, retries every 1 s) are the likely source of the wakeups (not confirmed). The `perf`
  sample had only about 60 samples, so the percentages in its hottest symbols are rough:
  `memmove`, the NVIDIA GL driver, `drop_glue` of the widget row, and `wgpu`'s render-pass
  creation all show up.

## Not measured yet

- Settings window with a controller moving (live input at the daemon's real rate). The
  biggest open question.
- Cost of iced layout and rasterization for the controller picture. Each live update
  gives the SVG a new handle, so the GPU renderer may re-rasterize it every time.
- Daemon with a controller active: the process scan fallback, `overlay_frame` during
  fades, and the per-event path with real evdev reads.
- Memory growth over time (no long-run measurement yet).

## How to re-measure

    scripts/bench.sh                      # before and after each change
    scripts/bench.sh gui
    scripts/profile.sh bench gui          # where the view's time goes
    cargo run --release -- daemon &       # then, with the pid:
    scripts/profile.sh stats <pid> 30
    scripts/profile.sh pid <pid> 30

Benchmarks run on a shared desktop are noisy. Compare runs back to back, and repeat any
change under about 10% before trusting it.

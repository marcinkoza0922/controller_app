# QA plan

What to check beyond `cargo test`, and what a human reviewer should look at. Automated checks are listed with their status, so it's clear what runs in CI and what still needs a person.

## 1. Issues found and their status

| # | Issue | Status |
|---|---|---|
| F1 | `README.md` said the pack format is **8**; the code and `docs/pack-format.md` say **9**. | Fixed. Enforced by check A-2. |
| F2 | The IPC socket fell back to a file in the shared `/tmp` with no permissions set, so the daemon's control socket could be reached by other users. | Fixed. The socket is `0600`, in a directory the daemon checks is owned by this user and sets to `0700`. The `/tmp` fallback now uses a per-user directory. Covered by `daemon::tests::the_control_socket_is_private_to_this_user`. Not yet exercised on a machine without `XDG_RUNTIME_DIR`. |
| F3 | No CI: nothing ran check, clippy, the tests or the audit automatically. | Fixed in the repo with `.github/workflows/ci.yml`. **Not yet run on GitHub.** The apt package list is a best guess and may need changes on the first run. |
| F4 | Packs might be able to trigger host programs. | Corrected. A pack can't name an arbitrary program. The only host-program paths reachable from a pack are the built-in Screenshot and ToggleRecording actions, which run fixed tools (`grim` or a recorder). No code change; whether an import preview should list these is still a reviewer's call (H-SEC-2). |
| F5 | `unwrap()` calls on daemon paths could panic and leave the pad grabbed. | Audited. The daemon had one production `unwrap()`, in `release_devices`; it is now a `let ... else`. The remaining production `unwrap`/`expect` calls are invariants or bundled assets: `config.rs` (legacy conversion), `icon.rs` (the bundled icon) and `monitor.rs` (a mutex). |
| F6 | `scripts/long-files.sh` lists files over 400 lines. | Unchanged. It's a maintainability note, not a defect. |
| F7 | `flatpak/cargo-sources.json` could drift from `Cargo.lock`. | Checked. Enforced by check A-7; currently in sync. |

## 2. Automated checks

Scripted checks live in `scripts/check-project.py`, and the Rust tests use the commands in `CLAUDE.md`. CI runs all of them (see `.github/workflows/ci.yml`).

| # | Check | Status | Where |
|---|---|---|---|
| A-0 | CI on push and pull request | Done (not yet run on GitHub) | `.github/workflows/ci.yml` |
| A-1 | CLI commands in the docs exist | Done | `check-project.py`, against the dispatch in `src/main.rs` |
| A-2 | Pack format number agrees across code and docs | Done | `check-project.py` |
| A-3 | Links and `#anchors` resolve; GitHub `blob/main` links point at real files and headings | Done | `check-project.py` |
| A-4 | README and wiki don't contradict each other | Not automated: a review item. The wiki repeats README sections, so it's worth reading both when either changes. | Manual (section 3.6) |
| A-5 | Every shipped pack survives a save and reload unchanged | Done | `library::tests::every_library_pack_survives_a_save_and_load` |
| A-6 | Config migration: the backup is written, the converted file reloads unchanged, and a second load changes nothing | Done. `Config::load_from` takes a path, so the tests use a temporary directory and never touch the real config. Covers the `config.toml.old` backup (a byte-for-byte copy), a second load (no new backup, same file), and a second conversion (keeps the earlier backup, writes `.old.2`). The `before-guide` backup is not tested. | `config::tests::a_legacy_config_*`, `a_later_conversion_*` |
| A-7 | `cargo-sources.json` matches `Cargo.lock` | Done | `check-project.py` |
| A-8 | Damaged pack and config text is refused or handled without panicking | Done, as a deterministic sweep (truncations, deleted and replaced characters). Not coverage-guided fuzzing; that would need `cargo-fuzz`. | `library::tests::damaged_packs_…`, `config::tests::damaged_configs_…` |
| A-9 | Engine outputs stay in range | Partly done. Pad-axis outputs are checked to be finite and within -1..=1 over random input. Ring sector coverage is not checked. | `engine::release_tests` |
| A-10 | No stuck keys, buttons or mouse buttons after releasing everything | Partly done. Engine-level: random input sequences on four profiles, followed by `release_all`, leave nothing held (`engine::release_tests`; checked by breaking `release_all` on purpose). Kernel-level: written (`daemon::kernel_tests::nothing_stays_pressed_in_the_kernel`, ignored, needs `/dev/uinput`). It runs the engine's output through the daemon's `dispatch` into the real virtual keyboard and mouse, reads what the kernel reports, and checks it against an independent count, with an unplug-style release every 50 steps. A write the kernel refuses is no longer dropped: the press state keeps what was not sent, and the daemon's periodic scan sends it again (`output::tests`, which fail when the old error handling is put back). Run in a container with `scripts/kernel-test-docker.sh`, where it passes (twice). It also runs on a host where the account is in the `input` group: `cargo test -- --ignored nothing_stays`.
| A-11 | Virtual devices and grabs are released when the daemon exits or is killed; keys it holds are released before it exits | Done for SIGTERM, SIGINT, SIGKILL, and the tray's Quit path (the tray is not exercised by the test; it needs a session). `tests/daemon_exit.rs` runs the real daemon against a fake controller made with uhid (`/dev/uhid`, a real HID path; the daemon ignores uinput devices). It holds a key and a mouse button, then stops the daemon. SIGTERM and SIGINT release what the daemon holds before it exits (a handler writes to a pipe, and the loop releases and exits). SIGKILL can't be handled: the test checks only that the grab is released and the virtual devices disappear. Run with `scripts/kernel-test-docker.sh daemon_`. |
| A-12 | Known vulnerabilities in dependencies | Done in CI. Currently four unmaintained or unsound warnings from transitive crates (for example `ttf-parser`, `lru`) and no vulnerabilities, so the job passes. | `cargo audit` in CI |
| A-13 | Each bundled font has a license file | Done | `check-project.py` |

`check-project.py` was checked against a scratch copy with planted errors (a wrong format number, a missing link file, a missing heading and an unknown command). It reported each one.

## 3. Manual QA

Each item has the expected result in the last column. Record the OS, compositor, controller model and firmware, and app version for each run. These are unchanged from the first version of this plan.

### 3.1 Hardware and hot-plug

| # | Steps | Expected |
|---|---|---|
| H-1 | Test each controller type you can get: Xbox (wired and wireless), DualSense, DualShock 4, Switch Pro, a Switch-style or 8BitDo pad, a generic pad. Run passthrough and a remapped profile on each. | Every button, stick and trigger produces the right output. Triggers and sticks are centred at rest. |
| H-2 | Unplug while a button is held: the held key and button are released, and the daemon keeps running | Done in the container: `tests/hotplug.rs` makes the controller with uhid, holds a key and a button, drops the controller, and reads the release from the virtual devices (which stay up). Menus closing isn't checked. |
| H-3 | Replug the same controller: it is managed again and its mappings work | Done in the container for the same controller (`hotplug_replugging_the_same_controller_works_again`). Not covered: a different controller in the same slot, and the gyro fallback. The fallback needs a real motion sensor (INPUT_PROP_ACCELEROMETER), which a simulated controller can't provide. |
| H-4 | Plug in a PlayStation or Switch pad, first without and then with the udev motion rule. | Without the rule, gyro is unavailable and the app says so. With it, gyro works. The profile doesn't flicker when the motion sensor appears a moment after the pad (two 2 s scans). |
| H-5 | Touchpad on DualSense/DS4 in touchpad-as-mouse mode. | Click and movement behave as configured. |
| H-6 | Rumble test button on a pad that supports it and one that doesn't. | Rumble works, or the app says it isn't supported. |
| H-7 | Run for several hours with the pad idle, then use it. | No drift, no lost input, no CPU use while idle (check `top`). |

### 3.2 Desktop focus and auto-switching

| # | Steps | Expected |
|---|---|---|
| D-1 | On KDE (KWin), add a game with a rule. Launch and switch between the game and the desktop. | The right profile is active in each. Alt+Tab from a game works. |
| D-2 | Repeat on Sway, and on Hyprland. | Same as D-1. The README says the Hyprland and Sway support is untested in the sandbox; mark each result as tested or not. |
| D-3 | Use the Flatpak build on KWin, with the window matched by class and title only. | Auto-switching works. Note any window that is matched wrongly. |
| D-4 | Games launched through Steam, Heroic, and Lutris, plus a plain binary. | The library finds them, and the rule matches the window. |
| D-5 | Two games with rules for the same window. | The import preview shows the conflict. The imported game takes over only when the user chooses it. |

### 3.3 Overlays, layers and menus

| # | Steps | Expected |
|---|---|---|
| O-1 | Open the on-screen keyboard over a fullscreen game (windowed and exclusive fullscreen). Type into a text field. | Keys reach the game. The overlay never takes keyboard focus. |
| O-2 | Open the numpad and the keyboard together. | Opening one closes the other. |
| O-3 | Hold and toggle layers, stack two layers, and change profiles while a layer is held. | Layer behaviour matches `docs/development/layers.md`. Held layers end on profile change. |
| O-4 | Open a menu from a stick direction and a combo. Pick an item several times. | Items fire once each. The menu stays up while the input is held. |
| O-5 | Info overlays and the live controller drawing, with layers on. | Labels show the layer's mappings. They don't overlap or overflow. |

### 3.4 Games and packs

These are checks of the shipped packs, not the code.

| # | Steps | Expected |
|---|---|---|
| G-1 | For each of the five shipped packs (`packs/`), play in the real game with the pack's Gamepad profile. | Every mapped action works and the game's own menus can be used. |
| G-2 | Each shipped pack that uses a flick stick or gyro (find them with `grep -l 'flick\|gyro' packs/*.padpack`). Use the *Test turn* button, then play. | Turn size matches the game's mouse sensitivity. Camera turns and stops cleanly. |
| G-3 | Turbo and macros in a game that rejects fast input. | Rate and timing are acceptable. Nothing is sent faster than the game can read. |
| G-4 | Export a game, import it on a clean profile, and play it. | The imported game behaves the same. |
| G-5 | Import a pack from an untrusted source (see H-SEC-2). | The preview shows everything the pack can do. Nothing runs before Save & apply. |
| G-6 | Games with anti-cheat (for example, one that blocks virtual input devices). | Record what happens. Don't try to evade anti-cheat; note the result in the docs if it blocks the app. |

### 3.5 Feel and timing

These need a person with the controller in hand. Record the values used, so a change can be compared.

| # | Check |
|---|---|
| T-1 | Deadzone, response curve, and acceleration feel right for a mouse-look game and for a menu. |
| T-2 | Combo window (default 60 ms): LB+RB chords register reliably without firing a single button by accident. |
| T-3 | Double-tap, triple-tap and long-press thresholds work for a relaxed press and a quick one. |
| T-4 | Toggle and Turbo are predictable across a long session. |
| T-5 | Input latency feels acceptable. Measure with a high-speed camera or an input-lag tool if a regression is suspected. |

### 3.6 GUI

| # | Check |
|---|---|
| U-1 | Light and dark themes. `todo.md` says dark mode was not checked. Check every page and every dialog. |
| U-2 | Every tab, sub-tab and empty state (no games, no profiles, no macros, no layers). |
| U-3 | Resize the window to its minimum size. Nothing is clipped. |
| U-4 | Keyboard-only navigation of the main flows (add game, edit a button, save). |
| U-5 | Save & apply with a conflict (the daemon is running and the file was edited by hand). The user is told which file wins. |
| U-6 | Reload from the command line (`padwight reload`) while the GUI is open. |
| U-7 | The app icon shows in the tray, the app menu and the window. |
| U-8 | Text in the labels: every item in `todo.md` that was checked off, rechecked on a new build. |
| U-9 | README and wiki read the same where they overlap (A-4). |

### 3.7 Install, upgrade and lifecycle

| # | Steps | Expected |
|---|---|---|
| I-1 | Fresh install on a clean machine, following README "Install", both with and without `uaccess`. | The daemon runs with no root. The udev rule fallback works. |
| I-2 | Upgrade from the previous release with a real `config.toml`. | Config converts, the `.old` backup is written, and every game and profile is still there. |
| I-3 | Downgrade attempt with a pack from a newer format. | The app refuses it and asks for an update. |
| I-4 | Flatpak build from scratch, install, run, and use the daemon from the sandbox. | Matches the README's list of sandbox differences. |
| I-5 | `systemctl --user restart padwight` mid-macro and mid-layer. Also kill the daemon with `SIGKILL`. | The daemon comes back, releases everything, and doesn't grab the pad twice. After `SIGKILL` the pad and virtual devices are free again (A-11). |
| I-6 | Suspend and resume with a pad connected. | The pad comes back and works. |
| I-7 | Log in and out. Check the service starts at login. | Works. |
| I-8 | Read the daemon logs (`src/daemon/logs.rs`) after a full session. | No errors or panics. Messages are clear to a user. |
| I-9 | Without `XDG_RUNTIME_DIR` the socket is private to its user: the directory is 0700 and owned by the user, the socket is 0600, another user is refused, and the daemon refuses a directory it doesn't own | Done, in the container. `tests/socket_fallback.rs` runs the daemon with no runtime dir and checks the modes and ownership; a second user (via `setpriv`) is refused with "Permission denied", after a positive control. It also checks the daemon refuses a fallback directory owned by someone else. Removing the 0600 step makes the first test fail. Run with `scripts/kernel-test-docker.sh socket_fallback`. |
| I-10 | Install files are valid: systemd units, udev rule, desktop entry, metainfo, Flatpak manifest | Done in CI. `scripts/check-install.sh` runs `systemd-analyze verify`, `udevadm verify`, `desktop-file-validate`, `appstreamcli validate` and checks the manifest's required keys. A missing validator is an error. Checked that a broken `command` in the manifest fails. |

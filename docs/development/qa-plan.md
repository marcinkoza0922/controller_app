# QA plan

What to check beyond `cargo test`, and what a human reviewer should look at. Automated checks are listed with their status, so it's clear what runs in CI and what still needs a person.

## 1. Issues found and their status

| # | Issue | Status |
|---|---|---|
| F1 | `README.md` said the pack format is **8**; the code and `docs/pack-format.md` say **9**. | Fixed. Enforced by check A-2. |
| F2 | The IPC socket fell back to a file in the shared `/tmp` with no permissions set, so the daemon's control socket could be reached by other users. | Fixed. The socket is `0600`, in a directory the daemon checks is owned by this user and sets to `0700`. The `/tmp` fallback now uses a per-user directory. Covered by `daemon::tests::the_control_socket_is_private_to_this_user` and, without `XDG_RUNTIME_DIR`, by `tests/socket_fallback.rs` (I-9). |
| F3 | No CI: nothing ran check, clippy, the tests or the audit automatically. | Fixed: `.github/workflows/ci.yml` runs on every pull request. Its first run found a problem in the install check, since fixed. |
| F4 | Packs might be able to trigger host programs. | Corrected. A pack can't name an arbitrary program. The only host-program paths reachable from a pack are the built-in Screenshot and ToggleRecording actions, which run fixed tools (`grim` or a recorder). No code change; whether an import preview should list these is still a reviewer's call (H-SEC-2). |
| F5 | `unwrap()` calls on daemon paths could panic and leave the pad grabbed. | Audited. The daemon had one production `unwrap()`, in `release_devices`; it is now a `let ... else`. The remaining production `unwrap`/`expect` calls are invariants or bundled assets: `icon.rs` (the bundled icon) and `monitor.rs` (a mutex). |
| F6 | `scripts/long-files.sh` lists files over 400 lines. | Unchanged. It's a maintainability note, not a defect. |
| F7 | `flatpak/cargo-sources.json` could drift from `Cargo.lock`. | Checked. Enforced by check A-7; currently in sync. |

## 2. Automated checks

Scripted checks live in `scripts/check-project.py` and `scripts/check-install.sh`, and the Rust tests use the commands in `CLAUDE.md`. CI runs the default tests, clippy, the audit and both scripts. The container tests need `/dev/uinput` and `/dev/uhid`, and run through `scripts/kernel-test-docker.sh`, not in CI.

| # | Check | Status | Where |
|---|---|---|---|
| A-0 | CI on push and pull request | Done | `.github/workflows/ci.yml`: build-and-test, project-checks, install-files, audit |
| A-1 | CLI commands in the docs exist | Done | `check-project.py`, against the dispatch in `src/main.rs` |
| A-2 | Pack format number agrees across code and docs | Done | `check-project.py` |
| A-3 | Links and `#anchors` resolve; GitHub `blob/main` links point at real files and headings | Done | `check-project.py` |
| A-4 | README and wiki don't contradict each other | Not automated: a review item. The wiki repeats README sections, so it's worth reading both when either changes. | Manual (section 3.6) |
| A-5 | Every shipped pack survives a save and reload unchanged | Done | `library::tests::every_library_pack_survives_a_save_and_load` |
| A-6 | Config folder: every setup, profile and shared item is saved as its own file, reloads unchanged, and a removed setup leaves no folder behind | Done. `Config::load_from` and `save_to` take a path, so the tests use a temporary directory and never touch the real config. Covers a full round trip (the app settings in `config.toml`, the setups as folders), the removal of a setup, a missing General folder falling back to the default, and file names made from unsafe item names. | `config::store::tests::*` |
| A-7 | `cargo-sources.json` matches `Cargo.lock` | Done | `check-project.py` |
| A-8 | Damaged pack and config text is refused or handled without panicking | Done, as a deterministic sweep (truncations, deleted and replaced characters). Not coverage-guided fuzzing; that would need `cargo-fuzz`. | `library::tests::damaged_packs_…`, `config::tests::damaged_configs_…` |
| A-9 | Engine outputs stay in range | Done. Pad-axis outputs are checked to be finite and within -1..=1 over random input. Ring sectors are checked to cover the full circle with no gaps, for 4, 8 and 12 sectors and several start angles. | `engine::release_tests`, `engine::stick::tests::rings_cover_the_whole_circle_without_gaps` |
| A-10 | No stuck keys, buttons or mouse buttons after releasing everything | Partly done. Engine-level: random input sequences on four profiles, followed by `release_all`, leave nothing held (`engine::release_tests`; checked by breaking `release_all` on purpose). Kernel-level: written (`daemon::kernel_tests::nothing_stays_pressed_in_the_kernel`, ignored, needs `/dev/uinput`). It runs the engine's output through the daemon's `dispatch` into the real virtual keyboard and mouse, reads what the kernel reports, and checks it against an independent count, with an unplug-style release every 50 steps. A write the kernel refuses is no longer dropped: the press state keeps what was not sent, and the daemon's periodic scan sends it again (`output::tests`, which fail when the old error handling is put back). Run in a container with `scripts/kernel-test-docker.sh`, where it passes (twice). It also runs on a host where the account is in the `input` group: `cargo test -- --ignored nothing_stays`. | | `engine::release_tests`, `daemon::kernel_tests` (container), `output::tests` |
| A-11 | Virtual devices and grabs are released when the daemon exits or is killed; keys it holds are released before it exits | Done for SIGTERM, SIGINT, SIGKILL, and the tray's Quit path (the tray is not exercised by the test; it needs a session). `tests/daemon_exit.rs` runs the real daemon against a fake controller made with uhid (`/dev/uhid`, a real HID path; the daemon ignores uinput devices). It holds a key and a mouse button, then stops the daemon. SIGTERM and SIGINT release what the daemon holds before it exits (a handler writes to a pipe, and the loop releases and exits). SIGKILL can't be handled: the test checks only that the grab is released and the virtual devices disappear. Run with `scripts/kernel-test-docker.sh daemon_`. | | `tests/daemon_exit.rs` (container) |
| A-12 | Known vulnerabilities in dependencies | Done in CI. Currently four unmaintained or unsound warnings from transitive crates (for example `ttf-parser`, `lru`) and no vulnerabilities, so the job passes. | `cargo audit` in CI |
| A-13 | Each bundled font has a license file | Done | `check-project.py` |

`check-project.py` was checked against a scratch copy with planted errors (a wrong format number, a missing link file, a missing heading and an unknown command). It reported each one.

## 3. Manual QA

Each item has the expected result in the last column. Record the OS, compositor, controller model and firmware, and app version for each run. These are unchanged from the first version of this plan.

### 3.1 Hardware and hot-plug

| # | Steps | Expected |
|---|---|---|
| H-1 | Test each controller type you can get: Xbox (wired and wireless), DualSense, DualShock 4, Switch Pro, a Switch-style or 8BitDo pad, a generic pad. Run passthrough and a remapped profile on each. | Every button, stick and trigger produces the right output. Triggers and sticks are centered at rest. |
| H-2 | Unplug while a button is held: the held key and button are released, and the daemon keeps running | Done in the container: `tests/hotplug.rs` makes the controller with uhid, holds a key and a button, drops the controller, and reads the release from the virtual devices (which stay up). Menus closing isn't checked. |
| H-3 | Replug the same controller: it is managed again and its mappings work | Done in the container for the same controller (`hotplug_replugging_the_same_controller_works_again`). Done for a different controller too (`hotplug_a_different_controller_takes_over_and_is_remapped`): a second, differently named controller is grabbed, remapped, and the first one's grab is released. The slot number itself isn't reported outside the daemon, so the test checks the behaviour, not the number. Not covered: the gyro fallback. The fallback needs a real motion sensor (INPUT_PROP_ACCELEROMETER), which a simulated controller can't provide. |
| H-4 | Plug in a PlayStation or Switch pad, first without and then with the udev motion rule. | Without the rule, gyro is unavailable and the app says so. With it, gyro works. The profile doesn't flicker when the motion sensor appears a moment after the pad (two 2 s scans). |
| H-5 | Touchpad on DualSense/DS4 in touchpad-as-mouse mode. | Click and movement behave as configured. |
| H-6 | Rumble test button on a pad that supports it and one that doesn't. | Rumble works, or the app says it isn't supported. |
| H-7 | Run for several hours with the pad idle, then use it. | No drift, no lost input, no CPU use while idle (check `top`). Partly done in the container: `tests/daemon_health.rs` idles a fake controller for 5 s and fails if the daemon uses 1% of a core or more (measured 0.2%, one clock tick). The several-hour run, drift and lost input still need a person. |

### 3.2 Desktop focus and auto-switching

| # | Steps | Expected |
|---|---|---|
| D-0 | Process-scan fallback (no tracker): a game's rule switches the profile while its process runs, and back when it exits. | Done in the container (`tests/process_focus.rs`). The daemon used to forget the game when the first focus report arrived after its startup scan, so a game that exited soon after start left the game's profile active. Now only a change of backend resets the scan. |
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
| U-0 | The settings window opens and draws under a display. | Done in the container under Xvfb (`tests/gui_smoke.rs`, `scripts/kernel-test-docker.sh gui_`). It renders with the CPU fallback where there is no GPU. Clicking through the tabs in the real window is still manual. |
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
| I-2 | Upgrade from the previous single-file `config.toml`. | Not supported, by design: the format changed and old configs aren't converted. The old file is overwritten on the next save, so keep a copy before upgrading. |
| I-3 | Downgrade attempt with a pack from a newer format. | The app refuses it and asks for an update. |
| I-4 | Flatpak build from scratch, install, run, and use the daemon from the sandbox. | Matches the README's list of sandbox differences. |
| I-5 | `systemctl --user restart padwight` mid-macro and mid-layer. Also kill the daemon with `SIGKILL`. | The daemon comes back, releases everything, and doesn't grab the pad twice. After `SIGKILL` the pad and virtual devices are free again (A-11). |
| I-6 | Suspend and resume with a pad connected. | The pad comes back and works. |
| I-7 | Log in and out. Check the service starts at login. | Works. |
| I-8 | Read the daemon logs (`src/daemon/logs.rs`) after a full session. | No errors or panics. Messages are clear to a user. Partly done: `common::assert_log_clean` fails on a panic or an output error, and runs at the end of the daemon-exit, hotplug and idle tests (container). It doesn't check other error lines, and the user-facing wording still needs a read. |
| I-9 | On a machine without `XDG_RUNTIME_DIR`, start the daemon and check the socket: `stat` on its directory shows `0700`, and on the socket `0600`. Run `padwight status` from another user's shell and check it is refused. | Only this user can reach the daemon (F2). |
| I-10 | Install files are valid: systemd units, udev rule, desktop entry, metainfo, Flatpak manifest | Done in CI. `scripts/check-install.sh` runs `systemd-analyze verify`, `udevadm verify`, `desktop-file-validate`, `appstreamcli validate` and checks the manifest's required keys. A missing validator is an error. Checked that a broken `command` in the manifest fails. |

## 4. Security review (for a human)

Automated checks can't judge these. Each needs a reviewer to decide what's acceptable.

| # | Question |
|---|---|
| H-SEC-1 | **IPC socket (F2).** Fixed for permissions. Should any request be refused by a non-owner even with the socket reachable? Which requests (`SetConfig`, `TestRumble`, …) are most sensitive? |
| H-SEC-2 | **Pack import (F4).** The only host-program actions a pack can contain are Screenshot and ToggleRecording. Should the import preview list them, so a player sees a pack can take screenshots or record the screen? |
| H-SEC-3 | **Virtual input.** The app creates a keyboard and mouse that any local program can use. Is that clear to users? |
| H-SEC-4 | **Flatpak sandbox.** `--device=all` lets the sandbox read and send input on the whole machine. Is the README's warning enough? |
| H-SEC-5 | **Config folder.** The daemon is the only writer while it runs. Saves build a staging folder and swap it in, so a failed save leaves the old setups. Are the files written atomically, and are their permissions `0600`? Every file is written `0600` through one helper (tested). No backup copies are made any more, so there is no second copy to protect. |
| H-SEC-6 | **Dependencies.** Read the `cargo audit` warnings (four today, all in transitive crates) and the licenses of direct dependencies. |

## 5. Judgement calls for a reviewer

- Are the shipped packs' mappings sensible for the games they cover? The tests only check the format, not whether the mapping is good.
- Are the defaults right: Gamepad passthrough, the Desktop mouse mapping, the 60 ms combo window, and the turbo range?
- Does the copy make sense to a new user? Read the GUI text as someone who hasn't seen the app.
- Is the scope of the wiki right? It repeats the README in several places, and the two should be kept in step (A-4).
- Which unsupported setups should be documented as unsupported (Sway and Hyprland, the Flatpak focus path, GNOME)?

## 6. Remaining work, in order

Automatable, not yet done:

1. ~~**Idle CPU and log checks (H-7, I-8).**~~ Done, see H-7 and I-8 above.
2. ~~**Config file permissions (H-SEC-5, partly).**~~ Done: the saved config is `0600` (`config::tests::a_saved_config_is_readable_only_by_its_owner`). The save used to leave it `0644`; it now creates the temporary file private and sets its mode again before the rename.
3. ~~**Process-based focus switching (D-partial).**~~ Done: `tests/process_focus.rs` starts the daemon with a game rule, runs a stand-in process named like the game, and checks the profile switches to the game's and back when it exits. The test found a race (fixed, see below). KWin, Sway and Hyprland stay manual.
4. ~~**GUI smoke test (U-partial).**~~ Done in the container: `tests/gui_smoke.rs` starts the real settings window under Xvfb against a daemon, checks that a window titled "Padwight" is on the display, keeps it open for 5 s, and fails on a panic. Tabs are not clicked through: nothing drives the window. Every page and tab is built in-process by `gui::tests::every_page_tab_and_dialog_builds`.
5. ~~**Ring sector coverage (A-9).**~~ Done: `engine::stick::tests::rings_cover_the_whole_circle_without_gaps`. A sweep of directions finds no gaps for 4, 8 and 12 sectors and several start angles. Breaking the sector offset makes it fail.
6. ~~**Controller in the same slot (H-3).**~~ Done: `hotplug_a_different_controller_takes_over_and_is_remapped` (container).

Needs a person or a real session:

7. Tray Quit on a desktop session, and whether libinput releases keys when a virtual keyboard disappears (only relevant to SIGKILL, a crash or a power cut).
8. The manual sections (3.1 to 3.6 and the rest of 3.7) on the controllers and compositors you have, in this order: H, D, O, G, I, U, T.
9. The security review (section 4) before the next release, with a second reviewer for H-SEC-1 and H-SEC-2.

## Commands

From `CLAUDE.md`:

```sh
cargo check --all-targets --message-format=short 2>&1 | head -40
cargo clippy --all-targets -- -D warnings
RUST_BACKTRACE=0 cargo nextest run --profile agent --hide-progress-bar --cargo-quiet
cargo test -- --ignored rumble      # needs /dev/uinput
```

Project checks, install files, and dependency audit:

```sh
python3 scripts/check-project.py
scripts/check-install.sh
cargo audit
```

Container tests (need `/dev/uinput` and `/dev/uhid`; run as root in the container):

```sh
scripts/kernel-test-docker.sh nothing_stays   # A-10, kernel level
scripts/kernel-test-docker.sh daemon_         # A-11, daemon exit
scripts/kernel-test-docker.sh hotplug_        # H-2, H-3
scripts/kernel-test-docker.sh idle_           # H-7 (idle CPU)
scripts/kernel-test-docker.sh process_        # D-0, process-scan switching
scripts/kernel-test-docker.sh gui_            # U-0, the settings window under Xvfb
scripts/kernel-test-docker.sh socket_fallback # I-9
```

Performance (not a pass/fail check). `scripts/profile.sh` runs `tests/profile` in a release build and writes one JSON file per measurement to `target/profile/`: press-to-output latency for the daemon's virtual pad, idle and under a 1 kHz stick flood; daemon CPU and memory, idle and under flood; daemon start-up, cold (no config, binary evicted from the page cache) and warm; and settings-window start-up with the daemon running. The latency figure excludes USB polling and the controller's own scan, since the test uses a uhid pad. When the run ends, `scripts/plot_profile.py` draws the charts (`target/profile/charts/*.png`) and `target/profile/report.html`, which pairs each chart with its numbers.

```sh
scripts/profile.sh                        # everything
scripts/profile.sh profile_input_latency  # one measurement
```

Desktop sessions (need test VMs, see below; not run in CI). Rerun them when the daemon's
interaction with the system changes (focus tracking, input, the GNOME extension), and when a
desktop, the kernel or the input stack gets a new release:

```sh
scripts/desktop-versions.sh   # network: latest GNOME, Plasma, Sway and Hyprland releases; exits 1 if GNOME's major isn't in the extension's shell-version list
DESKTOP_TEST_HOST=dev@127.0.0.1 DESKTOP_TEST_SSH_OPTS="-p <port> -i vms/id_ed25519" scripts/desktop-test.sh gnome   # or kde, sway, hyprland, labwc (a wlroots compositor)
```

`desktop-test.sh` syncs the checkout to the VM, builds it, and runs the checks in that session:
the tracker is found, a terminal's window is seen, its rule switches the profile, and the profile
falls back when the window closes. Set `DESKTOP_TEST_QMP` to the VM's QMP socket to close the GNOME
Overview first. Run it only against a throwaway VM: the daemon grabs gamepads and creates virtual
input devices. Record the versions it prints with the result.

Last run, 2026-10-09, kernel 7.2.9. Each row is the native build, with the Flatpak in the last column where it was run:

| Desktop | Version | Checks | Flatpak |
|---|---|---|---|
| GNOME | 51.0 | all pass (overlay skipped) | all pass (overlay skipped) |
| KDE Plasma | 6.7.5 | all pass | all pass |
| Sway | 1.12 | all pass | focus falls back to processes; overlay and rules skipped |
| Hyprland | 0.56.2 | all pass | all pass except the overlay, skipped (layer-shell hidden) |
| labwc | 0.20.2 | all pass | focus falls back to processes; overlay and rules skipped |

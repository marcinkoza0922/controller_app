# controller_app

Background service that remaps gamepad inputs, or turns them into mouse and keyboard input, with an iced settings GUI.

- **daemon**: grabs physical gamepads (evdev) so nothing else sees them, runs the active profile, and emits through virtual uinput devices (an Xbox 360-style pad plus a keyboard and a mouse).
- **gui**: edits profiles and talks to the daemon over `$XDG_RUNTIME_DIR/controller_app.sock`.
- **config**: `~/.config/controller_app/config.toml`. The daemon is the only writer while it runs; after editing by hand, run `controller_app reload`.

## Install

```sh
cargo install --path .
mkdir -p ~/.config/systemd/user
cp dist/controller_app.service ~/.config/systemd/user/
systemctl --user enable --now controller_app
controller_app            # open the GUI
```

On most desktops (systemd-logind with `uaccess`), your user can already open `/dev/uinput` and the gamepad nodes, so no root is needed. If it can't, add a udev rule that grants access, or add yourself to the `input` group.

## Profiles

Each profile maps:

- **buttons**: to a gamepad button, a key or key combo (`LEFTCTRL+C`), a mouse button, the scroll wheel (one notch per press, continuous while held), *next profile*, or **several of these at once**.
- **sticks**: to a gamepad stick, mouse pointer, scroll wheel, or direction keys (WASD/arrows), with a deadzone, a response curve, and an adjustable key threshold.
- **triggers**: to an analog gamepad trigger, or to a button action past a threshold.

On top of that:

- **Combos**: buttons pressed together (e.g. LB+RB) act as their own input. Combo members wait a short window (default 60 ms) for the rest of the combo. A member whose own action is *Disabled* works as a modifier with no time limit.
- **Gestures**: double tap, triple tap and long press per button. The final tap of a sequence fires on press and holds, so "double-tap and hold" works. A single press on a button with gestures fires once the gesture is decided.
- **Zones**: extra actions held while a stick or trigger is within part of its travel. For example, Left Shift on a partial stick push gives walk/run with WASD, and a half versus full trigger pull can do different things. Zones are hidden for controllers whose triggers are on/off only (e.g. Switch pads).

The defaults are **Gamepad** (1:1 passthrough) and **Desktop** (left stick moves the mouse, right stick scrolls, A/B click, LB+RB = Alt+Tab). Guide cycles between profiles in both.

More templates are available under "New from template…" in the GUI:

- **PC action**: WASD on the left stick (Shift at full push to sprint), mouse look on the right stick, Mouse 1/2 on RT/LT, A = E (use), B = Space, X = R, Y = F, LB/RB = Q/G, L3/R3 = Ctrl/V, D-pad = 1–4, Start/Select = Esc/Tab.
- **Strategy**: mouse pointer on the left stick, arrow-key camera on the right, A/B/X = left/right/middle click, RT = left click for drag-select, L3/R3 = zoom (wheel up/down), LB/RB = held Ctrl/Shift, D-pad = control groups 1–4.
- **Retro / platformer**: arrows on the D-pad and left stick, A/B/X/Y = Z/X/C/V, for keyboard-only indie games and emulators.

## Per-game profiles

Rules switch the profile when a game's window gets focus, and an optional default profile is used for everything else. A rule matches on:

- **Executable**: the program's file name. For Wine/Proton games it's the Windows `.exe` (e.g. `eldenring.exe`), not the Wine loader.
- **Steam App ID**: taken from the environment Steam sets, or from Proton's `steam_app_<id>` window class.
- **Window class**.

The GUI lists recently focused windows with a one-click "+ Rule". Switching with Guide or the GUI stays in effect until focus changes again. Focusing the settings window never switches profiles.

Focus tracking uses a small KWin script (KDE Plasma, Wayland or X11). On other desktops, rules apply while a matching process is running.

## GUI

- A live controller drawing that shows sticks, buttons and triggers as you use them.
- Profile editor with an on-screen keyboard for picking keys, so you don't need to know evdev key names.
- Per-controller "Manage" toggles and a "Test rumble" button (a strong pulse, then a weak one, for pads that support rumble), plus the enabled switch and the active profile.

## CLI

`controller_app status | enable | disable | profile <name> | next-profile | reload | daemon`

When `controller_app daemon` runs in a terminal, it keeps a live status line showing each controller's input and what is being output.

## Notes

- Virtual uinput devices, such as Steam Input's pad and our own, are skipped. For a pad this app manages, turn off Steam Input or games may see two controllers.
- Pads on the `xpad` driver report X/Y by label rather than position; this is corrected automatically.
- Rumble from games is forwarded to the physical controller when it supports force feedback. The end-to-end check needs `/dev/uinput`, so it is opt-in: `cargo test -- --ignored rumble`.

## License

GPL-3.0. See [LICENSE](LICENSE).

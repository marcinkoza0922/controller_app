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

For **gyro** on PlayStation and Switch controllers, also install the motion-sensor rule. Their motion sensors are a separate device that `uaccess` doesn't cover:

```sh
sudo cp dist/70-controller-app-motion.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

## Profiles

Each profile maps:

- **buttons**: to a gamepad button, a key or key combo (`LEFTCTRL+C`), a mouse button, the scroll wheel (one notch per press, continuous while held), *next profile*, or **several of these at once**.
- **sticks**: to a gamepad stick, mouse pointer, scroll wheel or direction keys (WASD/arrows), with a deadzone, a response curve and an adjustable press threshold. Each stick's **directions also act as buttons** on top of that (Left Stick Up, …): they can have any action or gesture and can be part of combos, e.g. LB + Right Stick Right. Gamepad outputs include stick directions too, so a button (or the D-pad) can push the virtual stick, and two directions make a diagonal.
- **triggers**: to an analog gamepad trigger, or to anything a button can do (a gamepad button, key, mouse button, macro, …), pressed once pulled past a threshold.

On top of that:

- **Toggle and Turbo**: wrap any action to make it a toggle (press once to hold it down, press again to release, e.g. hold-to-crouch games) or turbo (repeats 2–30 times a second while held). They combine: Toggle → Turbo is auto-fire you switch on and off.
- **Combos**: buttons pressed together (e.g. LB+RB) act as their own input. Combo members wait a short window (default 60 ms) for the rest of the combo. A member whose own action is *Disabled* works as a modifier with no time limit.
- **Gestures**: double tap, triple tap and long press per button. The final tap of a sequence fires on press and holds, so "double-tap and hold" works. A single press on a button with gestures fires once the gesture is decided.
- **Zones**: extra actions held while a stick or trigger is within part of its travel. For example, Left Shift on a partial stick push gives walk/run with WASD, and a half versus full trigger pull can do different things. Zones are hidden for controllers whose triggers are on/off only (e.g. Switch pads).

The defaults are **Gamepad** (1:1 passthrough) and **Desktop** (left stick moves the mouse, right stick scrolls, A/B click, LB+RB = Alt+Tab). Guide cycles between profiles in both.

More templates are available under "New from template…" in the GUI:

- **PC action**: WASD on the left stick (Shift at full push to sprint), mouse look on the right stick, Mouse 1/2 on RT/LT, A = E (use), B = Space, X = R, Y = F, LB/RB = Q/G, L3/R3 = Ctrl/V, D-pad = 1–4, Start/Select = Esc/Tab.
- **Strategy**: mouse pointer on the left stick, arrow-key camera on the right, A/B/X = left/right/middle click, RT = left click for drag-select, L3/R3 = zoom (wheel up/down), LB/RB = held Ctrl/Shift, D-pad = control groups 1–4.
- **Retro / platformer**: arrows on the D-pad and left stick, A/B/X/Y = Z/X/C/V, for keyboard-only indie games and emulators.

## Macros

The Macros tab holds named input sequences shared by all profiles, each in its own collapsible card ("+ New macro" adds one at the top). Each step is a **tap** (press, hold for N ms, release), **hold down**, **release**, **wait** (N ms), or **move stick** (a virtual-pad stick to a direction or custom position, held until the next stick step and recentered when the macro ends). Steps can press a key or key combo, mouse button, scroll wheel or gamepad button. "Insert motion…" adds fighting-game motions one frame apart: quarter circles, dragon punch and half circles, written facing right. Map a macro with the "Macro…" action:

- **Play once**: each press plays it through to the end, even if you let go early.
- **Repeat while held**: it loops until you let go, then stops at once.

Each pass of a macro ends by releasing anything it still holds. Wrap a repeating macro in a Toggle to loop it hands-off. Renaming a macro updates its mappings, and profile switches stop running macros.

## On-screen overlay

The "On-screen keyboard" action (also a button on the Overview tab, and `controller_app overlay-toggle`) opens a keyboard over everything, including fullscreen games. It is a Wayland layer-shell surface that never takes keyboard focus, so keys go to the window underneath. While it's open the controller drives it:

| Control | Action |
|---|---|
| D-pad / left stick | move between keys (repeats while held) |
| A | press the selected key (holding A holds the key) |
| Shift / Ctrl / Alt / Super | latch for the next key |
| X / Y / Start | Backspace / Space / Enter |
| hold B | close |

New Desktop profiles open it with a long press of Guide. It needs a compositor with layer-shell (KDE Plasma, Sway, Hyprland, …).

## Menus

The Overlays tab also holds on-screen **action menus**, shared by all profiles. Open one from any button, gesture, combo, trigger or stick direction with the "Open menu…" action (or `controller_app menu <name>`). A menu is on screen only while that input is held, and letting go closes it. To keep a menu up without holding, wrap the action in "Toggle…": the menu then stays until the input is pressed again. Choosing an item taps its action like a button press, so items can be keys, macros, toggles or the keyboard, and the menu stays up for more picks.

- **Radial**: aim a stick at an item; whatever is aimed at when you let go is chosen.
- **Directional**: four slots on the D-pad or face buttons.
- **List**: move with the D-pad or left stick; A chooses.
- **Button menu**: a list whose items can also be chosen directly with their own button.
- **Carousel**: cycle with the bumpers, triggers, D-pad or a stick; A chooses.

An item can open another menu of the same kind (radial menus can't open menus). The new menu is a child of the first: letting go closes both. While a menu is open the controller drives it, and anything the mappings were holding is released. If a menu is opened by something that can't be held (the command line, or a button whose single press only fires after its tap window), it stays until an item is chosen or B is pressed (Select for face-button menus). The overlay window stays running invisibly between uses, so menus appear instantly.

Each menu is a collapsible card on the Overlays tab, with a live preview; "Add a menu" adds another at the top. Under **Appearance**, each menu (and the keyboard) has its own screen position (a 3×3 grid of corners, edges and center, so it fits any monitor size or aspect ratio), size (50–200%), and color and opacity for the background, items and selected item. Text switches between light and dark to stay readable. Older configs with `cascade` menus still load.

## Gyro

Controllers with motion sensors (DualShock 4, DualSense, Switch Pro, Joy-Cons) can use their gyro in each profile:

- **Mouse**: gyro aiming, in pixels per degree turned.
- **Gamepad stick**: rotation speed deflects a virtual-pad stick, added to the physical stick. An anti-deadzone gets past the game's own stick deadzone.
- **Steering**: tilt the controller like a wheel to move a stick.

Horizontal aim can come from yaw (turning), roll (tilting) or both, and either axis can be inverted. Gyro can be **always on**, **on only while holding** an input (e.g. LT to aim down sights), **off while holding** (a clutch for repositioning the controller), or **toggled**. A **recenter** input sets the current tilt as straight for steering. Slow movement below a jitter threshold is scaled down to hide drift, and "Calibrate gyro" in the controller list measures the drift while the controller sits still. The PC action template turns on gyro mouse aiming while LT is held.

## Per-game profiles

Rules switch the profile when a game's window gets focus, and an optional default profile is used for everything else. A rule matches on:

- **Executable**: the program's file name. For Wine/Proton games it's the Windows `.exe` (e.g. `eldenring.exe`), not the Wine loader.
- **Steam App ID**: taken from the environment Steam sets, or from Proton's `steam_app_<id>` window class.
- **Window class**.

The GUI lists recently focused windows with a one-click "+ Rule". Switching with Guide or the GUI stays in effect until focus changes again. Focusing the settings window never switches profiles.

Focus tracking uses a small KWin script (KDE Plasma, Wayland or X11). On other desktops, rules apply while a matching process is running.

## GUI

- **Overview**: a live controller drawing labelled with the active profile's mappings, the controller list (Manage, Test rumble, Calibrate gyro), and per-game rules.
- **Profile**: the profile being edited, with its own labelled drawing and sub-tabs for Buttons, Sticks & triggers, Combos and Gyro. Mappings collapse to one-line summaries ("A ▸ Left click"); click a name to edit it. **Find by pressing** jumps to whatever you press or push on the controller.
- **Macros**: step editor with exact millisecond fields.

Problems that would block saving (unknown keys, missing macros, incomplete combos) are flagged on the row and with ⚠ on its tab. Section explanations sit behind ⓘ tooltips, and keys can be picked from an on-screen keyboard.

## CLI

`controller_app status | enable | disable | profile <name> | next-profile | reload | overlay-toggle | menu <name> | daemon`

When `controller_app daemon` runs in a terminal, it keeps a live status line showing each controller's input and what is being output.

## Notes

- PlayStation and Switch controllers are also readable through hidraw, which SDL (and Steam) use directly. Games may then see the real controller as well as ours. Turn off Steam Input for the pad, or set `SDL_JOYSTICK_HIDAPI=0` for the game.
- Virtual uinput devices, such as Steam Input's pad and our own, are skipped. For a pad this app manages, turn off Steam Input or games may see two controllers.
- Pads on the `xpad` driver report X/Y by label rather than position; this is corrected automatically.
- Rumble from games is forwarded to the physical controller when it supports force feedback. The end-to-end check needs `/dev/uinput`, so it is opt-in: `cargo test -- --ignored rumble`.

## License

GPL-3.0. See [LICENSE](LICENSE).

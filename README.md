# controller_app

Background service that remaps gamepad inputs, or turns them into mouse and keyboard input, with an iced settings GUI.

- **daemon**: grabs physical gamepads (evdev) so nothing else sees them, runs the active profile, and emits through virtual uinput devices (an Xbox 360-style pad plus a keyboard and a mouse).
- **gui**: edits profiles and talks to the daemon over `$XDG_RUNTIME_DIR/controller_app.sock`.
- **config**: `~/.config/controller_app/config.toml`. The daemon is the only writer while it runs; after editing by hand, run `controller_app reload`. A config from before games existed is converted on first load, and the original is kept as `config.toml.old`. Each profile that an auto-switch rule pointed to becomes a game of its own with those rules; the rest go to General. Macros, menus and info overlays limited to one game's profiles move into that game, and the others become shared.

## Games

Everything is organized by game. A game holds its own profiles, macros, menus, info overlays and the auto-switch rules that activate it; all of its profiles can use all of its items. **General** is a built-in game with no rules, for the desktop and a plain gamepad. Its Macros, Menus and Info overlays tabs also hold the **shared** items, which every profile of every game can use. Names only need to be unique within a game, and a game's items can't reuse a shared item's name.

The GUI's sidebar lists General and the games (with a search), and each game's page has sub-tabs: **Profiles**, **Macros**, **Menus**, **Info overlays** and **Details** (name, rules, export, delete). On the Macros, Menus and Info overlays tabs, "Copy from another game…" previews another game's (or a library game's) items and copies one in to adapt.

## Packs and the library

A game can be shared as a **pack**: **Details → Export…** writes it to a `.padpack` file (TOML), copying in any shared items it uses. The export dialog warns about references to missing items and about anything beyond a plain XInput pad (today, gyro), which players with simpler controllers won't get. Re-exporting your own game keeps its pack ID; exporting someone else's makes a fork with its own ID that credits the original.

**+ Add game** opens the picker: the built-in library (games you have installed in Steam, Heroic or Lutris, or running, come first), an empty game from a template, or **Import a file…**. Importing shows a preview first: what's inside, what your controller lacks, a game with the same name (add under a new name, or replace yours), shared items it renames to avoid clashes, and other games' rules for the same window (the imported game takes over unless you keep yours). Nothing changes until Save & apply. Imported games are ordinary, editable games that remember their pack: importing a newer version of the same pack, or an app update shipping a newer library version (marked "update" in the sidebar), offers to update it and lists what you changed since. A pack made by a newer app version is refused with a request to update.

The library is `packs/*.padpack` in this repository, embedded at build time; see `packs/README.md`.

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
- **sticks**: to a gamepad stick, mouse pointer, scroll wheel or direction keys (WASD/arrows), with a deadzone, a response curve and an adjustable press threshold. Mouse sticks also have **acceleration** (speed grows while held at full push), and under *Advanced response* a ramp time, an outer-edge boost, a vertical speed ratio and smoothing. Each stick's **directions also act as buttons** on top of that (Left Stick Up, …): they can have any action or gesture and can be part of combos, e.g. LB + Right Stick Right. Gamepad outputs include stick directions too, so a button (or the D-pad) can push the virtual stick, and two directions make a diagonal.
- **triggers**: to an analog gamepad trigger, or to anything a button can do (a gamepad button, key, mouse button, macro, …), pressed once pulled past a threshold.

On top of that:

- **Toggle and Turbo**: wrap any action to make it a toggle (press once to hold it down, press again to release, e.g. hold-to-crouch games) or turbo (repeats 2–30 times a second while held). They combine: Toggle → Turbo is auto-fire you switch on and off.
- **Combos**: buttons pressed together (e.g. LB+RB) act as their own input. Combo members wait a short window (default 60 ms) for the rest of the combo. A member whose own action is *Disabled* works as a modifier with no time limit.
- **Gestures**: double tap, triple tap and long press per button. The final tap of a sequence fires on press and holds, so "double-tap and hold" works. A quick tap on a button with gestures fires once the gesture is decided; holding it past the tap window presses the button's own action right away and holds it until release (unless a long press is set).
- **Zones**: extra actions held while a stick or trigger is within part of its travel. For example, Left Shift on a partial stick push gives walk/run with WASD, and a half versus full trigger pull can do different things. Zones are hidden for controllers whose triggers are on/off only (e.g. Switch pads).

General's defaults are **Gamepad** (1:1 passthrough) and **Desktop** (left stick moves the mouse, right stick scrolls, A/B click, LB+RB = Alt+Tab). Guide cycles between the profiles of the active game in both.

More templates are available under "New from template…" in the GUI:

- **PC action**: WASD on the left stick (Shift at full push to sprint), mouse look on the right stick, Mouse 1/2 on RT/LT, A = E (use), B = Space, X = R, Y = F, LB/RB = Q/G, L3/R3 = Ctrl/V, D-pad = 1–4, Start/Select = Esc/Tab.
- **Strategy**: mouse pointer on the left stick, arrow-key camera on the right, A/B/X = left/right/middle click, RT = left click for drag-select, L3/R3 = zoom (wheel up/down), LB/RB = held Ctrl/Shift, D-pad = control groups 1–4.
- **Retro / platformer**: arrows on the D-pad and left stick, A/B/X/Y = Z/X/C/V, for keyboard-only indie games and emulators.

## Layers

A layer changes some of the controller's mappings while it's on, then changes them back, e.g. hold LB and the face buttons type F1–F4 and the right stick scrolls. Layers belong to a game (its **Layers** tab) and apply over whichever of its profiles is active; anything a layer doesn't set stays as in the profile. A layer can override buttons and stick directions, gestures, sticks, triggers (with their zones) and gyro, add combos, and switch the profile's combos off.

- **Turning one on**: map "Layer…" to any button, trigger, stick direction, zone, gesture or combo: the layer is on while that input is held. Wrapped in **Toggle** it stays on until pressed again, which also lets a menu item switch it (menu items can only toggle layers). Layers can turn on further layers.
- **Several at once**: they stack, and the newest one wins where they disagree.
- **Switching over**: an input that's down when a layer comes on or goes off keeps doing what it started until it's released; sticks, triggers and gyro switch modes right away. Held layers end when the profile changes or an on-screen menu or keyboard opens; toggled ones stay on within the game.
- **Showing it**: each layer shows its name on screen while on (with its own position, size and colors), or one of the game's info overlays (e.g. a cheat sheet), or nothing for quick ones like "hold Y to lean". The live controller drawing labels the inputs as they act with the layers that are on.
- **Editing**: the Layers tab uses the profile editor, shown over one of the game's profiles. Inputs the layer doesn't set read "Same as Gameplay: …" with **Override**; overridden ones have **Back to base**. Layers use their profile's timings. "Copy from another game…" copies a layer (with the macros, menus and info overlays it uses), handy for sequels. Shared items can't use layers.

## Macros

A game's Macros tab holds named input sequences, each in its own collapsible card ("+ New macro" adds one at the top). Each step is a **tap** (press, hold for N ms, release), **hold down**, **release**, **wait** (N ms), or **move stick** (a virtual-pad stick to a direction or custom position, held until the next stick step and recentered when the macro ends). Steps can press a key or key combo, mouse button, scroll wheel or gamepad button. "Insert motion…" adds fighting-game motions one frame apart: quarter circles, dragon punch and half circles, written facing right. Map a macro with the "Macro…" action:

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
| hold LT | hold Shift |
| hold B | close |

New Desktop profiles open it with a long press of Guide. It needs a compositor with layer-shell (KDE Plasma, Sway, Hyprland, …).

The "On-screen numpad" action (or `controller_app numpad-toggle`) opens a smaller pad with the digits 0–9 and a dot, for codes and number fields: move with the D-pad or left stick, A presses a number, X is backspace, Start presses Enter, and holding B closes it. It types the top-row number keys, so it works whatever the Num Lock state. Both pads have their own position, size and colors on the Settings page, and opening one closes the other.

## Menus

A game's Menus tab holds on-screen **action menus**. Open one from any button, gesture, combo, trigger or stick direction with the "Open menu…" action (or `controller_app menu <name>`). A menu is on screen only while that input is held, and letting go closes it. To keep a menu up without holding, wrap the action in "Toggle…": the menu then stays until the input is pressed again. Choosing an item taps its action like a button press, so items can be keys, macros, toggles or the keyboard, and the menu stays up for more picks.

- **Radial**: aim a stick at an item; whatever is aimed at when you let go is chosen.
- **Directional**: four slots on the D-pad or face buttons.
- **List**: move with the D-pad or left stick; A chooses.
- **Button menu**: a list whose items can also be chosen directly with their own button.
- **Carousel**: cycle with the bumpers, triggers, D-pad or a stick; A chooses.
- **Grid**: a list laid out in up to 6 columns and 6 rows; move in all four directions with the D-pad or left stick, A chooses.

An item can open another menu of the same kind (radial menus can't open menus). The new menu is a child of the first: letting go closes both. While a menu is open the controller drives it, and anything the mappings were holding is released. A quick tap can't hold a menu up, so a tap-triggered menu (e.g. on a double tap that isn't the button's last gesture) needs a Toggle. Menus opened from the command line or an analog zone, which can't be held, stay until an item is chosen or B is pressed (Select for face-button menus). The overlay window stays running invisibly between uses, so menus appear instantly.

Each menu is a collapsible card on the Menus tab, with a live preview; "Add a menu" adds another at the top. Under **Appearance**, each menu (and the keyboard) has its own screen position (a 3×3 grid of corners, edges and center, so it fits any monitor size or aspect ratio), size (50–200%), and color and opacity for the background, items and selected item. Text switches between light and dark to stay readable. Older configs with `cascade` menus still load.

## Info overlays

Info overlays put text on screen without taking the controller, mainly to show a game's controls. Each one is a grid of cells (rows of cells that line up in columns), with its own position, size and colors. Cells can hold tokens:

- **Button glyphs** drawn the way the controller in use labels them: `{south}` is A on Xbox, ✕ on PlayStation and B on Nintendo; also `{east}` `{west}` `{north}` `{lb}` `{rb}` `{lt}` `{rt}` `{select}` `{start}` `{guide}` `{ls}` `{rs}` `{l3}` `{r3}` `{dpad}` `{up}` `{down}` `{left}` `{right}`. Controllers that can't be recognized get the "Fallback glyphs" kind chosen on the Settings page.
- **Live values**, updated every second: `{time}` `{time12}` `{date}` `{profile}` `{layer}` (the layers on, e.g. "Hotkeys + Build") `{app}` (the focused program's executable) `{title}` `{pid}` `{cpu}` `{ram}` `{gpu}` (AMD only) `{controller}`.

An info overlay is shown always while its game is active, for a few seconds when the game starts (the first time it's focused after launching, e.g. a "config loaded" note), or with the "Show info overlay…" action while that input is held (wrap it in Toggle to keep it up). An always-shown overlay can't also be mapped to an action; for one shown only sometimes, use an action (typically a Toggle). One shown by an action can **linger** for a few seconds after it's let go or toggled off. Timed ones fade out at the end. For an overlay that should stay up until dismissed, map "Toggle → Show info overlay…" to a button and tick **On when the game starts**: it shows at launch and goes when the button is pressed. Any Toggle can start on that way (a layer, a held key, …). Overlays at the same screen position stack.

## Gyro

Controllers with motion sensors (DualShock 4, DualSense, Switch Pro, Joy-Cons) can use their gyro in each profile:

- **Mouse**: gyro aiming, in pixels per degree turned.
- **Gamepad stick**: rotation speed deflects a virtual-pad stick, added to the physical stick. An anti-deadzone gets past the game's own stick deadzone.
- **Steering**: tilt the controller like a wheel to move a stick.

Horizontal aim can come from yaw (turning), roll (tilting) or both, and either axis can be inverted. Gyro can be **always on**, **on only while holding** an input (e.g. LT to aim down sights), **off while holding** (a clutch for repositioning the controller), or **toggled**. A **recenter** input sets the current tilt as straight for steering. Slow movement below a jitter threshold is scaled down to hide drift, and "Calibrate gyro" in the controller list measures the drift while the controller sits still. The PC action template turns on gyro mouse aiming while LT is held.

## Per-game profiles

Each game's rules (on its Details tab) switch to one of its profiles when the game's window gets focus, and when it loses focus the default profile (on the Settings page; General › Gamepad unless changed, or "keep current profile") takes over. Each switch, and each game launch, shows a short toast at the top of the screen naming the controller profile now active and its game; switching back to the default when a game loses focus doesn't. Rules are checked game by game, in order; a rule can be switched off without deleting it. A rule matches on:

- **Executable**: the program's file name. For Wine/Proton games it's the Windows `.exe` (e.g. `eldenring.exe`), not the Wine loader.
- **Steam App ID**: taken from the environment Steam sets, or from Proton's `steam_app_<id>` window class.
- **Window class**.

A game's Details tab lists recently focused windows with a one-click "+ Rule". Switching with Guide or the GUI stays in effect until focus changes again. Focusing the settings window never switches profiles.

Focus tracking depends on the desktop; the daemon picks the right one by itself and logs which one it uses (`controller_app status` shows it):

- **KDE Plasma** (Wayland or X11): a small KWin script.
- **Sway**: the compositor's IPC socket (`$SWAYSOCK`).
- **Hyprland**: its event and command sockets.

GNOME is not supported: Mutter has no layer-shell, so the on-screen keyboard, menus and overlays can't be shown, and it lets no other program ask for the focused window. On other desktops, rules apply while a matching process is running.

## GUI

- **Overview**: a live controller drawing labelled with the active profile's mappings, and the controller list (Manage, Test rumble, Calibrate gyro).
- **Settings**: automatic switching and its default profile, the on-screen keyboard and numpad, and fallback glyphs.
- **A game's Profiles tab**: the profile being edited, with its own labelled drawing and sub-tabs for Buttons, Sticks & triggers, Combos and Gyro. Mappings collapse to one-line summaries ("A ▸ Left click"); click a name to edit it. **Find by pressing** jumps to whatever you press or push on the controller.
- **A game's Layers tab**: the game's layers, edited like a profile (see Layers).
- **A game's Macros tab**: step editor with exact millisecond fields.

Problems that would block saving (unknown keys, missing macros, incomplete combos) are flagged on the row and with ⚠ on its tab. Section explanations sit behind ⓘ tooltips, and keys can be picked from an on-screen keyboard.

## CLI

`controller_app status | enable | disable | profile <name> | next-profile | reload | overlay-toggle | numpad-toggle | menu <name> | daemon`

`profile <name>` looks in the active game, then General, then the first game with a profile of that name. `menu <name>` opens one of the active game's menus, or a shared one.

When `controller_app daemon` runs in a terminal, it keeps a live status line showing each controller's input and what is being output.

## Notes

- PlayStation and Switch controllers are also readable through hidraw, which SDL (and Steam) use directly. Games may then see the real controller as well as ours. Turn off Steam Input for the pad, or set `SDL_JOYSTICK_HIDAPI=0` for the game.
- Virtual uinput devices, such as Steam Input's pad and our own, are skipped. For a pad this app manages, turn off Steam Input or games may see two controllers.
- Pads on the `xpad` driver report X/Y by label rather than position; this is corrected automatically.
- Rumble from games is forwarded to the physical controller when it supports force feedback. The end-to-end check needs `/dev/uinput`, so it is opt-in: `cargo test -- --ignored rumble`.

## License

GPL-3.0. See [LICENSE](LICENSE).

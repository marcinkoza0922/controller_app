# padwight

Ever wanted to play a PC game with a controller, only to find it ignores your pad, or only half-listens to it? padwight is for that. It sits between your gamepad and the desktop and turns what the pad does into something the game understands: keyboard keys, mouse movement, clicks, or a virtual controller of its own.

It's built for two kinds of game:

- **Games with no controller support, or only partial support.** Point a stick at the mouse or at WASD, put a macro on a button, give a trigger a mouse click, or send a whole combo of keys on one press. Missing a button the game never bound? Want gyro aiming on a pad that has a gyro? padwight can fill those gaps. Games that play well with a pad often still have a launcher that wants a mouse, or a name field that assumes you're sitting at a keyboard, and padwight covers those too (see [Mouse and keyboard from the couch](#mouse-and-keyboard-from-the-couch)).
- **Games that already work with a controller, but could go further.** Remap buttons on the fly, in the middle of a game, without leaving it. Where a game allows it, add gyro aiming or a flick-stick setup on a controller that has the sensors. Use macros to pull off complex inputs, like fighting-game motions, with one press. And control media playback from the pad while you play.

It runs quietly in the background and has a settings window for the fiddly parts. Your setups are plain files, so they're easy to back up, share, or copy to another machine.

Here's how it's put together:

- **daemon**: grabs physical gamepads (evdev) so nothing else sees them, runs the active profile, and emits through virtual uinput devices (an Xbox 360-style pad plus a keyboard and a mouse).
- **gui**: edits profiles and talks to the daemon over `$XDG_RUNTIME_DIR/padwight.sock`.
- **config**: `~/.config/padwight/`. `config.toml` holds the app settings. Each setup is a folder under `setups/` (General is `general/`, shared items are `shared/`), with a `setup.toml` and one file per profile, layer, macro, menu, overlay and log. To move a setup to another machine, copy its folder. The daemon is the only writer while it runs, so after editing by hand, run `padwight reload`.

## Documentation

For users:

- **Manual**: a plain-language guide, shown on the Manual page of the settings window. Its source is [docs/manual/](docs/manual/), plain Markdown that is also built into the app, so the two can't drift apart.
- [docs/tutorial-deus-ex-pack.md](docs/tutorial-deus-ex-pack.md): a step-by-step walkthrough of making a pack, using Deus Ex on an Xbox-style pad, with the reasons behind the less obvious mappings.
- [docs/pack-format.md](docs/pack-format.md): reference for the `.padpack` format (format 13).

For developers, in [docs/development/](docs/development/):

- [architecture.md](docs/development/architecture.md): how the processes, input pipeline, config and focus tracking fit together, with a module map and test commands.
- [specs.md](docs/development/specs.md), [layers.md](docs/development/layers.md), [sticks.md](docs/development/sticks.md), [guide.md](docs/development/guide.md): design notes for individual features.
- [game-setup-guidelines.md](docs/development/game-setup-guidelines.md): where actions usually go when setting up a game's bindings (sticks, buttons, hotkeys, system buttons).

## Setups

Everything is organized into setups: one controller setup per game. Each game keeps its own bindings and switches to them when you start it. A setup holds its own profiles, macros, menus, info overlays, and the auto-switch rules that activate it. All of its profiles can use all of its items. **General** is a built-in setup with no rules, for the desktop and a plain gamepad. Its Macros, Menus and Info overlays tabs also hold the **shared** items, which every profile of every setup can use. Names only need to be unique within a setup, and a setup's items can't reuse a shared item's name.

The sidebar lists General and your setups (with a search). Each setup's page has five sub-tabs: **Profiles**, **Macros**, **Menus**, **Info overlays** and **Details** (name, rules, export, delete). On the Macros, Menus and Info overlays tabs, "Copy from another setup…" previews another setup's items (or one from the library) and copies the ones you want in, so you can adapt them.

## Packs and the library

Found a setup worth sharing? Export it as a **pack**: **Details → Export…** writes a `.padpack` file (TOML) and copies in any shared items it uses. The export dialog warns you about references to missing items.

Whether a pack needs more than a plain XInput pad (gyro, or back paddles) is something you declare on each **profile**, not on the pack. On the profile's Gyro tab, tick *Can't be played without gyro* when it depends on gyro (a flick stick setup that turns up and down with gyro, say). Leave it unticked when gyro only adds to a scheme that works without it. The back paddles section of the Buttons tab has the same box. It's your word, nothing is detected, and the export dialog just lists what you've declared. Players whose controller lacks a feature only see the profiles that don't need it, and Guide skips the others. Re-exporting your own setup keeps its pack ID. Exporting someone else's makes a fork with its own ID that credits the original.

**+ Add setup** opens the picker. It offers the built-in library first (setups for games you have installed in Steam, Heroic or Lutris, or that are running), an empty setup from a template, or **Import a file…**. Importing shows a preview before anything changes: what's inside, what your controller lacks, whether a setup with the same name already exists (add under a new name, or replace yours), any shared items it renames to avoid clashes, and other setups' rules for the same window (the imported game takes over unless you keep yours). Nothing changes until you choose Save & apply.

Imported setups are ordinary, editable setups that remember their pack. When a newer version of the same pack turns up, or an app update ships a newer library version (marked "update" in the sidebar), you're offered the update, along with a list of what you've changed since. A pack made by a newer app version is refused, with a request to update the app.

The library is `packs/*.padpack` in this repository, embedded at build time. See `packs/README.md`.

## Install

```sh
cargo install --path .
mkdir -p ~/.config/systemd/user
cp dist/padwight.service ~/.config/systemd/user/
systemctl --user enable --now padwight
padwight            # open the GUI
```

For an application-menu entry and icon for the settings window:

```sh
install -Dm644 flatpak/io.github.marcinkoza0922.Padwight.svg ~/.local/share/icons/hicolor/scalable/apps/io.github.marcinkoza0922.Padwight.svg
install -Dm644 flatpak/io.github.marcinkoza0922.Padwight.desktop ~/.local/share/applications/io.github.marcinkoza0922.Padwight.desktop
```

Controllers can be unplugged, reconnected or swapped while padwight runs, and it copes. A disconnect releases everything that controller was holding and closes a menu it had open. A toast names the controller that left or arrived. Your profile stays put, so reconnecting the same pad picks up where you left off.

If the pad that comes back (or replaces it) lacks something the active profile needs, such as gyro, the setup's first profile that doesn't need it takes over. If none does, the default profile takes over, and a toast tells you so. When a pad with the feature is connected again, your profile returns, unless you've changed it since. The switch only happens once the controllers have looked the same for two 2-second scans, so a motion sensor that appears a moment after its pad won't make the profile flicker.

On most desktops (systemd-logind with `uaccess`), your user can already open `/dev/uinput` and the gamepad nodes, so no root is needed. If yours can't, add a udev rule that grants access, or add yourself to the `input` group.

For **gyro** on PlayStation and Switch controllers, also install the motion-sensor rule. Their motion sensors show up as a separate device that `uaccess` doesn't cover:

```sh
sudo cp dist/70-padwight-motion.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

### Flatpak

`flatpak/io.github.marcinkoza0922.Padwight.yml` builds the same binary as a Flatpak. Run these from the checkout:

```sh
flatpak install --user flathub org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08 org.flatpak.Builder
flatpak run org.flatpak.Builder --user --force-clean --disable-rofiles-fuse --install-deps-from=flathub --repo=repo build-dir flatpak/io.github.marcinkoza0922.Padwight.yml
flatpak remote-add --user --no-gpg-verify --if-not-exists padwight-local file://$PWD/repo
flatpak install --user --reinstall padwight-local io.github.marcinkoza0922.Padwight
flatpak run io.github.marcinkoza0922.Padwight          # open the GUI
```

`--disable-rofiles-fuse` is only needed when flatpak-builder itself runs inside a sandbox. `cargo-sources.json` lists the crates the build downloads. Regenerate it with flatpak-builder-tools' `flatpak-cargo-generator.py` whenever `Cargo.lock` changes.

To run the daemon at login, copy `dist/padwight-flatpak.service` to `~/.config/systemd/user/` and enable it. The Flatpak differs from the native install in a few ways:

- Settings live in `~/.var/app/io.github.marcinkoza0922.Padwight/config/padwight/`. To keep your existing setups, copy `~/.config/padwight/` there.
- The motion-sensor rule is a host file, so install it with the `sudo` commands above. The app can't run `sudo` itself.
- Game detection reads Steam and Lutris libraries read-only. Other library folders need `flatpak override --user --filesystem=/path:ro io.github.marcinkoza0922.Padwight`.
- The sandbox has its own process list, so a window is identified by its class and title only, not by its executable. Focus tracking works through KWin, GNOME and Hyprland. On GNOME, the sandbox can reach the whole Shell D-Bus interface (`--talk-name=org.gnome.Shell`), and it writes the extension into `~/.local/share/gnome-shell/extensions`, which is what lets it install and enable the extension.
- Sway's IPC socket isn't shared into the sandbox, so on Sway and labwc (and other wlroots compositors) focus falls back to matching running processes. Hyprland's socket is shared, so focus works there. On all wlroots compositors, including Hyprland, the compositor hides its layer-shell protocol from sandboxed apps, so the on-screen keyboard and overlays don't show in a Flatpak. Run the app natively there for those.
- Screenshot and recording actions run host programs (grim, gpu-screen-recorder, …), which the sandbox doesn't include, so they may not work.
- `--device=all` is needed so the sandbox can reach `/dev/uinput`. That lets the app read and send input on the machine, just like the native daemon.

## Profiles

A profile is the heart of a setup: the mappings you use while playing. Each profile maps:

- **buttons**: to a gamepad button, a key or key combo (`LEFTCTRL+C`), a mouse button, the scroll wheel (one notch per press, continuous while held), or **several of these at once**.
- **back paddles**: on controllers that have them (Xbox Elite, DualSense Edge, Steam Deck), the paddles get their own section under the buttons and map like any other button. Each profile says whether it needs them, just like gyro.
- **sticks**: to a gamepad stick, the mouse pointer, the scroll wheel, a flick stick, or a button ring (its sectors can press keys, e.g. WASD or the arrows). Each has a deadzone, a response curve and an adjustable press threshold. Mouse sticks also get **acceleration** (speed grows while held at full push), **Invert Y**, and under *Advanced response*, a ramp time, an outer-edge boost, a vertical speed ratio and smoothing. Scroll sticks get **Invert Y** too. Each stick's **directions also act as buttons** of their own (Left Stick Up, …): they can have any action or gesture, and can be part of combos, e.g. LB + Right Stick Right. Gamepad outputs include stick directions too, so a button (or the D-pad) can push the virtual stick, and two directions make a diagonal.
- **button rings**: a stick can instead act as a ring of 4, 8 or 12 sectors. Pointing it into a sector holds that sector's action (any action a button can have). You can adjust the angle of the first sector, how far out the stick must go, and how sticky the sector boundaries are.
- **flick stick**: push a stick out and the camera turns that way at once (spread over about 0.1 s). Rotate the stick and the camera turns by the same angle. It's horizontal only, so aim up and down with gyro or the other stick. Set *Turn size* to the mouse pixels one full turn takes in the game, then press **Test turn**, which moves the mouse one turn after 3 seconds so you can check. Only one stick per profile can be a flick stick. Advanced options: flick time, turning smoothing, a dead angle straight ahead, and optional looking up and down.
- **triggers**: to an analog gamepad trigger, or to anything a button can do (a gamepad button, key, mouse button, macro, …), pressed once the trigger is pulled past a threshold.

On top of that, you get some handy extras:

- **Toggle and Turbo**: wrap any action in a toggle (press once to hold it down, press again to release, e.g. for hold-to-crouch games), or in turbo (repeats 2–30 times a second while held). A macro inside a turbo plays every set number of milliseconds, never faster than the macro takes. They combine nicely: Toggle → Turbo is auto-fire you can switch on and off.
- **Combos**: buttons pressed together (e.g. LB+RB) act as their own input. Combo members wait a short window (default 60 ms) for the rest of the combo. A member whose own action is *Disabled* works as a modifier, with no time limit.
- **Gestures**: double tap, triple tap and long press, per button. The final tap of a sequence fires on press and holds, so "double-tap and hold" works. A quick tap on a button with gestures fires once the gesture is decided. Holding past the tap window presses the button's own action right away and holds it until release, unless a long press is set.
- **Zones**: extra actions that are held while a stick or trigger is within part of its travel. For example, Left Shift on a partial stick push gives you walk and run with WASD, and a half versus full trigger pull can do two different things. Zones are hidden for controllers whose triggers are simply on or off (e.g. Switch pads).

General's defaults are **Gamepad** (1:1 passthrough) and **Desktop** (left stick moves the mouse, right stick scrolls, A/B click, LB+RB = Alt+Tab). Tapping Guide opens the Guide overlay. Profiles switch from Quick Settings (Guide + Start) or the app, never from the controller alone.

Want a head start? These are under "New from template…" in the GUI:

- **PC action**: WASD on the left stick (Shift at full push to sprint), mouse look on the right stick, Mouse 1/2 on RT/LT, A = E (use), B = Space, X = R, Y = F, LB/RB = Q/G, L3/R3 = Ctrl/V, D-pad = 1–4, Start/Select = Esc/Tab.
- **Strategy**: mouse pointer on the left stick, arrow-key camera on the right, A/B/X = left/right/middle click, RT = left click for drag-select, L3/R3 = zoom (wheel up/down), LB/RB = held Ctrl/Shift, D-pad = control groups 1–4.
- **Retro / platformer**: arrows on the D-pad and left stick, A/B/X/Y = Z/X/C/V, for keyboard-only indie games and emulators.

## Layers

A layer changes some of your mappings while it's on, then changes them back when it's off. For example, hold LB and the face buttons type F1–F4 and the right stick scrolls. Layers belong to a setup (on its **Layers** tab) and apply over whichever of its profiles is active. Anything a layer doesn't set stays as it was in the profile. A layer can override buttons and stick directions, gestures, sticks, triggers (with their zones) and gyro. It can also add combos, or switch the profile's combos off.

- **Turning one on**: map "Layer…" to any button, trigger, stick direction, zone, gesture or combo. The layer is on while that input is held. Wrap it in **Toggle** and it stays on until you press again, which also lets a menu item switch it (menu items can only toggle layers). Layers can turn on further layers.
- **Several at once**: layers stack, and the newest one wins where they disagree.
- **Switching over**: an input that's held down when a layer comes on or goes off keeps doing what it started until released. Sticks, triggers and gyro switch modes right away. Held layers end when the profile changes or an on-screen menu or keyboard opens. Toggled ones stay on within the game.
- **Showing it**: each layer can show its name on screen while it's on (with its own position, size and colors), or show one of the setup's info overlays (e.g. a cheat sheet), or nothing at all for quick ones like "hold Y to lean". The live controller drawing labels inputs with the layers that are on, as they act.
- **Editing**: the Layers tab uses the profile editor, shown over one of the setup's profiles. Inputs the layer doesn't set read "Same as Gameplay: …" with an **Override** button. Overridden ones get **Back to base** to undo. Layers use their profile's timings. "Copy from another setup…" copies a layer along with the macros, menus and info overlays it uses, which is handy for sequels. Shared items can't use layers.

## Macros

A setup's Macros tab holds named input sequences, each in its own collapsible card ("+ New macro" adds one at the top). Each step is one of:

- **tap**: press, hold for N ms, release
- **hold down** or **release**
- **wait**: N ms
- **move stick**: sends a virtual-pad stick to a direction or custom position, held until the next stick step, and recentered when the macro ends

Steps can press a key or key combo, a mouse button, the scroll wheel or a gamepad button. "Insert motion…" adds fighting-game motions one frame apart: quarter circles, dragon punch and half circles, written facing right.

Map a macro with the "Macro…" action, and pick how it plays:

- **Play once**: each press plays it through to the end, even if you let go early.
- **Repeat while held**: loops until you let go, then stops at once.

Each pass of a macro releases anything it still holds when it ends. Wrap a repeating macro in a Toggle to loop it hands-free. Renaming a macro updates its mappings, and profile switches stop any macros that are running.

## Mouse and keyboard from the couch

A game can work fine with a controller and still trip you up. Its launcher may need a mouse to pick settings or a server, or its name entry may assume you're at a desk with a keyboard. For those moments, tap Guide to open its overlay: the notes the game's author wrote, a menu, and what each button does. The menu has the on-screen keyboard and numpad, media controls, Quick Settings and, while Steam runs, the Steam overlay.

Hold Guide and press X, Y, LB, RB, L3 or Select for a shortcut (keyboard, numpad, media, screenshot, recording, force quit). Hold Guide and press right stick, or press right stick with the overlay open, for mouse mode: the controller moves the pointer, clicks, scrolls and sends Enter, Tab, Escape and modifier keys.

The full list is in the manual's [Guide button](docs/manual/guide-button.md) page. The shortcuts are the same in every game. Each profile's **Guide** tab sets the overlay's notes and how its mapping list reads.

## On-screen overlay

The "On-screen keyboard" action (also a button on the Overview tab, and `padwight overlay-toggle`) opens a keyboard over everything, including fullscreen games. It's a Wayland layer-shell surface that never takes keyboard focus, so your keys still go to the window underneath. While it's open, the controller drives it:

| Control | Action |
|---|---|
| D-pad / left stick | move between keys (repeats while held) |
| A | press the selected key (holding A holds the key) |
| Shift / Ctrl / Alt / Super | latch for the next key |
| X / Y / Start | Backspace / Space / Enter |
| hold LT | hold Shift |
| hold B | close |

Guide + X, or X with the Guide overlay open, opens the keyboard. It needs a compositor with layer-shell (KDE Plasma, Sway, Hyprland, …).

The "On-screen numpad" action (or `padwight numpad-toggle`) opens a smaller pad with the digits 0–9 and a dot, for codes and number fields. Move with the D-pad or left stick, press A for a number, X for backspace, Start for Enter, and hold B to close. It types the top-row number keys, so it works whatever the Num Lock state. Both pads have their own position, size and colors on the App settings page, and opening one closes the other.

## Menus

A setup's Menus tab holds on-screen **action menus**. Open one from any button, gesture, combo, trigger or stick direction with the "Open menu…" action (or `padwight menu <name>`). A menu stays on screen only while its input is held, and letting go closes it. To keep a menu up without holding, wrap the action in "Toggle…", and it stays until you press the input again. Choosing an item taps its action like a button press, so items can be keys, macros, toggles or the keyboard. The menu stays up for more picks.

Pick the style that suits the job:

- **Radial**: aim a stick at an item. Whatever you're aiming at when you let go is chosen.
- **Directional**: four slots on the D-pad or face buttons.
- **List**: move with the D-pad or left stick, A chooses.
- **Button menu**: a list whose items can also be chosen directly with their own button.
- **Carousel**: cycle with the bumpers, triggers, D-pad or a stick, A chooses.
- **Grid**: a list laid out in up to 6 columns and 6 rows. Move in all four directions with the D-pad or left stick, A chooses.

An item can open another menu of the same kind (radial menus can't open menus). The new menu is a child of the first, so letting go closes both. While a menu is open, the controller drives it, and anything the mappings were holding is released. A quick tap can't hold a menu up, so a menu opened by a tap (e.g. on a double tap that isn't the button's last gesture) needs a Toggle. Menus opened from the command line or an analog zone can't be held, so they stay until you choose an item or press B (Select for face-button menus). The overlay window stays running invisibly between uses, so menus appear instantly.

Each menu is a collapsible card on the Menus tab, with a live preview. "Add a menu" adds another at the top. Under **Appearance**, each menu (and the keyboard) gets its own screen position (a 3×3 grid of corners, edges and center, so it fits any monitor size or aspect ratio), a size (50–200%), and a color and opacity for the background, items and selected item. Text switches between light and dark to stay readable. Older configs with `cascade` menus still load.

## In-game menu

Press Guide + Start together to open a menu over the game in front. **Quick Settings** changes a few handy values, such as stick and gyro sensitivity, Invert Y and the profile. **Edit Controls** changes that setup's buttons, sticks, triggers, gyro, combos, layers, macros and zones. Changes apply right away and are saved when you choose Save. With no game in front, Edit Controls is refused. Quick settings instead makes a setup for the window in front.

The wiki has the details: [the in-game menu](https://github.com/marcinkoza0922/padwight/wiki/In-Game-Menu) and [editing controls in game](https://github.com/marcinkoza0922/padwight/wiki/Editing-Controls-in-Game).

## Info overlays

Info overlays put text on screen without taking over the controller, mostly to show a game's controls. Each one is a grid of cells (rows of cells that line up in columns), with its own position, size and colors. Cells can hold tokens:

- **Button glyphs**, drawn the way the controller in use labels them. `{south}` is A on Xbox, ✕ on PlayStation and B on Nintendo. Also `{east}` `{west}` `{north}` `{lb}` `{rb}` `{lt}` `{rt}` `{select}` `{start}` `{guide}` `{ls}` `{rs}` `{l3}` `{r3}` `{dpad}` `{up}` `{down}` `{left}` `{right}`. Controllers that can't be recognized get the "Fallback glyphs" kind chosen on the App settings page.
- **Live values**, updated every second: `{time}` `{time12}` `{date}` `{profile}` `{layer}` (the layers on, e.g. "Hotkeys + Build"), `{app}` (the focused program's executable), `{title}`, `{pid}`, `{cpu}`, `{ram}`, `{gpu}` (AMD only), `{wifi}` (signal strength, with its icon), `{system_battery}` (a laptop's or handheld's charge), and `{controller_battery}` (the controller in use's charge, where it reports one; `~` when it's estimated from a coarse level).
- **Icons**: `{pc}` (a monitor, or a laptop or handheld shape) and `{controller}` (the controller in use, drawn for its kind), meant to sit beside a battery. Add `:icon` to `{system_battery}` or `{controller_battery}` for a gauge instead of the number, and use `{controller:name}` for the controller's name.

An info overlay can appear in three ways: always while its setup is active, for a few seconds when the game starts (the first time it's focused after launching, e.g. a "config loaded" note), or while an input is held via the "Show info overlay…" action (wrap it in Toggle to keep it up). An always-shown overlay can't also be mapped to an action. For one that appears only sometimes, use an action, typically a Toggle. An overlay shown by an action can **linger** for a few seconds after it's let go or toggled off, and timed ones fade out at the end.

For an overlay that should stay up until you dismiss it, map "Toggle → Show info overlay…" to a button and tick **On when the game starts**. It shows at launch and goes away when you press the button. Any Toggle can start on that way (a layer, a held key, …). Overlays at the same screen position stack.

## Gyro

Controllers with motion sensors (DualShock 4, DualSense, Switch Pro, Joy-Cons) can use their gyro in each profile:

- **Mouse**: gyro aiming, in pixels per degree turned.
- **Gamepad stick**: rotation speed deflects a virtual-pad stick, added to the physical stick. An anti-deadzone gets past the game's own stick deadzone.
- **Steering**: tilt the controller like a wheel to move a stick.

Horizontal aim can come from yaw (turning), roll (tilting) or both, and either axis can be inverted. Gyro can be **always on**, **on only while holding** an input (e.g. LT to aim down sights), **off while holding** (a clutch for repositioning the controller), or **toggled**. A **recenter** input sets the current tilt as straight for steering. Slow movement below a jitter threshold is scaled down to hide drift. "Calibrate gyro" in the controller list measures the drift while the controller sits still.

The PC action template turns on gyro mouse aiming while LT is held. StarCraft's Gameplay profile has a "+ Gyro" twin that moves the cursor by tilting, as an extra to the left stick. The library's shooters (Deus Ex, F.E.A.R., Max Payne 1 and 2) each ship three profiles: one without gyro, one with gyro as an extra, and a flick stick one that needs gyro. Their rules start on the plain one. If you pick another when the Guide offer appears, that one becomes the setup's.

## Automatic profile switching

Each setup's rules (on its Details tab) switch to one of its profiles when the game's window gets focus. When the game loses focus, the default profile takes over. You set that on the App settings page: General › Gamepad unless you change it, or "keep current profile". Each switch, and each game launch, shows a short toast at the top of the screen naming the controller profile now active and its setup. Switching back to the default when a game loses focus doesn't toast.

Rules are checked setup by setup, in order, and you can switch a rule off without deleting it. A rule matches on:

- **Executable**: the program's file name. For Wine/Proton games it's the Windows `.exe` (e.g. `eldenring.exe`), not the Wine loader.
- **Steam App ID**: taken from the environment Steam sets, or from Proton's `steam_app_<id>` window class.
- **Window class**.

A setup's Details tab lists recently focused windows, each with a one-click "+ Rule". Switching with Guide or the GUI stays in effect until focus changes again. Focusing the settings window never switches profiles.

Focus tracking depends on your desktop. The daemon picks the right method by itself and logs which one it uses (`padwight status` shows it):

- **KDE Plasma** (Wayland or X11): a small KWin script.
- **Sway**: the compositor's IPC socket (`$SWAYSOCK`).
- **Hyprland**: its event and command sockets.
- **wlroots compositors** (labwc, Wayfire, river, and others that offer `wlr-foreign-toplevel-management`): the protocol's window list. It doesn't say which process owns a window, so these match windows by class only, and executable-name rules don't match.
- **GNOME** (Mutter): a small GNOME Shell extension, which the daemon installs and enables itself. A log out and back in may be needed the first time.

GNOME can't show the on-screen keyboard, menus or overlays, because Mutter has no layer-shell. Focus tracking works, through the extension. On other desktops, rules apply while a matching process is running.

## GUI

- **Overview**: a live controller drawing labelled with the active profile's mappings, *What's happening* (the latest presses and what each one did, plus each profile switch and the rule or action behind it), and the controller list (Manage, Test rumble, Calibrate gyro).
- **Settings**: automatic switching and its default profile, the on-screen keyboard and numpad, fallback glyphs, and colour-blind mode for menu colours.
- **A setup's Profiles tab**: the profile you're editing, with its own labelled drawing and sub-tabs for Buttons, Sticks & triggers, Combos and Gyro. Mappings collapse to one-line summaries ("A ▸ Left click"). Click a name to edit it. **Find by pressing** jumps to whatever you press or push on the controller.
- **A setup's Layers tab**: the setup's layers, edited like a profile (see Layers).
- **A setup's Macros tab**: a step editor with exact millisecond fields.

Problems that would block saving (unknown keys, missing macros, incomplete combos) are flagged on the row and with ⚠ on its tab. Section explanations sit behind ⓘ tooltips, and keys can be picked from an on-screen keyboard.

## CLI

`padwight status | enable | disable | profile <name> | next-profile | reload | overlay-toggle | numpad-toggle | menu <name> | daemon`

`profile <name>` looks in the active setup first, then General, then the first setup with a profile of that name. `menu <name>` opens one of the active setup's menus, or a shared one.

When `padwight daemon` runs in a terminal, it keeps a live status line showing each controller's input and what's being output.

Opening the GUI starts the daemon in the background if it isn't already running. Its output goes to `~/.local/state/padwight/daemon.log`, and the daemon keeps running after the window closes. Stopping the daemon closes the window.

Outside systemd (in a terminal, or from an autostart entry), the daemon also puts an icon in the system tray. Clicking it opens the settings window, and its menu has a Remapping switch, Open settings, and Quit. Under systemd, the unit manages the daemon, so no icon is shown. Desktops without a tray (such as GNOME without an extension) show no icon, and the daemon works fine without one. In a Flatpak, the tray needs the `org.kde.StatusNotifierWatcher` permission, which the manifest grants.

## Good to know

- PlayStation and Switch controllers are also readable through hidraw, which SDL (and Steam) use directly. Games may then see the real controller as well as ours. Turn off Steam Input for the pad, or set `SDL_JOYSTICK_HIDAPI=0` for the game.
- Virtual uinput devices, such as Steam Input's pad and padwight's own, are skipped. For a pad this app manages, turn off Steam Input, or games may see two controllers.
- Pads on the `xpad` driver report X/Y by label rather than position. This is corrected automatically.
- Rumble from games is forwarded to the physical controller when it supports force feedback. The end-to-end check needs `/dev/uinput`, so it's opt-in: `cargo test -- --ignored rumble`.

## License

GPL-3.0. See [LICENSE](LICENSE).

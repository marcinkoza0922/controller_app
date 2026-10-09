# `.padpack` format reference

A pack is one game in one file: TOML, in the same shapes the app writes to `config.toml`. This page describes **format 9**, the format the current app writes. For a walkthrough of building one, see [tutorial-deus-ex-pack.md](tutorial-deus-ex-pack.md). The shipped examples are in `packs/`.

The easiest way to write a pack is in the app, then **Details → Export…**. Read this page when you want to edit the file by hand, check a value, or understand what the export produced.

## Top level

```toml
format = 9                       # pack format version (required)
macros = []                      # optional, see Macros
overlay_font = "Rajdhani"        # optional, font for overlays, menus and keyboards

[pack]                           # required
id = "744efa38-…"                # required, stable across versions
name = "Deus Ex"                 # required, becomes the game's name
version = "1.1"
author = "padwight"
description = "…"
made_with = "Xbox controller"

[[rules]]                        # auto-switch rules (see Rules)
[[profiles]]                     # required, at least one
[[macros]]                       # optional
[[menus]]                        # optional
[[info_overlays]]                # optional
[[layers]]                       # optional
[keyboard_style]                 # optional, on-screen keyboard look
[numpad_style]                   # optional, on-screen numpad look
[media_style]                    # optional, media controls look
[menu_style]                     # optional, in-game menu and Edit Controls look
```

Unknown top-level keys are an error, so a typo fails the import rather than being ignored. `log_overlays` is also accepted.

### `[pack]`

| Key | Meaning |
|---|---|
| `id` | A UUID. Keeps the game recognised across versions; a re-export keeps it, and a fork gets a new one. |
| `name` | The game name. |
| `version` | Free text, compared as versions (`1.2` is newer than `1.1`). |
| `author`, `description`, `made_with` | Shown in the import preview. `made_with` is informational, e.g. `"Xbox controller"`. |
| `based_on` | Only on forks: a table with `id`, `name`, `author`, `version` of the original. |

### Rules

Rules switch the game's profile when a matching window gets focus.

```toml
[[rules]]
kind = "executable"       # "executable" | "steam_app_id" | "window_class"
value = "DeusEx.exe"
profile = "Deus Ex"       # must be a profile of this pack
```

- `executable`: the program's file name. For Wine/Proton games this is the Windows `.exe`.
- `steam_app_id`: the Steam App ID, as a string.
- `window_class`: the window's class.
- `enabled = false` turns a rule off without deleting it. It is written only when false.

## Profiles

```toml
[[profiles]]
name = "Deus Ex"
combos = []
combo_window_ms = 120    # default 60
tap_window_ms = 250      # default 250: longest gap that still continues a double/triple tap
long_press_ms = 500      # default 500: hold time for a long press
requires = []            # optional, e.g. ["gyro"]; see Requirements

[profiles.buttons]       # one entry per button; unset buttons do nothing
South = "disabled"
```

Buttons are: `South`, `East`, `North`, `West` (the face buttons, A/B/Y/X or ✕/○/△/□), `LeftBumper`, `RightBumper`, `LeftStick`, `RightStick`, `Select`, `Start`, `Guide`, `DpadUp`, `DpadDown`, `DpadLeft`, `DpadRight`. The stick directions are also buttons: `LeftStickUp`, `LeftStickDown`, `LeftStickLeft`, `LeftStickRight`, and the same with `RightStick…`. Triggers are not buttons; they have their own section below.

### Actions

A button's value is an **action**. Each form is one TOML key:

| Form | Example | What it does |
|---|---|---|
| `"disabled"` | `DpadUp = "disabled"` | Nothing. Also what an unset button does. |
| `keys` | `keys = ["KEY_LEFTCTRL", "KEY_C"]` | Presses the evdev keys together, releases together. |
| `mouse` | `mouse = "Right"` | A mouse button: `Left`, `Right`, `Middle`, `Back`, `Forward`. |
| `wheel` | `wheel = "Up"` | One wheel notch per press, and keeps scrolling while held. `Up`, `Down`, `Left`, `Right`. |
| `gamepad` | `gamepad = "Guide"` | Sends a gamepad button to the virtual pad, with any button name above. |
| `"next_profile"` | | Switches to the next profile of the game. |
| `"toggle_overlay"`, `"toggle_numpad"` | | Opens or closes the on-screen keyboard or numpad. |
| `"screenshot"`, `"toggle_recording"`, `"toggle_media"`, `"force_quit"` | | The Guide-layer actions: screenshot, start or stop recording, media controls, force-quit the focused game. |
| `multi` | `multi = [{ layer = "Guide" }, { show_info = "Controls" }]` | Several actions at once, pressed in order and released in reverse. |
| `toggle` | `[profiles.buttons.LeftStick.toggle]` `keys = ["KEY_X"]` | First press holds the inner action, next press releases it. |
| `turbo` | `turbo = { action = { keys = ["KEY_F"] }, rate = 10.0 }` | Repeats the inner action 2–30 times a second while held. |
| `macro` | `macro = { name = "QCF", repeat = false }` | Plays a macro. `repeat = true` loops while held. |
| `layer` | `layer = "Guide"` | Holds a layer on while the input is held. |
| `show_info` | `show_info = "Controls"` | Shows an info overlay while held. |
| `show_log` | `show_log = "Inputs"` | Shows a log overlay while held. |
| `open_menu` | `open_menu = "Belt"` | Opens a menu while held. |

Toggle with a start state: `toggle = { action = { keys = ["KEY_X"] }, start_on = true }`. Without `start_on`, a toggle starts off and is written as the inner action only.

Gestures and combos are attached to a button:

```toml
[profiles.gestures.LeftBumper.double_tap]
keys = ["KEY_BACKSPACE"]

[profiles.gestures.Start.long_press]
keys = ["KEY_KPSLASH"]

[[profiles.combos]]        # buttons pressed together act as their own input
buttons = ["LeftBumper", "RightBumper"]
action = { keys = ["KEY_LEFTALT", "KEY_TAB"] }
```

- Gesture kinds: `double_tap`, `triple_tap`, `long_press`.
- A combo needs at least two buttons. Members wait `combo_window_ms` for the rest of the combo before acting alone. A member whose own action is `"disabled"` works as a modifier with no time limit.

Key names are evdev names (`KEY_A`, `KEY_LEFTSHIFT`, `KEY_KPPLUS`, `KEY_RIGHTBRACE`, `KEY_APOSTROPHE`, …). The keyboard overlay in the app lists them.

## Sticks

```toml
[profiles.left_stick]
deadzone = 0.12          # radial deadzone, 0.0–1.0
curve = 1.0              # response exponent for mouse and scroll: 1.0 linear, higher = finer at small pushes (default 2.0)
key_threshold = 0.25     # push at which direction keys and direction buttons press (default 0.5)

[profiles.left_stick.action.keys]
up = "KEY_W"
down = "KEY_S"
left = "KEY_A"
right = "KEY_D"
```

The `action` table picks what the stick does:

| Form | Keys | Meaning |
|---|---|---|
| `disabled` | | Nothing. |
| `gamepad` | `gamepad = { stick = "Left", invert_y = false }` | Feeds a virtual-pad stick. |
| `mouse` | `mouse = { speed = 1600.0 }` | Pointer movement; `speed` is pixels per second at full push. Optional: `accel`, `accel_ramp_ms`, `y_scale`, `outer_boost`, `smoothing_ms`, `invert_y`. |
| `scroll` | `scroll = { speed = 4.0, invert_y = false }` | Wheel notches per second at full push. `invert_y` flips the direction (stick up scrolls down). |
| `keys` | the table above | Direction keys (WASD, arrows). |
| `ring` | `ring = { sectors = 8, start_angle = 0.0, inner_radius = …, hysteresis = …, actions = [...] }` | The stick's angle picks one of `sectors` slices, and that slice's action is held. |
| `flick` | `flick = { full_turn_px = 8000.0, flick_threshold = 0.9, flick_time_ms = 100, rotate_smoothing_ms = 0, forward_deadzone = 0.0, vertical = "off", vertical_speed = 1200.0 }` | Flick stick for turning the camera. `vertical = "look"` adds up and down. |

**Zones** hold an extra action while the stick is within part of its travel:

```toml
[[profiles.left_stick.zones]]
min = 0.0        # 0.0–1.0 of travel
max = 0.6
action = { keys = ["KEY_LEFTSHIFT"] }
```

Triggers take zones too, under `[[profiles.left_trigger.zones]]`. The app hides zones for controllers whose triggers are on/off only.

## Triggers

```toml
[profiles.right_trigger.action.button]
threshold = 0.3                  # pull (0.0–1.0) that counts as pressed
action = { mouse = "Left" }      # any action a button can have
```

Or pass the trigger through as an analog axis: `[profiles.right_trigger.action] gamepad = "Right"` (`"Left"` or `"Right"`), or `"disabled"`.

## Gyro

Only meaningful for controllers with motion sensors. Written under `[profiles.gyro]`:

```toml
[profiles.gyro]
horizontal = "yaw"        # "yaw" (turn) | "roll" (tilt) | "yaw_and_roll"; pitch always drives vertical
invert_x = false
invert_y = false
noise_threshold = 1.0     # rotation slower than this (degrees/second) is scaled down

[profiles.gyro.mode.mouse]          # or: "off", stick = {...}, steering = {...}
sensitivity = 15.0                  # pixels per degree turned

[profiles.gyro.activation.unless_held]
button = "DpadUp"                   # or: "always", while_held, toggle
```

- `mode`: `"off"`, `mouse = { sensitivity }`, `stick = { stick, full_rate, anti_deadzone }` (rotation speed deflects a virtual stick), or `steering = { stick, max_angle }` (tilt steers a stick).
- `activation`: `"always"`, `while_held = { button = … }`, `unless_held = { button = … }` (a clutch: off while held), or `toggle = { button = … }`. The input can also be `left_trigger` or `right_trigger`, as in `while_held = "left_trigger"`.
- `recenter` (optional): an input that sets the current tilt as straight ahead.

## Requirements

A profile says what it cannot be played without, so the app can offer only profiles a player's controller supports:

```toml
requires = ["gyro"]
```

Only `gyro` exists today. Don't set `requires` for a profile that only *adds* gyro to a scheme that works without it: leave it out. The export dialog lists declared needs and warns about gyro use that isn't declared, but doesn't block.

## Menus

```toml
[[menus]]
name = "Belt"
cancel = "East"               # optional: button that backs out (default East)

[menus.kind.radial]
stick = "Right"

[[menus.items]]
label = "Belt 1"
action = { keys = ["KEY_1"] }

[menus.style]
position = "center"           # a 3×3 grid: top_left … bottom_right
scale = 1.0

[menus.style.background]
color = "#16181c"
opacity = 0.92
```

Menu kinds (`menus.kind`):

| Kind | TOML | Notes |
|---|---|---|
| Radial | `[menus.kind.radial] stick = "Right"` | Aim the stick at an item; release to pick it. Items are arcs of a circle, each sized by its `weight` (default 1). `boxes = true` puts them in boxes at equal angles instead. |
| Directional | `[menus.kind.directional] cluster = "DPad"` (or `"FaceButtons"`) | Four slots, up/right/down/left. |
| List | `kind = "list"` | D-pad or left stick moves; A picks. |
| Button | `kind = "buttons"` | A list whose items can also be picked with their own `button`. |
| Carousel | `[menus.kind.carousel] controls = "Bumpers"` | Cycles with `Bumpers`, `Triggers`, `DPad`, `LeftStick` or `RightStick`. |
| Grid | `[menus.kind.grid] columns = 4` | Up to 6 columns and 6 rows. |

A menu item's `action` can be `open_menu = "Name"`, which opens a submenu. Radial menus can't open submenus. `style` also has `items` and `selected` paints, as in the example above. `corners` (0–1, default 0.4) sets how round boxes, keys and panels are: 0 is square and 1 is a circle wherever the shape allows.

## Macros

```toml
[[macros]]
name = "Dodge"
steps = [
    { press = { keys = ["KEY_LEFTSHIFT"] } },
    { wait = 120 },
    { tap = { action = { keys = ["KEY_SPACE"] }, hold_ms = 50 } },
    { release = { keys = ["KEY_LEFTSHIFT"] } },
    { stick = { stick = "Left", x = 0.0, y = -1.0 } },
]
```

Steps: `tap` (press, hold `hold_ms`, release), `press` (keep held until a matching `release` or the macro's end), `release`, `wait` (milliseconds), and `stick` (move a virtual stick to `x`, `y` in −1..1, y positive down; it recentres when the macro ends). Macros are referenced from buttons with `macro = { name = "…" }`.

## Info overlays

```toml
[[info_overlays]]
name = "Controls"
title = "This game's controls"    # optional heading; may hold {tokens}
always = false                    # true: shown whenever the game is active
on_start = 6.0                    # optional: seconds shown when the game starts
linger = 2.0                      # optional: seconds it stays after release
rows = [
    ["{south}", "Jump", "{east}", "Use"],
    ["{lt} {rt}", "Scope · fire", "{lb}", "Belt (hold)"],
]

[info_overlays.style]
position = "center"
scale = 0.9
```

- Each row is a list of cells, and cells line up in columns. A cell is text with `{tokens}`.
- Button glyphs: `{south} {east} {west} {north} {lb} {rb} {lt} {rt} {select} {start} {guide} {ls} {rs} {l3} {r3} {dpad} {up} {down} {left} {right}`. They are drawn for the controller in use.
- Live values: `{time} {time12} {date} {profile} {layer} {app} {title} {pid} {cpu} {ram} {gpu} {wifi} {system_battery} {controller_battery}`, plus `{current_input}` (with the `current_input` settings).
- An overlay is shown by a `show_info` action, or always when `always = true`.

## Layers

A layer changes some of the profile's mappings while it is held or toggled on. Everything it doesn't set falls through to the profile.

```toml
[[layers]]
name = "Guide"
indicator = "bindings"           # "name" | "bindings" | "off" | { info = "Name" }
indicator_title = "Hold {guide} and press"
indicator_delay_ms = 250         # appears only if held this long
swallow_unbound = true           # buttons the layer doesn't set do nothing

[layers.buttons]
West = "toggle_overlay"
DpadUp = "next_profile"

[layers.buttons.DpadDown]
keys = ["KEY_TAB"]

[layers.right_stick.action.mouse]
speed = 1600.0
```

A layer can override `buttons`, `gestures`, `combos`, `disabled_combos`, `left_stick`, `right_stick`, `left_trigger`, `right_trigger` and `gyro`. Each override replaces the profile's setting for that input only.

**Guide** is special. In a pack, every profile's `Guide` button must hold the layer named `Guide`, and the pack must contain that layer: the library check fails otherwise. The app adds the default Guide layer when it creates a profile that holds it, but importing doesn't add it, so a pack should carry its own copy, as the Deus Ex pack does. The default layer holds the system shortcuts while Guide is down: `+ X` keyboard, `+ Y` numpad, `+ RT` / `+ LT` clicks, `+ Right stick` mouse, `+ D-pad` Enter, Tab and Escape, `+ D-pad Up` next profile, and so on. Its bindings are listed in [the Guide design notes](development/guide.md).

## Keyboard, numpad and font

```toml
overlay_font = "Rajdhani"

[keyboard_style]
position = "bottom_center"
scale = 1.0

[keyboard_style.background]
color = "#080c0d"
opacity = 0.94
```

`keyboard_style`, `numpad_style`, `media_style` and `menu_style` use the same shape as an overlay's `style`. `overlay_font` names a bundled font (`assets/fonts`) or an installed family.

## Versioning

- `format` is an integer. The app writes `9` and reads up to `9`. A pack with a newer `format` is refused with "update the app to import it". Older formats are upgraded when read.
- Format history:
  - 2 added layers.
  - 3 added toggles that start on.
  - 4 added keyboard and numpad styles.
  - 5 added the overlay font.
  - 6 added grid menus.
  - 7 added the Guide actions (`screenshot`, `toggle_recording`, `force_quit`), layer indicators with generated bindings, extra info overlays and a delay.
  - 8 moved controller needs from the pack to each profile (`requires`).
  - 9 added the media controls and in-game menu looks (`media_style`, `menu_style`).
- Bump `format` only for a breaking change to the shape. Adding an optional key with a default doesn't need it.

## Checks

The repository's test suite parses every file in `packs/` and checks it: the format, unique IDs and names, no dangling references (macros, menus, info overlays, layers, profiles), that each rule points at an existing profile, and that a plain pad can play at least one profile. A pack you add to `packs/` is checked the same way, and it is embedded in the next build.

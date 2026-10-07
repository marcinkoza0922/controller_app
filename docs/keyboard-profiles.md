# Keyboard profiles (remapping keyboard and mouse)

Status: all four stages implemented (2026-10-07), except menus, the on-screen keyboard and numpad on keyboard inputs (a controller drives them), layers that act across devices (a layer is on for the device that turned it on), and the per-key stick amount (a walk modifier): the pad template uses Shift as L3 instead. Stage 4 added gestures, combos, toggle, turbo, macros, next profile, layers and overlays for keyboard inputs. Stages 1–3: profile kinds, grabbing, key and mouse-button remaps, the panic chord, gamepad outputs, mouse movement as a stick or scaled pointer, wheel and movement-direction inputs, and the ESDF layout and keyboard-to-pad templates. Stage 4 is not, and neither is the per-key stick amount (a walk modifier): the pad template uses Shift as L3 instead.

## Goals

1. **Play controller-only games with a keyboard and mouse.** Keys and mouse motion drive a virtual
   Xbox-style pad.
2. **Rebind keys without touching game settings.** For example ESDF layout, applied system-wide or
   per game through the existing auto-switch rules.

## Decisions at a glance

| Topic | Decision |
|---|---|
| Profile type | Every profile is `Gamepad` (default) or `Keyboard`. Chosen when the profile is created or duplicated, fixed afterwards. |
| Where they live | In games like any profile. A game can hold both kinds. |
| Inputs | Keyboard keys, mouse buttons, wheel and mouse motion. All attached keyboards and mice count as one combined keyboard and mouse. |
| Outputs | Keys, mouse buttons, gamepad buttons and sticks, and every other action (macros, menus, layers, overlays, next profile). |
| Parity | Anything a gamepad input can be mapped to, a keyboard or mouse input can be too: Toggle, Turbo, combos, gestures, zones, layers. |
| Active profile | One at a time, of either kind. A keyboard profile leaves physical pads as pass-through; a gamepad profile leaves keyboards and mice alone. |
| Grab | Keyboards and mice are grabbed only while a keyboard profile is active. A gamepad profile leaves them untouched. |
| Unmapped keys | Per-profile setting: pass through (default) or block. |
| Reserved keys | Never remappable, always passed through: both Super/Meta keys, Ctrl+Alt+Fn VT switches. |
| Panic chord | Chord set in Settings, never blank (default Ctrl+Alt+Shift+Esc) that releases the grab and disables remapping. |
| Switching | Auto-switch rules (either kind), and a "Next profile" action on any key. Next profile cycles profiles of the same type only. |
| Physical gamepads | Unaffected by a keyboard profile; they keep running their gamepad profile. |
| Templates | ESDF layout, and Keyboard → Xbox pad. |

## Profile type

- `Profile` gets a `kind` field: `gamepad` (default when absent) or `keyboard`. Existing configs and
  packs load unchanged as gamepad profiles, so no migration is needed beyond the default.
- The type picker appears where profiles are created: New, New from template, Duplicate (which keeps
  the source's type). The editor shows only the inputs that fit the type. Changing the type later is
  not offered; duplicate and rebuild instead.
- Profile pickers and the sidebar mark keyboard profiles (a small keyboard icon) so the two kinds are
  easy to tell apart.
- A game's Profiles tab lists both kinds. Name uniqueness stays per game across kinds.

## Active profile per device class

One profile is active at a time, of either kind (an early design had a slot per kind; it
confused which one a toast or rule meant, so it was dropped).

- While a keyboard profile is active it also runs on physical pads, which pass through unchanged
  (its pad mappings are the pass-through ones). Keyboards and mice are grabbed only then.
- An auto-switch rule that points at a profile activates it. When the game loses focus, the
  default profile (Settings) comes back.
- `controller_app profile <name>` activates the named profile, whatever its kind.
- "Next profile" cycles the profiles of the active game that share the active one's kind.
- Layers, macros, menus and info overlays belong to the game as before. Layers have no kind: a layer
  overrides whatever inputs it names, for either kind, and any input can turn one on. The layer editor
  is shown over a profile of either kind.

## Inputs

All keyboard and mouse inputs are normalised into the same event type the engine already uses for
pad buttons and axes.

- **Keys**: every evdev `KEY_*` code on a keyboard, press and release. Modifier keys are ordinary
  inputs and can be combo members, so Shift+W can be mapped separately from W.
- **Mouse buttons**: left, right, middle, side, extra, and any other `BTN_*` mouse button.
- **Wheel**: up and down (and horizontal), each notch a press, like the pad's scroll outputs in
  reverse.
- **Mouse motion**: relative X/Y. It has two uses:
  - as a **stick source**, driving a virtual pad stick (see below);
  - as **four direction buttons** (Mouse Up/Down/Left/Right) that press while the pointer moves
    that way past a threshold, so a flick can fire an action or a combo.

Everything the profile editor supports for a pad button (action, several actions at once, gestures,
combos, Toggle, Turbo, layers) is available per key and per mouse button.

### Mouse motion as a stick

Mouse motion is a relative device, so it needs conversion into a stick deflection:

- **Sensitivity** (counts per full deflection), **invert X/Y**, a **curve**, and a **deadzone**,
  shaped like the stick settings that already exist.
- Deflection **decays back to centre** when the mouse stops, over an adjustable time (default
  short), so a stopped mouse doesn't hold the stick over.
- Targets: left stick, right stick (default), or no stick.
- **Mouse → mouse scaling** is the other mode: the pointer keeps moving as a mouse, with a
  sensitivity multiplier and X/Y invert. This is the default for a new keyboard profile so it behaves
  as a normal mouse until the user changes it.

## Outputs

Everything a gamepad profile can output, including:

- Keys and key combos, mouse buttons, wheel, mouse movement.
- Gamepad buttons, triggers and stick directions on the virtual pad. A key can push the stick
  (WASD → left stick), two directions make a diagonal, and an optional per-key **stick amount** (0–100%)
  allows a walk/run toggle.
- Macros, menus, info overlays, layers, on-screen keyboard and numpad, Next profile.

A keyboard profile that only remaps keys to keys needs no virtual pad. The virtual pad is created
only when the pad is already part of the daemon (it is today), so no change there.

## Grabbing and safety

The keyboard is the user's main way to recover from a bug, so this section is deliberately strict.

- Devices are grabbed (`EVIOCGRAB`) only while the active profile is a keyboard one. Switching to a gamepad
  profile releases them. Hotplug adds and removes devices while
  active.
- Output goes through the existing virtual keyboard and mouse. The daemon never grabs its own
  devices (the `VIRTUAL_PREFIX` rule stays).
- **Reserved keys** always pass through, even in a profile that blocks unmapped keys: Left and Right
  Super/Meta, and the Ctrl+Alt+F1–F12 VT switches. The editor shows them greyed and refuses to
  map them or use them in combos. A pass-through key is forwarded as-is, in order with the
  remapped ones, with no Toggle, Turbo, gesture or combo wait.
- **Panic chord**: Ctrl+Alt+Shift+Esc by default, changeable in Settings but never blank. It releases every grabbed
  device and puts remapping in a *disabled* state (the same as the remapping switch in the
  header), shows a toast, and releases all keys the daemon is holding. Enabling remapping again
  from the GUI or `controller_app enable` brings it back.
- All held output is released when the profile changes or the daemon stops, as for pad profiles.
  If the daemon dies, the kernel drops the grab when the file descriptor closes, so a crash can't
  leave the keyboard locked.
- Latency: remapped keys should add no perceptible delay. Keys that are neither combo members nor
  gesture inputs are forwarded at once; only inputs that need a decision window wait for it, as on
  the pad today.

## Unmapped keys

Each keyboard profile has the setting **Other keys**: *Pass through* (default) or *Block*.

- Pass through: any key without a mapping is forwarded unchanged. A ESDF layout profile is just
  four rows. Mouse movement, wheel and unmapped buttons are forwarded the same way.
- Block: unmapped keys are swallowed, for controller-only games where stray keys are unwanted.
  Reserved keys still pass through.

A key that has a mapping, including "Disabled", is never also forwarded. "Disabled" is how to
block a single key in a pass-through profile.

## Devices

All attached keyboards and mice are treated as one. There is no device picker. Devices that are
virtual (ours, or others like remote-desktop or Steam Input devices) are skipped, like
pads today. A device counts as a keyboard if it reports letter keys, and as a mouse if it reports
relative X/Y and a mouse button; one device can be both. Gamepads that also report keys are still
handled as pads.

## GUI

- **Profile creation**: a Type choice (Gamepad / Keyboard) in New and in New from template;
  Gamepad preselected.
- **Keyboard profile editor** sub-tabs: **Keys** (on-screen keyboard from `src/keyboard.rs`,
  click a key to edit it; list of remapped keys as one-line summaries such as "W ▸ E"),
  **Mouse** (buttons, wheel, and motion mode with its settings), **Combos**, and a **General**
  section with the Other keys setting.
- "Find by pressing" works for keyboard profiles by pressing a key or clicking a mouse button.
  (The pointer is not grabbed while the editor itself is focused, so the GUI stays usable.)
- Reserved keys are shown greyed with a tooltip saying why.
- The live controller picture on Overview/Profiles is replaced by the keyboard picture, with
  remapped keys highlighted and labelled with their action.
- Validation follows the existing checks: unknown keys, missing macros, incomplete combos,
  mapping a reserved key, and two rows for the same key all block saving.
- The Settings page gets the panic chord.

## Templates

Offered under "New from template…" with the Keyboard type:

- **ESDF layout**: for playing a WASD game with the hand on ESDF. The three letter rows (top, home
  and bottom) shift one key to the right: E sends W, S sends A, D sends S, F sends D, G sends F,
  and so on. The key at the left end of each row (Q, A, Z) sends the row's last key (backslash,
  apostrophe, slash), so no key is lost. Everything else passes through.
- **Keyboard → Xbox pad**: W/A/S/D → left stick (Shift = half deflection for walking), mouse motion → right stick,
  Left/Right click → RT/LT, Space → A, E → X, R → Y, Q/F → LB/RB, Ctrl → B,
  Tab/Esc → Select/Start, 1–4 → D-pad, other keys blocked.

## Packs

- A pack's profile carries its `kind`; older packs have none, so they import as gamepad profiles.
- The export dialog warns when a keyboard profile outputs to a gamepad (the importer needs a virtual
  pad, which the daemon always creates, so this is informational) and when it uses a reserved key
  (impossible to save, so never shown).
- A pack made by an app version that doesn't know `kind` would read keyboard profiles as gamepad
  profiles, which is wrong. The pack format version is bumped when a keyboard profile is present,
  so older apps refuse it with the existing "update the app" message.

## Implementation stages

1. **Model and editor shell.** `kind` on `Profile` with the serde default; type picker; one active
   profile in the daemon state and IPC (`status`, `profile`). Keyboard profiles
   are empty and do nothing yet.
2. **Capture and key/button remaps.** Keyboard and mouse discovery and grab/release (`input.rs`,
   `daemon.rs`), reserved keys, panic chord, Other keys setting, key → key and key → mouse-button
   remaps through the engine. This stage alone covers ESDF layout; ship that template here.
3. **Pad outputs and mouse motion.** Keys → pad buttons and stick directions, mouse motion → stick
   and mouse → mouse scaling, wheel and mouse direction inputs. Ship the Keyboard → Xbox pad template.
4. **Action parity.** Gestures, combos, Toggle, Turbo, zones where they apply, layers, macros,
   menus, overlays and Next profile on keyboard inputs. Then packs, README and the CLI help.

## Resolved questions

1. **Layers are kind-less.** A layer overrides whatever inputs it names, in either slot. Mixing
   pad and keyboard inputs in one layer is possible and left to the user; it also allows setups that
   use both a gamepad and KB+M.
2. **Zones**: only for mouse motion used as a stick. Keys have none.
3. **Panic chord** is a Settings option. It can be changed but not left blank. Default Ctrl+Alt+Shift+Esc.
4. **Focus-aware grab** is not included for now.
5. **Keyboard layouts**: labels are by QWERTY position; no layout-aware labels.

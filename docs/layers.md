# Layers (mode shifts)

Status: implemented (2026-10-06), except per-layer timings (see the open questions).

## Goal

While an input is held, some of the controller's mappings change, then change back when it's
let go. For example: hold LB, and the face buttons type F1–F4 and the right stick scrolls
instead of moving the mouse. This is like Shift on a keyboard, or Steam Input's action layers.

## Decisions at a glance

| Topic | Decision |
|---|---|
| What a layer is | A named set of overrides belonging to a game, like its macros and menus. It applies on top of whichever of the game's profiles is active; inputs it doesn't set keep that profile's mapping. |
| What it can override | Buttons and stick directions, gestures, combos, sticks, triggers (with zones), gyro. |
| Activation | While held, through a new **"Layer…"** action. Wrapped in **Toggle**, it stays on until toggled off, which also lets menu items switch layers. |
| What can hold a layer | Any input that can hold an action: buttons, stick directions, trigger pulls, zones, gestures, combos. |
| The holding button's own action | No special tap-or-hold behavior. Give it a gesture (e.g. tap for its own action, long press for the layer) if it should do both. |
| Several layers at once | They stack. The most recently held one wins where they disagree. |
| Nesting | A layer's own bindings can hold further layers. |
| Inputs held when a layer changes | Keep doing what they started until released. The layer applies from their next press. |
| Sticks, triggers, gyro | Their modes switch immediately when a layer starts or ends. |
| Editor | A **Layers** tab on the game's page, using the profile editor's tabs. Inputs a layer doesn't set read "Same as Gameplay: …" (compared with a chosen profile), with an **Override** button. |
| Shared items | Can't refer to layers. They already can't refer to any game's own macros, menus or info overlays; layers are always a game's own. |
| Combos in a layer | Added to the base's combos. A layer combo on the same buttons replaces the base one, and a layer can switch a base combo off. |
| Feedback | Each layer shows an on-screen **indicator** while active, which can be switched off per layer. Also a `{layer}` token for info overlays, listing all active layers. |
| Timings | The base profile's for now. Per-layer timings get decided after trying layers out. |

## Data model

A game gains a list of layers, next to its macros, menus and info overlays. A layer holds only
what it overrides; everything else falls through to the layers below it and finally to the
active profile (the **base**). Since every profile of the game can use every layer, one layer
works the same way over each of them, e.g. "hold Y to lean" in both a gameplay and a vehicle
profile.

```text
Game
├── profiles, macros, menus, info overlays, rules (as today)
└── layers: [Layer]

Layer
├── name                 unique within the game
├── buttons              Button → ButtonAction, only the overridden ones
├── gestures             Button → Gestures, only the overridden buttons
├── combos               added to (or replacing, same buttons) the base's combos
├── disabled_combos      base combos switched off while held, by their buttons (so the same
│                        setting works over every profile that has such a combo)
├── left_stick?, right_stick?          whole StickConfig, if overridden
├── left_trigger?, right_trigger?      whole TriggerConfig (action and zones), if overridden
├── gyro?                whole GyroConfig, if overridden
└── indicator            what's on screen while it's active (see "Indicator")
```

Sticks, triggers and gyro are overridden as a whole: a layer either keeps the base's right
stick entirely, or sets its own complete right-stick settings. That keeps "what does this
stick do now" answerable at a glance.

The new action is `ButtonAction::Layer(name)`, like `Macro` and `OpenMenu`. It refers to a layer
of the same game.

```toml
[[games]]
name = "Skyrim"

[[games.profiles]]
name = "Gameplay"
# … as today …

[games.profiles.buttons.LeftBumper]
layer = "Hotkeys"

[[games.layers]]
name = "Hotkeys"
indicator = "name"          # or "off", or { info = "F-keys" }

[games.layers.buttons.South]
keys = ["KEY_F1"]

[games.layers.right_stick]
deadzone = 0.15
[games.layers.right_stick.action.scroll]
speed = 15.0
```

## How it behaves

### Engaging and stacking

- An input mapped to "Layer X" starts the layer when it presses, and ends it when it releases.
  This uses the same press and release the engine already tracks for other held actions, so
  trigger pulls, zones, combos and gestures (e.g. a long press) all work.
- **Toggle → Layer X** turns the layer on with one press and off with the next, like any
  toggled action. A toggled layer joins the stack when switched on, and leaves it when
  switched off.
- Held layers form a stack, in the order they were engaged. To resolve an input, the engine
  looks through the stack from newest to oldest, then at the base, and uses the first setting
  it finds.
- Letting go of a layer's input removes that layer from the stack wherever it is. Layers above
  it stay.
- If two inputs hold the same layer (or one holds it and a toggle has it on), it stays until
  all of them let go.
- A layer that holds itself is ignored. So is a "Layer…" that names a missing layer, which the
  editor flags.

### Inputs already held

- A button, stick direction, trigger pull or zone that is down when the stack changes keeps the
  action it started with, until it's released. Its next press uses the new stack. There are no
  stray key releases and no surprise presses.
- This includes the input holding the layer: if the layer gives that button a different
  action, the button still releases the layer when let go. The layer's action only applies
  from its next press.
- Running macros, toggles and turbo continue unaffected.

### Analog inputs

- Stick modes (gamepad stick, mouse, scroll, direction keys), trigger modes and gyro settings
  switch immediately when the stack changes. For example, the cursor stops and scrolling
  starts.
- Direction keys from a stick in direction-keys mode count as held: a key that's down stays
  down until that direction is released.
- Zones count as held as well: an active zone keeps its action until the stick or trigger leaves
  it.

### Combos and gestures

- While the stack changes, the active combos are the base's, minus any switched off by a held
  layer, plus each held layer's combos. A combo on the same buttons is taken from the newest
  layer that defines it.
- Gestures follow the newest layer that overrides that button's gestures, else the base's.
  The base profile's timings (tap window, long press, combo window) apply throughout; layers
  don't have their own.
- A button whose layer has gestures but whose base has none gains them only while the layer is
  held. The existing timing rules (wait out the tap window) apply only then.

### Everything else

- Switching profiles releases held inputs, as it does today, so held layers end. Toggled
  layers stay on when the new profile is in the same game, and apply over it. Switching to
  another game drops all layers.
- Opening the on-screen keyboard, numpad or a menu releases mappings as it does today, which
  drops held layers. **Toggled layers stay on**: a menu item can toggle a layer, and the menu
  closing must not undo that.
- Menu items can't hold a layer, since a menu item is a tap, but they can toggle one: in a
  menu item, "Layer…" is offered only inside Toggle. Choosing the item switches the layer on
  or off, and the menu stays up for more picks as usual.
- Macros can't hold or toggle layers ("Layer…" isn't offered as a macro step).
- `{layer}` (new info overlay token) shows the active layers, oldest first, joined with " + "
  (e.g. "Hotkeys + Build"). It shows "—" when none are active, and redraws as layers change.

### Indicator

Every layer shows something on screen while it's active, so it's clear the controller is
doing something different. It can be switched off for layers that are quick and obvious, for
example "hold Y, and the sticks lean" in a shooter. Per layer, the indicator is one of:

- **Its name** (the default): a small label with the layer's name, drawn like an info
  overlay (default: top center, small, see-through), with its own position, size and colors
  under Appearance.
- **An info overlay** of the game (or a shared one): shown only while the layer is active.
  This suits a cheat sheet such as "F1 Map · F2 Journal · F3 Skills · F4 Craft" for an F-key
  layer.
- **Off.**

Indicators of several active layers stack at their position, like info overlays do. They are
drawn by the overlay window that already shows info overlays and menus.

## Editor

- **Layers tab.** The game's page gets a **Layers** tab, between Profiles and Macros. It has a
  layer picker with **+ New layer**, rename and delete, and a **Compare with** picker
  choosing which of the game's profiles shows through as the base (default: the profile last
  edited on the Profiles tab).
- **Editing a layer.** It uses the profile editor's Buttons, Sticks & triggers, Combos and Gyro
  tabs:
  - An input the layer doesn't override is muted and reads "Same as Gameplay: Space", with an
    **Override** button that copies that profile's setting into the layer to edit.
  - An overridden input has **Back to base**, which removes the override.
  - For sticks, triggers and gyro, overriding copies the whole stick, trigger (with its zones)
    or gyro settings.
  - On the Combos tab, the compared profile's combos appear with **Switch off in this layer**,
    and **+ Add combo** adds a layer combo.
- **The drawing.** The controller drawing labels show what the edited layer does: the layer's
  overrides, and the compared profile elsewhere.
- **Live drawing.** The Overview's live drawing (and the Profiles tab's, for the active profile)
  labels the inputs as they act right now: with layers held or toggled on, the active layers'
  overrides (newest first) over the active profile. The daemon reports the active layers in
  its status, and the caption names them ("Layers: Hotkeys + Build").
- **Copying.** "Copy from another game…" offers layers too, for games that share a control
  scheme, such as sequels. A copied layer brings the macros, menus and info overlays it uses,
  as other copies do.
- **The action.** "Layer…" appears in the action list everywhere a held action makes sense in a
  game's profiles and layers, and inside "Toggle…" in a game's menu items. It offers the game's
  layers, plus **+ New layer** to make one in place. It isn't offered in shared items.
- **Indicator.** Each layer has an "Indicator" setting next to its name: Name (with Appearance
  and a preview), an info overlay of the game, or Off.
- **Renames and deletes.** Renaming a layer updates every "Layer…" action that names it.
  Deleting one leaves those actions flagged as missing, as with macros.

## Validation

- Layer names are non-empty and unique within their game.
- "Layer…" must name a layer of the same game. In a menu item, it must be inside Toggle.
- Shared macros, menus and info overlays can't use "Layer…" at all.
- An indicator set to an info overlay must name one the game can use.
- A layer's overrides get the same checks as the base: valid keys, existing macros, menus and
  info overlays, combos of at least two buttons, and zone ranges.
- Packs: layers are part of the game, so export and import carry them like its macros. Layers are
  never shared, so they can't clash with shared items on import. Gyro in a layer counts toward
  the `gyro` requirement.

## Open questions

- **Per-layer timings.** Should a layer be able to set its own tap window or long-press time?
  Decide after trying layers out.

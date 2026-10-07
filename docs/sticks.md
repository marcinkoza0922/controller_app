# Stick behaviors: mouse response, rings and flick stick

Status: implemented (2026-10-07), except the ring's radial preview and a guided calibration (see the open questions).

## Goal

Give sticks more ways to drive a mouse or buttons, in the style of Steam Input and
JoyShockMapper:

1. **Mouse response**: acceleration, vertical scale, outer-edge boost and smoothing, on top of
   today's deadzone and power curve.
2. **Ring**: the stick's angle picks one of N sectors, and each sector holds an action while the
   stick points into it (button ring, stick-to-ring).
3. **Flick stick**: pushing the stick turns the camera to that direction at once; rotating the
   stick then turns it by the same angle.

## Where things are today

- `StickAction` (`src/config.rs`) has `Disabled`, `Gamepad`, `Mouse { speed }`,
  `Scroll { speed }` and `Keys`.
- `StickConfig` has a radial `deadzone`, one power-law `curve` (mouse and scroll only),
  `key_threshold` and `zones`.
- `Engine::tick` (`src/engine.rs`) turns the stick position into mouse and scroll output with a
  sub-pixel accumulator (`take_whole`). `Engine::stick` handles the gamepad and keys modes and
  zones. `needs_tick` decides whether the daemon's timer has anything to do.
- Layers already replace a stick's whole `StickConfig`, so any new mode works in a layer
  without extra code.
- `engine.rs` is far over the long-file limit. The new stick code goes in its own module and
  the existing stick code moves with it (phase 0).

## Decisions at a glance

| Topic | Decision |
|---|---|
| Config shape | New fields and variants only. Every new field is `#[serde(default)]` with a value that behaves like today, so existing configs and packs load unchanged. |
| Mouse extras | Optional fields on `StickAction::Mouse`, not a new variant. |
| Ring and Flick | New `StickAction` variants. |
| Ring sectors | 4, 8 or 12 equal sectors with an adjustable start angle. No free-form sectors for now. |
| Ring actions | Each sector holds a `ButtonAction`, through the same path as zones, so toggle, turbo, macros, menus and layers all work. |
| Flick output | Spread over a short time (default 100 ms) rather than sent as one jump, which games often drop. |
| Flick calibration | A single `full_turn_px` setting: how many mouse pixels make a 360° turn in the game. |
| Acceleration UI | One 0–100% slider; ramp time and boost behind an "Advanced" fold. |
| Packs | No export warning: these need nothing beyond a plain XInput pad. |
| Flick sticks | At most one stick per profile (and per layer) is a flick stick; Save & apply refuses two. Revisit only with a compelling case. |
| Flick vertical | Optional, under Advanced, from the first version (see Flick stick). |
| Gyro | Unchanged and independent. Flick on the right stick plus gyro for fine aim is the intended pairing. |

## Data model

```toml
# Mouse mode, with the new optional fields (shown with their defaults)
[right_stick]
deadzone = 0.1
curve = 2.0
[right_stick.action.mouse]
speed = 1600.0
accel = 0.0            # 0 = off. 0..1: how much the speed grows while held at full deflection
accel_ramp_ms = 400    # time to reach full acceleration
y_scale = 1.0          # vertical speed relative to horizontal
outer_boost = 0.0      # extra speed multiplier in the outer 10% of travel
smoothing_ms = 0       # low-pass time constant on the stick position; 0 = off

# Ring
[right_stick.action.ring]
sectors = 8            # 4, 8 or 12
start_angle = 0.0      # degrees clockwise from up for the middle of sector 1
inner_radius = 0.5     # deflection (after the deadzone) at which a sector engages
hysteresis = 0.1       # fraction of a sector the stick must overshoot to change sector
actions = [ ... ]      # one ButtonAction per sector, in clockwise order

# Flick stick
[right_stick.action.flick]
full_turn_px = 8000.0     # mouse pixels for a 360° turn in the game
flick_threshold = 0.9     # deflection that starts a flick
flick_time_ms = 100       # the initial flick is spread over this long
rotate_smoothing_ms = 0   # smoothing of the rotation after the flick
forward_deadzone = 0.0    # degrees around "up" where a flick turns nothing
vertical = "off"          # Advanced: off | look (stick up/down also moves the mouse vertically)
vertical_speed = 1200.0   # px/s at full deflection, used when vertical = "look"
```

In Rust:

```rust
Mouse { speed: f32, #[serde(flatten)] response: MouseResponse }  // accel, accel_ramp_ms, y_scale, outer_boost, smoothing_ms
Ring { sectors: u8, start_angle: f32, inner_radius: f32, hysteresis: f32, actions: Vec<ButtonAction> }
Flick { full_turn_px: f32, flick_threshold: f32, flick_time_ms: u32, rotate_smoothing_ms: u32, forward_deadzone: f32, vertical: FlickVertical, vertical_speed: f32 }
```

`Mouse { speed }` in older configs still parses because the new fields default. A `Ring` with fewer
`actions` than `sectors` leaves the missing sectors doing nothing; the editor keeps the two in step.

## How it behaves

### Mouse response

Applied in `tick`, in this order, to the stick position after the deadzone:

1. **Smoothing**: an exponential moving average of the position with the given time constant.
2. **Curve**: today's power curve on the magnitude.
3. **Outer boost**: above 90% of travel, the magnitude is scaled up by `1 + outer_boost`,
   blended in linearly so there is no step.
4. **Acceleration**: a multiplier `1 + accel × ramp` where `ramp` rises from 0 to 1 over
   `accel_ramp_ms` while the deflection is above 90%, and falls back to 0 as soon as it drops
   below (instantly on release). Partial pushes never accelerate, so fine aim stays steady.
5. **Scale**: `x × speed`, `y × speed × y_scale`, into the existing accumulator.

Scroll mode ignores the new fields.

### Ring

- Below `inner_radius` (after the deadzone) no sector is active.
- At or above it, the stick angle picks a sector. A different sector is chosen only when the
  stick has moved `hysteresis` of a sector width past the boundary, so jitter on an edge doesn't
  flicker.
- Changing sector releases the old sector's action and presses the new one. Going back inside
  `inner_radius` releases it.
- Sector actions are held through the same `digital` path that zones use, under a new
  `Source::RingSector(stick, i)`, so they follow the usual rules: gestures, toggle and turbo
  wrappers, `Menu…` and `Layer…` actions, and release when the profile or layer changes.
- Ring is not also a gamepad stick. A profile that wants both uses zones, or a layer.

### Flick stick

State per stick: whether a flick is in progress, the last angle, and the part of the current
flick not yet sent.

- **Start**: when the deflection crosses `flick_threshold` (rising), the stick's angle from
  "up" is the flick angle. Its mouse equivalent is `angle / 360 × full_turn_px`. Angles inside
  `forward_deadzone` of up turn nothing.
- **Spread**: the flick amount is added to a pending pool and drained over `flick_time_ms` in
  `tick`, with a smoothstep ease so it starts and stops softly. The existing sub-pixel
  accumulator carries the remainders.
- **Rotate**: while the stick stays out, each tick turns the camera by the change in stick angle
  since the last tick, using the shortest way round, optionally smoothed. Tiny changes under a
  small noise floor are ignored.
- **Release**: dropping below the threshold ends the flick. Nothing snaps back.
- By default only the horizontal axis is driven; vertical movement stays with gyro or another
  stick, which is how flick stick is normally used. With the Advanced option `vertical = look`,
  the stick's up/down deflection also moves the mouse vertically at `vertical_speed`, using
  the stick's curve.
- The stick's own deadzone applies first. Below `flick_threshold` the stick does nothing.

### Layers and profile changes

- Switching a stick's mode (by layer or profile) cancels any pending flick, resets ring state
  and releases held ring actions, as a stick leaving `Keys` mode does today.
- `needs_tick` is true while a flick has a pending amount or the stick is rotating, while a
  mouse stick is deflected or still smoothing out, and while an accelerated mouse ramp is
  non-zero.

## Editor

- The stick kind picker in `src/gui/profile.rs` gains **Ring** and **Flick stick**.
- **Mouse** keeps its speed and curve controls. A new "Response" section holds the acceleration
  slider, with ramp time and boost under **Advanced**, plus vertical scale and smoothing.
- **Ring** ("Button ring" in the picker): a sector count picker, the first sector's angle, the
  inner radius and hysteresis, and one action editor per sector, labelled with its direction.
  The radial preview that highlights the live sector is not built yet.
- **Flick stick**: the settings above, with a short note on finding `full_turn_px` (see
  below).
- One-line summaries ("mouse, 1600 px/s, accel 40%", "ring, 8 sectors", "flick, 8000 px/turn")
  go in the summary function next to `StickAction::Mouse`.
- The controller picture labels a ring stick with its sector actions around the stick, and a
  flick stick as "Flick".

### Calibrating `full_turn_px`

A **Test turn** button on the Flick settings asks the daemon to wait 3 seconds, then move the
mouse `full_turn_px` pixels to the right over a second, on a virtual mouse of its own. The user
switches to the game and watches. If the game turned N degrees, the size to use is the current
one × 360 / N, and the editor says so beside the button. The value is also plain-editable.

## Validation

`src/gui/checks.rs` flags, on the stick row and the profile tab (and blocks Save & apply
like other errors):

- Both sticks set to Flick in the same profile or layer (also marked on the Sticks tab).
- A ring sector action that names a missing macro, menu, layer or info overlay.
- A ring sector action with an unknown key.
- `sectors` not in {4, 8, 12}, `full_turn_px` not positive, a `flick_threshold` not above the
  deadzone.

## Tests

Engine tests, driven by synthetic axis events as the existing stick tests are:

- **Mouse**: acceleration ramps only at full deflection, resets on release, and `y_scale`
  scales only the vertical axis. Old config values give the same output as before.
- **Ring**: each sector holds its action at its centre angle, boundaries switch with
  hysteresis, going inside the inner radius releases, and a toggle-wrapped sector action
  stays held.
- **Flick**: `vertical = look` moves the mouse vertically with up/down; a 90° push produces about a quarter of `full_turn_px` over `flick_time_ms`;
  rotating by 45° while held produces an eighth; releasing and re-pushing flicks again;
  `forward_deadzone` suppresses a straight-up flick.
- **Layers**: a layer that switches a stick to Flick or Ring, and back, releases and cancels
  cleanly.
- **Config**: old stick configs parse and re-serialize without the new fields; new ones round
  trip.
- **Checks**: the validation cases above.

## Phases

0. **Extract**: move the stick code out of `engine.rs` into a `src/engine/stick.rs` module (no
   behavior change). The rest builds on this.
1. **Mouse response**: the new optional fields, engine, editor section, tests.
2. **Ring**: the variant, engine, radial editor, validation, tests.
3. **Flick stick**: the variant, engine, editor, calibration helper, tests.
4. **Docs and library**: update the README's stick section, and adopt the new modes in
   `packs/` where they help (e.g. a shooter pack that uses flick stick plus gyro).

## Open questions

- **Acceleration shape**: a linear ramp over `accel_ramp_ms` is the simplest. A curve (ease-in)
  might feel better; decide after trying it.
- **Ring and diagonals**: an 8-sector ring makes diagonals easy to hit; whether 4 sectors
  should offer a rotated (X-shaped) layout is left to `start_angle`.
- **Per-axis curves for mouse mode**: not planned; `y_scale` covers the common need.
- **Calibration without a game**: measuring the game's turn size reliably from the app alone
  isn't possible, so the helper stays manual or semi-manual.

## What was built differently

- The ring's radial preview with a live highlight is not built; the editor lists the sectors.
- Calibration is the **Test turn** button above, not a guided measurement.
- Vertical look follows the stick's up/down deflection, so with it on, holding the stick up or
  down also keeps looking up or down. It is off by default and under *Advanced flick*.

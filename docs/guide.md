# Guide button as a shift key

Status: steps 1–2 implemented (2026-10-07): the `Shift` action, the swallow option, the default
Guide layer without Screenshot, ForceQuit and ToggleRecording (L3, RB and B are unbound in it
for now), and the migration. Step 3 (`Screenshot`, in `src/capture.rs`) and `ForceQuit` (`src/quit.rs`, B held 2 s) are done too; `ToggleRecording` (`src/record.rs`, L3) is written but untried against a real gpu-screen-recorder; the packs are still to do.

## Goal

A plain Guide press shouldn't switch profiles. Steam uses Guide for itself, so Guide maps to
Guide by default and Steam is assumed to be installed. Guide is also used as a shift key,
like the Steam Deck: hold it and other inputs become system shortcuts (keyboard, numpad,
screenshot, mouse clicks and so on).

## Decisions at a glance

| Topic | Decision |
|---|---|
| Mechanism | A **Guide layer** (see `layers.md`), opened by a new `Shift` action on the Guide button. |
| Plain Guide | `Shift { layer, tap }`: the layer starts on press. If Guide is released without any input the layer binds having been used, a Guide tap (`BTN_MODE`) goes to the virtual pad. Steam therefore sees Guide on release, and never sees its own hold or long press. |
| Unbound inputs while held | Swallowed (a layer option, on for the Guide layer): Guide+A does nothing rather than pressing A in the game. |
| Where it lives | A copy of the layer in General and in every game and pack, all built by one `guide_layer()` function. Editable per game. |
| Profile switching | Moves to Guide + D-pad Up (Next profile). |
| New actions | `Screenshot`, `ToggleRecording`, `ForceQuit`. Built-in and named, never a generic "run command", because packs are shared files. |
| Recording tool | gpu-screen-recorder. |
| Force quit | Held for about 2 s inside the action itself (not the profile's long-press time), with a toast and rumble while it counts. |

## Default Guide layer

| Input | Action |
|---|---|
| Guide (alone, released unused) | Guide tap to Steam |
| + X | Open the on-screen keyboard |
| + Y | Open the on-screen numpad |
| + RB | Screenshot to `~/Pictures/Screenshots/<game>/` |
| + L3 | Start / stop video recording |
| + RT | Left click |
| + LT | Right click |
| + Right stick | Move the mouse |
| + B (held ~2 s) | Force quit the focused window |
| + D-pad Right / Down / Left | Enter / Tab / Escape |
| + D-pad Up | Next profile |

## New actions

- **`Shift { layer, tap }`.** Engine change: starts the layer on press like `Layer`, and on
  release emits `tap` only if no input bound by the layer was used meanwhile. "Used" means a
  button, stick direction or trigger pull the layer overrides (or, for the swallow option, any
  input at all). The press and release of the holding button follow the layer rules for
  inputs held across a stack change.
- **`Screenshot`.** The daemon picks the tool by compositor (kwin: spectacle, sway and
  hyprland: grim), names the file with a timestamp, and saves under
  `~/Pictures/Screenshots/<active game's name>/` (General's name when no game is active).
  A toast confirms it.
- **`ToggleRecording`.** The first press spawns gpu-screen-recorder writing to
  `~/Videos/Recordings/<game>/`; the next stops it with SIGINT. The daemon tracks the child
  so a second start can't happen, and stops it on exit. A toast and the overlay show that
  recording is on (today only toasts say so; there is no lasting indicator). If the program that was in front when it started (a game, or any non-desktop window; the same target as ForceQuit) exits, the recording is ended and saved. If the tool isn't installed, the action toasts that instead of failing
  silently.
- **`ForceQuit`.** Acts on the focused window's process: a game with a rule goes by the game's
  name, any other window by its title. It never touches the desktop (shell, compositor, display
  server, session services; see `quit::protected`), a window with no process, or this app.
  Sends SIGTERM to the process tree, then SIGKILL after 3 s. Letting go before the 2 s hold
  completes cancels it. Without focus tracking it can only end the active game's process.

All three appear in the action picker with summaries, and pack validation knows them.
`requires` on packs lists gpu-screen-recorder or the screenshot tool where they are used.

## Info overlays

Holding Guide shows what it does, and games add to it.

- **Layer indicators.** A layer's indicator can now be `bindings`: a cheat sheet generated
  from the layer's own buttons, triggers and sticks (`info::layer_sheet`), so it can't drift
  from the real bindings. A layer can also list more info overlays of its game to show with it
  (`also_info`), and wait `indicator_delay_ms` before showing any of it, so a quick Guide tap
  (which reaches Steam) doesn't flash them. The layer itself is active immediately.
- **Headings.** The generated sheet is headed "Hold {guide} and press" (a layer's
  `indicator_title`), and each pack's Controls overlay "This game's controls" (an info overlay's
  new `title`), so it is clear that the shortcuts need Guide held and the centre panel is the
  game's own mapping.
- **Defaults.** The Guide layer uses `bindings`, bottom left, after 250 ms. In the library
  packs it also shows the game's `Controls` overlay, so two overlays are up while Guide is held.
- **Removed from the packs.** The Start + Select combo that showed Controls, Deus Ex's startup
  `Hints` overlay, and the pack shortcuts for the on-screen keyboard and numpad (D-pad Up
  gestures in Deus Ex, Select hold in StarCraft's Gameplay profile). Guide + X / Guide + Y
  replace them. Pack descriptions and Controls rows are updated.

## Rollout

- **Templates and defaults.** `passthrough`, `pc_action`, `strategy` and the other templates
  set Guide to the Shift instead of `NextProfile`; new games and General get the layer.
- **Safety invariant.** The template test that "Guide always cycles profiles" becomes: every
  profile's Guide is the Shift, and the Guide layer binds Next profile, so no profile can trap
  you.
- **Existing configs.** A migration changes Guide only where it is still exactly
  `NextProfile`, and adds the `Guide` layer only when the game has no layer of that name.
  Edited bindings are left alone.
- **Packs.** The five library packs get the layer and the new Guide binding, and a version
  bump so installed games get the normal Update badge.
- **Shortcuts that become redundant** (for example pack bindings that open the keyboard or
  numpad) are reviewed and trimmed in the packs.

## Build order

1. Engine: `Shift` action and the swallow-unbound layer option, with tests.
2. `guide_layer()` with the config-only bindings (keyboard, numpad, clicks, mouse stick, keys,
   next profile), wired into General, templates and the migration. Usable on its own.
3. `Screenshot`.
4. `ForceQuit`.
5. `ToggleRecording`.
6. Packs, version bumps and the remaining docs (`specs.md` Guide open question, `layers.md`
   pointer).

## Open questions

- Does Steam Input treat a tap that arrives on release differently from a normal press, for
  example the Steam menu opening on press? Check on a real setup after step 2.
- Whether gpu-screen-recorder needs a portal prompt on each start under the user's compositor;
  if so, recording may need a fixed capture target.

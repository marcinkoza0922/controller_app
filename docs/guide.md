# Guide button as a shift key

Status: steps 1–2 implemented (2026-10-07): the swallow option, the default
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
| Mechanism | A **Guide layer** (see `layers.md`), held by the Guide button. |
| Plain Guide | `Layer("Guide")`. While the daemon is controlling an app, it owns the Guide button: nothing reaches Steam on a press, hold or single tap. A **double tap** is a gesture that sends an ordinary Guide (`BTN_MODE`) to the virtual pad, which opens the Steam overlay. |
| Unbound inputs while held | Swallowed (a layer option, on for the Guide layer): Guide+A does nothing rather than pressing A in the game. |
| Where it lives | A copy of the layer in General and in every game and pack, all built by one `guide_layer()` function. Editable per game. |
| Profile switching | Moves to Guide + D-pad Up (Next profile). |
| New actions | `Screenshot`, `ToggleRecording`, `ForceQuit`. Built-in and named, never a generic "run command", because packs are shared files. |
| Recording tool | gpu-screen-recorder. |
| Force quit | Held for about 2 s inside the action itself (not the profile's long-press time), with a toast and rumble while it counts. |

## Default Guide layer

| Input | Action |
|---|---|
| Guide (double tap) | Guide to Steam |
| + X | Open the on-screen keyboard |
| + Y | Open the on-screen numpad |
| + LB | Media controls (see below) |
| + RB | Screenshot to `~/Pictures/Screenshots/<game>/` |
| + L3 | Start / stop video recording |
| + RT | Left click |
| + LT | Right click |
| + Right stick | Move the mouse |
| + B (held ~2 s) | Force quit the focused window |
| + D-pad Right / Down / Left | Enter / Tab / Escape |
| + D-pad Up | Next profile |

## New actions

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

## Media controls

`ToggleMedia` (Guide + LB) opens a panel at the top of the screen for any MPRIS player
(Spotify, a browser playing YouTube Music, a local player). `src/media.rs` has a worker thread
on the session bus, so a stuck player can't stall the daemon. It shows title, artist, player,
progress and volume. While it is open the controller drives it: D-pad left / right = seek 10 s,
up / down = volume, LB / RB = previous / next track, A = play / pause, Y = switch player, B (or Guide +
LB again) = close. It also closes after 10 s without input. The player that is playing is
chosen; Y overrides that until it goes away. Existing configs keep their stored Guide layer,
so bind `ToggleMedia` there by hand.

## Info overlays

Holding Guide shows what it does, and games add to it.

- **Layer indicators.** A layer's indicator can now be `bindings`: a cheat sheet generated
  from the layer's own buttons, triggers and sticks (`info::layer_sheet`), so it can't drift
  from the real bindings. A layer can wait `indicator_delay_ms` before showing its indicator, so a quick
  tap doesn't flash it. The layer itself is active immediately.
- **Headings.** The generated sheet is headed "Hold {guide} and press" (a layer's
  `indicator_title`), and each pack's Controls overlay "This game's controls" (an info overlay's
  new `title`), so it is clear that the shortcuts need Guide held and the centre panel is the
  game's own mapping.
- **Defaults.** The Guide layer uses `bindings`, bottom left, after 250 ms. In the library
  packs the Guide button has two outputs, `Layer("Guide")` and `ShowInfo("Controls")`, so two
  overlays are up while Guide is held.
- **Removed from the packs.** The Start + Select combo that showed Controls, Deus Ex's startup
  `Hints` overlay, and the pack shortcuts for the on-screen keyboard and numpad (D-pad Up
  gestures in Deus Ex, Select hold in StarCraft's Gameplay profile). Guide + X / Guide + Y
  replace them. Pack descriptions and Controls rows are updated.

## Rollout

- **Templates and defaults.** `passthrough`, `pc_action`, `strategy` and the other templates
  set Guide to hold the layer instead of `NextProfile`; new games and General get the layer.
- **Safety invariant.** The template test that "Guide always cycles profiles" becomes: every
  profile's Guide holds the layer, and the Guide layer binds Next profile, so no profile can trap
  you.
- **Existing configs.** A migration changes Guide only where it is still exactly
  `NextProfile`, and adds the `Guide` layer only when the game has no layer of that name.
  Edited bindings are left alone.
- **Packs.** The five library packs get the layer and the new Guide binding, and a version
  bump so installed games get the normal Update badge.
- **Shortcuts that become redundant** (for example pack bindings that open the keyboard or
  numpad) are reviewed and trimmed in the packs.

## Build order

1. Engine: the swallow-unbound layer option, with tests.
2. `guide_layer()` with the config-only bindings (keyboard, numpad, clicks, mouse stick, keys,
   next profile), wired into General, templates and the migration. Usable on its own.
3. `Screenshot`.
4. `ForceQuit`.
5. `ToggleRecording`.
6. Packs, version bumps and the remaining docs (`specs.md` Guide open question, `layers.md`
   pointer).

## Open questions

- Whether gpu-screen-recorder needs a portal prompt on each start under the user's compositor;
  if so, recording may need a fixed capture target.

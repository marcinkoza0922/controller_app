# In-game layout editing and Quick Settings

Status: phases 1, 2a and 2b are done, and the save and discard prompt is in (branch `in-game-layout-edit`, not committed). The one thing the editor doesn't offer is Multi, which holds several actions in one slot; it stays in the pack file.

## Goal

A player can change the controls of the game they are playing without leaving it. Controller input drives the UI, and the UI draws over the game.

## Decisions

| Topic | Decision |
|---|---|
| Entry | Guide + Start pressed together opens a hardcoded menu over the game. |
| Menu items | "Quick Settings" first, "Edit Controls" last. |
| Quick Settings | Stick speed (for sticks that move the mouse), gyro sensitivity (gyro in mouse mode), Invert Y (every Y inversion in the profile at once), and the profile when the game has more than one. |
| Quick Settings target | The active profile of the active game. Edits are saved to the config and apply at once. |
| Editor scope | The full pack of the current game. Other games and the General profile are not shown. |
| Editor overlay | Drawn over the game. Controller input goes to the UI while it is open. The game is not paused. |
| Editor layout | Simplified: one column, large text, current game only. |
| Editor apply | Each change applies live to the running mappings. Nothing reaches the config file until Save. Closing with unsaved changes asks: save and close, discard and close, or keep editing. Discard puts back the config as it was when the menu opened. |
| New pack | A game with no pack gets one. Outside any game, a Quick Settings change makes a pack for the window in front: the active profile is copied from General, the change goes into the copy, and General is left as it was. The pack is named after the executable. Without a window, the change stays in General. (Edit Controls is refused instead; see the phase 2a notes.) |
| Navigation | Left stick and D-pad move the cursor. A chooses. B backs out. Bumpers (switching tabs) are not used yet, since the editor has no tabs. |
| Macros and turbo | Still in the app (turbo wraps a macro). Included in the editor. |

## Phase 1 (done)

- `src/system_menu.rs`: the menu state machine, the Quick Settings rows and value changes, and the Guide + Start chord check. Unit tests cover the chord, the menu order, stick-speed and Invert Y changes, and stick stepping.
- `src/daemon.rs`: `Active::Settings`. `chord_opens_menu` runs before input routing and opens the menu. `settings_input` applies the menu's outcome. The menu is drawn as a normal `MenuView`, so the overlay process does not change.
- Pack creation: `Config::add_game_for_window` (`src/config/window_game.rs`) and `Daemon::pack_from_general`.

## Phase 2a (done): Edit Controls for buttons

- `src/layout_editor.rs`: three pages. The button list (each row shows the button's action), one button's action (Gamepad button…, Disabled, On-screen keyboard, Screenshot), and the choice of gamepad button. A choice goes back to the button list.
- Choosing Edit Controls outside a game makes the pack first (`pack_from_general`), so the editor edits the game's own profile. With no window in front, it is refused: the menu goes back to its top page and says "Edit Controls needs a game" (`settings_input` in `daemon.rs`).
- The editor is drawn with `MenuView`, so the overlay process didn't change.
- Button rows carry the controller chips (A, B, LB, LS and so on, as the rest of the app draws them): the top page, the pad picker, gestures, combos and layer bindings. Stick and trigger rows use LS, RS, LT and RT.
- In a two-column list, up and down follow the order the list reads in: bottom of the left column to top of the right, and back round (the last row to the first). Left and right move to the other column on the same row. Both columns scroll together. The rules are `list_step`, `list_start` and `list_columns` in `menu.rs`, shared by the config menus, the Guide menu and the editor. On an editor page in two columns, left and right move between columns rather than changing a value.
- Rows carry a tone (`Tone` in `menu.rs`): adding rows ("Add combo", a macro's tap, press or release, and so on) are tinted green, and removing or deleting rows ("Delete combo", "Remove last step", "Delete macro") red. Ordinary items, such as a macro's steps, keep the plain color. This applies to every list menu that sets a tone. Added rows are lime (`#5a9a1a` mixed three-quarters into the row, about 4.7:1 with white text). Button rows draw the same controller glyphs as the info overlays, for the controller in use (`OverlayFrame::family`), not text badges; stick and trigger rows keep their text tags. A Settings page switch, "Color-blind mode" (`Config::colorblind_tones`), draws added rows in blue and removed rows in orange, and puts a + or − in front of each, so they don't rely on red and green.
- Lists longer than 8 rows show in two columns, and longer ones scroll with the cursor (`ONE_COLUMN_ROWS` and `TWO_COLUMN_ROWS` in `overlay.rs`), with ▲ / ▼ when rows are hidden. This applies to every list menu.
- Breadcrumbs: a page shows the menus it was opened from above its title ("Menu › Edit Controls › A button"), in place of the old "N deep" suffix. A config menu gets them from its menu stack (`MenuView::crumbs`); the editor from its own page history.

## Phase 2b

Sticks (done): Edit Controls lists Left stick and Right stick after the buttons. A stick page sets its action (Disabled, Gamepad stick, Mouse, Scroll), its speed (Mouse and Scroll), Invert Y, and its deadzone. Left and right on the D-pad or stick change the speed and deadzone by one step. Rings and flick are not editable here (zones are, see below); a stick with one shows "Other (edit in the pack file)", and choosing its action moves it to Disabled.

Triggers (done): Action (Disabled, Gamepad trigger, Button), and for a Button trigger, what it acts as (six pad buttons) and the press point. Gyro (done): Mode (Off or Mouse), mouse sensitivity, Invert X and Y, and the noise threshold. Stick and steering gyro modes are edited in the pack file. A stick or gyro mode the editor doesn't cycle through is left as it is, not reset.

Layers (done): the Layers row lists the game's layers and "Add layer" makes one. A layer's page sets each button's action (or leaves it unbound). A button can hold a layer through "Hold a layer…" on its action page, which lists the game's layers. A new layer does nothing until a button holds it.

Macros (done): the Macros row lists the game's macros, and "Add macro" makes one. A macro's page adds a tap, press or release (each picks a button), a stick step (one of nine directions for either stick), a 100 ms wait, and removes the last step. A step row removes that step. "Delete macro" removes the macro unless a button, layer or menu of the game still uses it; the page says so when it does.

Zones (done): a stick's or trigger's page has a Zones row. A zone's page sets its start and end (moved with left and right, the start never passing the end), its action (the same choices as a button, with Keys and Hold a layer), and deletes it. A new zone covers the top half of the travel.


Gyro activation (done): "Activation" cycles always, held with LT or RT, off while LT or RT is held, and toggled by LT or RT. Other inputs stay in the pack file.

Action choices (done): every simple `ButtonAction` the editor can offer is in the action page: Disabled, the on-screen keyboard and numpad, screenshot, recording, media controls, force quit, mouse buttons (left, right, middle, back, forward), wheel in all four directions, and keys (a picker that ticks several at once). Toggle, turbo and the turbo rate or the toggle's start state are on the same page. Picker rows open a list of the game's layers, macros, menus, info overlays or log overlays. A macro picked this way plays once; its repeat setting stays in the pack file.

Still in the pack file: Multi, which holds several actions in one slot. Stick and steering gyro modes, and the gyro's other inputs, are there too.

Gestures and combos (done): a button's Gestures page sets its double tap, triple tap and long press (or clears them to "Not set"). Combos lists the profile's combos, with "Add combo". A combo page ticks its buttons (two or more make a combo), sets its action, and deletes it. The gesture and combo actions use the same choices as a button.

## Open questions

- Each change saves the whole config. If that is too heavy while a game runs, phase 2b can batch edits behind a Save.

## User docs

The user-facing pages are in `wiki/`: `In-Game-Menu.md` (quick settings, saving, color-blind mode) and `Editing-Controls-in-Game.md` (every page of the editor). `Menus.md`, `Info-Overlays.md`, `Buttons-and-Actions.md`, `Sticks-and-Triggers.md`, `Layers.md`, `Macros.md`, `Gyro.md`, `Games-and-Profiles.md` and `Getting-Started.md` each have a short note with a link. The README has an "In-game menu" section. Update those when the editor changes.

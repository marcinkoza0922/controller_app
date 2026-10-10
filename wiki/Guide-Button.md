# The Guide button

The Guide button (the PlayStation, Xbox or Home button) works the same in every game. Padwight doesn't pass it to the game or to Steam, so a power button is safe to press. Guide opens the **Guide overlay**, which has the game's notes, a menu, and what each button does. It also holds the system shortcuts: keyboard, numpad, media, screenshots, recording and force quit.

## The Guide overlay

Tap Guide to open the overlay, and tap it again to close it. It opens when you let go, so a quick press with another button is a shortcut, not an overlay.

The overlay has four parts:

- **Notes** on the left: what the game's author wants you to know, such as limits in the control scheme, or when to switch layers. Set on the profile's **Guide** tab.
- **The menu** in the middle: Quick Settings, the on-screen keyboard and numpad, mouse mode, media controls, the next profile and, while Steam runs, the Steam overlay.
- **The Mappings box** on the right: what each button, trigger and stick does in the current profile, with its gestures and combos. The text, and which rows show, can be changed on the **Guide** tab.
- **Shortcuts** in the bottom left corner: the Guide shortcuts listed below, and the mouse-mode controls while mouse mode is on.

Move the menu with the D-pad or the left stick (hold it to repeat), choose with **A**, and close it with **B**.

## Shortcuts

Hold Guide and press one of these, or, with the overlay open, press it alone. Either way the overlay closes and the shortcut runs.

| Shortcut | Action |
|---|---|
| X | On-screen keyboard |
| Y | On-screen numpad |
| LB | Media controls |
| RB | Screenshot |
| L3 | Start or stop screen recording |
| Hold Select for 2 seconds | Force quit the focused window |

Anything else pressed with Guide held does nothing, so it never reaches the game underneath. These shortcuts are fixed. To reach the same actions from other buttons, bind them on the **Buttons** tab, as usual.

Pressing Guide also closes the on-screen keyboard, numpad or media controls, however they were opened.

### Force quit

Holding Select for 2 seconds ends the focused window's process tree (SIGTERM, then SIGKILL after 3 seconds). It never touches the desktop, the compositor or the display server. A toast and rumble show while the hold counts, and letting go early cancels it.

### Screenshots and recordings

The screenshot action saves to `~/Pictures/Screenshots/<game>/`. Recording uses gpu-screen-recorder, saving to `~/Videos/Recordings/<game>/`. If gpu-screen-recorder isn't installed, the action shows a toast saying so.

## Mouse mode

Hold Guide and press the right stick, or press the right stick with the overlay open, to turn on mouse mode. The overlay closes, and the bottom left corner lists the controls:

| Control | Action |
|---|---|
| Right stick | Moves the mouse |
| Left stick | Scrolls. Pressing it clicks the middle button |
| RT / LT | Left click / right click |
| LB / RB | Back / forward |
| D-pad Right, Down, Left | Enter, Tab, Escape |
| D-pad Up | Holds Shift, so a click or key with it is shifted |
| X | Holds Ctrl |
| Y | Holds Alt |
| B | Delete |

Press Guide again to leave mouse mode. Your profile is still the one you chose. Other buttons do nothing in mouse mode.

## The Guide tab

Each profile has a **Guide** tab in the profile editor. It holds two things:

- **Notes**: the text for the Notes box. Plain text, with one line per line. `{buttons}` show as glyphs, as on info overlays.
- **Mapping guide**: the rows behind the Mappings box. For each input, gesture or combo you can:
  - change its text, such as "Use" for A where the button is mapped to E. Clear the text to go back to the mapping's own description;
  - show or hide it. A hidden row leaves the overlay, but the mapping still works;
  - share a row with another input, so "Lean" can be one row for both D-pad sides. Choose the other input in **Share with**. A shared row takes more inputs with **Add an input**, and **Split** takes it apart.

Rows follow the profile's mappings. If you change what A does, the row updates, unless you've written its text yourself. Gestures and combos are listed as well, with their own text and visibility, marked ×2 for a double tap, ×3 for a triple tap and "hold" for a long press. Combos show their buttons joined with "+", and shared rows with commas.

The overlay's Notes and Mappings boxes show only while the overlay is open.

## Changing controls while you play

Guide + Start still opens the [in-game menu](In-Game-Menu), with Quick Settings and Edit Controls.

## Steam

Steam's overlay is in the Guide overlay's menu, and only while Steam is running. Padwight doesn't pass Guide to Steam on a double tap any more.

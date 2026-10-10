# On-screen keyboard and numpad

Both pads open over everything, including fullscreen games. They are layer-shell surfaces that never take keyboard focus, so keys go to the window underneath.

They need a compositor with layer-shell support: KDE Plasma, Sway, Hyprland and similar. GNOME doesn't have it. See [Desktop support](Desktop-Support).

## Opening them

- **On-screen keyboard**: the "On-screen keyboard" action, the button on the Overview page, or `padwight overlay-toggle`. Hold Guide and press X, or open the Guide overlay and press X. See [The Guide button](Guide-Button).
- **On-screen numpad**: the "On-screen numpad" action, or `padwight numpad-toggle`. Hold Guide and press Y, or open the Guide overlay and press Y.

Opening one closes the other.

## Keyboard controls

| Control | Action |
|---|---|
| D-pad / left stick | Move between keys (repeats while held) |
| A | Press the selected key. Holding A holds the key. |
| Shift / Ctrl / Alt / Super | Latch for the next key |
| X / Y / Start | Backspace / Space / Enter |
| Hold LT | Hold Shift |
| Hold B | Close |

## Numpad controls

The numpad has the digits 0 to 9 and a dot, for codes and number fields.

| Control | Action |
|---|---|
| D-pad / left stick | Move |
| A | Press the selected number |
| X | Backspace |
| Start | Enter |
| Hold B | Close |

The numpad types the top-row number keys, so it works whatever the Num Lock state is.

## Appearance

Both pads have their own position, size and colours on the **Settings** page. A game can use its own look for either one, from its Details tab. The [media controls](Buttons-and-Actions) and the [in-game menu](In-Game-Menu) have the same kind of Appearance section, on the same pages.

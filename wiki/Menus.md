# Menus

A menu is an on-screen list of choices. A game's **Menus** tab holds them, and shared menus can be used by every game.

## Opening a menu

Use **Open menu…** on any button, gesture, combo, trigger or stick direction. The menu is on screen only while that input is held. Letting go closes it.

To keep a menu up without holding, wrap the action in **Toggle…**. The menu then stays up until the input is pressed again.

Choosing an item taps its action, as if the button had been pressed. Items can be keys, [macros](Macros), toggles or the keyboard. The menu stays up, so you can make several picks.

A quick tap can't hold a menu up. A menu opened by a tap, such as on a double tap that isn't the button's last gesture, needs a Toggle.

## Menu kinds

- **Radial**: aim a stick at an item. Whatever is aimed at when you let go is chosen.
- **Directional**: four slots, on the D-pad or the face buttons.
- **List**: move with the D-pad or left stick. A chooses.
- **Button menu**: a list where each item can also be chosen with its own button.
- **Carousel**: cycle with the bumpers, triggers, D-pad or a stick. A chooses.
- **Grid**: up to 6 columns and 6 rows. Move in all four directions with the D-pad or left stick. A chooses.

## Nested menus

An item can open another menu of the same kind. Radial menus can't open other menus. The new menu is a child of the first, and letting go closes both.

While a menu is open, the controller drives it. Anything the mappings were holding is released.

Menus opened from the command line (`padwight menu <name>`) or from an analog zone can't be held. They stay open until an item is chosen or B is pressed (Select, for face-button menus).

## Appearance

Each menu is a collapsible card on the Menus tab, with a live preview. Under **Appearance**, each menu and the on-screen keyboard have their own:

- position: a 3×3 grid of corners, edges and centre, so it fits any screen,
- size: 50% to 200%,
- colours and opacity for the background, the items and the selected item.

Text switches between light and dark to stay readable against the background.

The overlay process stays running, invisibly, between uses, so menus appear immediately.

# Menus

A menu is an on-screen list of choices. A game's **Menus** tab holds them, and shared menus can be used by every game.

## Opening a menu

Use **Open menu…** on any button, gesture, combo, trigger or stick direction. The menu is on screen only while that input is held. Letting go closes it.

To keep a menu up without holding, wrap the action in **Toggle…**. The menu then stays up until the input is pressed again.

Choosing an item taps its action, as if the button had been pressed. Items can be keys, [macros](Macros), toggles or the keyboard. The menu stays up, so you can make several picks.

A quick tap can't hold a menu up. A menu opened by a tap, such as on a double tap that isn't the button's last gesture, needs a Toggle.

![The Belt menu on the Menus tab: a radial preview with ten slots and the items list below](https://raw.githubusercontent.com/marcinkoza0922/padwight/main/docs/images/menus-belt.png)

## Menu kinds

- **Radial**: aim a stick at an item. Whatever is aimed at when you let go is chosen. The items are arcs of a circle, evenly sized unless you change an item's arc size; **Use boxes** puts them in boxes at equal angles instead.
- **Directional**: four slots, on the D-pad or the face buttons.
- **List**: move with the D-pad or left stick. A chooses.
- **Button menu**: a list where each item can also be chosen with its own button.
- **Carousel**: cycle with the bumpers, triggers, D-pad or a stick. A chooses.
- **Grid**: up to 6 columns and 6 rows. Move in all four directions with the D-pad or left stick. A chooses.

## Nested menus

An item can open another menu of the same kind. Radial menus can't open other menus. The new menu is a child of the first, and letting go closes both.

While a menu is open, the controller drives it. Anything the mappings were holding is released.

Menus opened from the command line (`padwight menu <name>`) or from an analog zone can't be held. They stay open until an item is chosen or B is pressed (Select, for face-button menus).

## Long menus and colours

A list with more than eight rows is shown in two columns. Down moves through the list in reading order, from the bottom of the left column to the top of the right and back round. Left and right move between the columns. Both columns scroll with the cursor.

Each menu shows where it is above its title, such as `Menu › Edit Controls › A button`.

Button rows show the controller's glyphs, the same as the [info overlays](Info-Overlays). In the editor, rows that add something are tinted lime and rows that remove something are tinted red. **Colour-blind mode** on the Settings page uses blue and orange, with a + or − in front.

## Appearance

Each menu is a collapsible card on the Menus tab, with a live preview. Under **Appearance**, each menu and the on-screen keyboard have their own:

- position: a 3×3 grid of corners, edges and centre, so it fits any screen,
- size: 50% to 200%,
- colours and opacity for the background, the items and the selected item.

Text switches between light and dark to stay readable against the background.

The overlay process stays running, invisibly, between uses, so menus appear immediately.

## Overlay motion

Overlays move as they open and close, as the cursor moves and as something is picked. Under **Settings → Overlay motion** each kind of overlay has its own style: the on-screen keyboard, the numpad, menus, media controls, library offers, info overlays and input logs. Subtle is the default.

- **Subtle**: a quick fade, and the highlight glides from item to item.
- **Playful**: panels drop in with a bounce, and the highlight overshoots its item. Suits cheerful games.
- **Stagger**: items fade in one after another.
- **Grim**: slow fades that rise into place, and a dim, heavy pulse on a pick. Suits dark fantasy.
- **Brutal**: snaps in and out, the highlight jumps, and a pick jolts and flashes. Suits shooters and fights.
- **Off**: nothing moves.

Each setup can set its own on its **Details** tab. Any kind it doesn't set uses the global motion.

## Menu sounds

Each setup can play a faint tick as the cursor moves and a different sound when an item is picked. Set them on the setup's **Details** tab, where each sound can be previewed with **Play**.

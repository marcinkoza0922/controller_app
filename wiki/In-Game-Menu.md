# The in-game menu

Guide + Start, pressed together, opens a menu over the game you're playing. Tapping Guide alone opens the [Guide overlay](Guide-Button) instead, with the game's notes, its mappings and the system shortcuts. You can change a few settings from it, or open the full editor for the game's controls. The menu is built into padwight, not part of any profile, so it's there for every game.

Use the controller to drive it: the left stick or D-pad moves, **A** chooses, and **B** goes back or closes the menu.

## The top page

- **Quick Settings**: a few values, changed on the spot.
- **Edit Controls**: every button, stick, trigger, gyro setting, combo, layer, and macro of the game. See [Editing controls in game](Editing-Controls-in-Game).

## Quick Settings

Only the rows that apply to the profile you're playing are listed:

- **Left stick speed** and **Right stick speed**: for sticks set to Mouse.
- **Gyro sensitivity**: when gyro is set to Mouse.
- **Invert Y**: flips every Y inversion in the profile at once, sticks and gyro.
- **Profile**: switches to the next or previous profile of the game, when it has more than one.

Left and right change a number by about 10%. A chooses a toggle or the next profile.

### Outside a game

With no game running, Quick Settings doesn't change General. It makes a game instead. Your change goes into a new game named after the window in front, such as `eldenring` for `eldenring.exe`. That game gets a copy of General's active profile, with your change, and a rule that picks it for the window. General stays as it was. The next time that window is focused, the new game's profile is used.

Without a window in front, the change goes to General.

## Appearance

The menu has its own position, size and colours, set in the **In-game menu** card on the **Settings** page, or per game on its Details tab. Edit Controls uses the same look. The media controls have their own look the same way.

## Saving

Changes take effect straight away, but they aren't written to the config file until you save.

- **Closing** the menu with unsaved changes asks: **Save and close**, **Discard and close**, or **Keep editing**.
- **Discard** puts back the settings as they were when the menu opened, including any game it made.

## Edit Controls

Choosing **Edit Controls** opens the editor for the game in front. It needs a game: if no game or window is in front, the menu says so and stays on its top page, so nothing is changed. If you're outside a game but a window is in front, the editor makes a game for it first, the same way Quick Settings does.

[Editing controls in game](Editing-Controls-in-Game) lists what each page does.

## Long lists and breadcrumbs

A list with more than eight rows is shown in two columns, filled top to bottom with the left first. Moving the cursor follows that order:

- Down from the bottom of the left column goes to the top of the right.
- Down from the bottom of the right goes to the first option, and up from the first goes to the bottom of the right.
- Left and right move between the columns, on the same row.

The top of each page shows where you are, such as `Menu › Edit Controls › A button`.

## Colours and glyphs

Buttons in menus are drawn with the same controller glyphs as the info overlays, for the controller in use. Rows that add something are tinted lime green, and rows that remove something or delete it are tinted red. The labels also say what the row does, such as "Add combo" and "Delete combo".

If red and green are hard to tell apart, turn on **Colour-blind mode** on the Settings page, under **Menu colours**. Added rows then turn blue, removed rows turn orange, and each one starts with a + or −.

## See also

- [Menus](Menus): the menus you set up for buttons and gestures.
- [Info overlays](Info-Overlays): the same button glyphs, on screen while a game runs.

# Getting started

This walks through the first mapping. Once [Installation](Installation) is done, run `padwight` to open the settings window.

## 1. Check the controller

The **Overview** page draws your controller and labels each input with the active profile's mapping. The controller list on the same page shows what padwight has found. Use **Test rumble** to check force feedback, and **Calibrate gyro** if the controller has motion sensors (keep it still while it runs).

![The Overview page, with the controller drawing and the sidebar of games](https://raw.githubusercontent.com/marcinkoza0922/padwight/main/docs/images/overview.png)

## 2. Know what the defaults do

**General** is the built-in game for the desktop and for plain gamepad use, and it has two profiles:

- **Gamepad**: 1:1 passthrough. The pad behaves as if padwight weren't there.
- **Desktop**: the left stick moves the mouse, the right stick scrolls, A and B click, and LB+RB sends Alt+Tab.

The **Settings** page chooses the default profile, which is used when no game is in focus.

## 3. Edit a mapping

Open a game's **Profiles** tab, pick a profile, and click a mapping's name to edit it. Each mapping collapses to a one-line summary such as `A ▸ Left click`.

If you don't know which input is which, use **Find by pressing**: it jumps to whatever you press or push on the controller.

## 4. Add a game

Click **+ Add game**. You can:

- pick a game from the **built-in library** (games found installed in Steam, Heroic or Lutris, or running, are listed first),
- start from an **empty game** from a template, or
- **import a file** (a `.padpack`). See [Packs and the library](Packs-and-the-Library).

Templates include **PC action**, **Strategy** and **Retro / platformer**. Each one is a complete starting point for its kind of game.

![The Add a game picker, with the library's built-in games and the template dropdown](https://raw.githubusercontent.com/marcinkoza0922/padwight/main/docs/images/add-game.png)

## 5. Save

Edits stay in memory until you click **Save & apply**. **Revert** drops them. Problems that would block saving (unknown keys, missing macros, incomplete combos) are flagged on the row and with ⚠ on its tab.

## Changing controls while you play

Press Guide and Start together to open the [in-game menu](In-Game-Menu). It has quick settings for the game you're playing, and the editor for its controls.

## Next

- [Games and profiles](Games-and-Profiles): make a game switch profiles when it gets focus.
- [Buttons and actions](Buttons-and-Actions): what a button can do.
- [Layers](Layers): change mappings while a button is held.

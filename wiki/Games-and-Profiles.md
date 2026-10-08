# Games and profiles

Everything is organized by game. A game holds its own:

- profiles,
- [macros](Macros), [menus](Menus) and [info overlays](Info-Overlays),
- [layers](Layers),
- and the **auto-switch rules** that activate it.

All of a game's profiles can use all of its items.

## General and shared items

**General** is a built-in game with no rules. It covers the desktop and plain gamepad use. Its Macros, Menus and Info overlays tabs also hold the **shared** items, which every profile of every game can use.

Names only need to be unique within a game. A game's item can't reuse the name of a shared item.

## The game page

The sidebar lists General and your games, with a search box. Each game's page has these sub-tabs:

- **Profiles**: edit the profiles (buttons, sticks and triggers, combos, gyro).
- **Layers**: the game's layers.
- **Macros**, **Menus**, **Info overlays**.
- **Details**: name, rules, export and delete.

On the Macros, Menus and Info overlays tabs, **Copy from another game…** previews another game's (or a library game's) items and copies one in to adapt.

## Auto-switch rules

![The Details tab: the game's name, its auto-switch rules, and the on-screen keyboard settings](https://raw.githubusercontent.com/marcinkoza0922/padwight/main/docs/images/details-rules.png)

A game's **Details** tab holds its rules. When the game's window gets focus, padwight switches to one of the game's profiles. When the window loses focus, the **default profile** (set on the Settings page) takes over. You can also keep the current profile.

A rule matches on one of:

- **Executable**: the program's file name. For Wine and Proton games this is the Windows `.exe` (for example `eldenring.exe`), not the Wine loader.
- **Steam App ID**: taken from the environment Steam sets, or from Proton's `steam_app_<id>` window class.
- **Window class**.

Rules are checked game by game, in order. A rule can be switched off without deleting it.

The Details tab lists recently focused windows, with a one-click **+ Rule** for each.

Each switch, and each game launch, shows a short toast naming the active profile and its game. Switching back to the default when a game loses focus doesn't show one.

Switching with Guide or from the GUI holds until focus changes again. Focusing the settings window never switches profiles.

Automatic switching depends on the desktop. See [Desktop support](Desktop-Support).

## Profiles

Each profile is a complete mapping: buttons, sticks, triggers, combos, gyro and layers. To make a second profile for a game, use **Duplicate** on the Profiles tab, or start from a template under **New from template…**.

The Guide button is the usual way to move between profiles in a game. In the default Guide layer, Guide + D-pad Up goes to the next profile. See [Buttons and actions](Buttons-and-Actions#the-guide-button).

## Older configs

A config from before games existed is converted on first load. The original is kept as `config.toml.old`. Each profile that an auto-switch rule pointed to becomes a game of its own with those rules. The other profiles go to General. Macros, menus and info overlays that a single game's profiles used move into that game. The rest become shared.

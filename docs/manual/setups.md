# Controller setups and profiles

A **controller setup** holds everything padwight keeps for one game: its profiles, its macros, menus, info overlays and layers, and the rules that pick it when that game's window has focus. In the sidebar, a setup is listed under **Setups**. **General** is the setup for the desktop and plain controller use. It has no rules, so it's used when no other setup matches.

## Profiles

A profile is one complete set of mappings inside a setup, for example one for a whole game or one for a different way of playing it. Profiles don't mix with each other. If a game needs a different control scheme for a context inside play, such as driving, use a [layer](layers.md) instead. On the **Profiles** tab, **Duplicate** copies the profile you're on, and **New from template** starts from a ready-made one.

## Switching profiles

- **Automatically:** a rule in the setup picks its profile when the game's window has focus.
- **By hand:** choose a profile in the top bar's *Active profile* list, or press Guide + Start and pick one in Quick Settings.
- **When no setup matches:** the *When no setup matches, use* setting on the App settings page picks the profile.

## Rules

A rule matches a window by its program name, its Steam App ID, or its window class. Wine and Proton games match by their `.exe` name.

To make a rule quickly, focus the game and use **Recently focused windows** on the setup's Details tab. It adds a rule for the profile you're editing.

> **Tip:** automatic switching needs a desktop that reports which window has focus: KDE Plasma, GNOME, Sway or Hyprland. See [Troubleshooting](troubleshooting.md).

## Shared items

The **Macros**, **Menus** and **Info overlays** tabs on General also hold shared items. Every profile of every setup can use them. Names only need to be unique within one setup.

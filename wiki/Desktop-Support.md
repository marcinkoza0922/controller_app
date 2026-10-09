# Desktop support

Automatic profile switching needs to know which window has focus. padwight picks the right tracker for your desktop by itself, and logs which one it's using. `padwight status` shows it too.

| Desktop | Focus tracking | Automatic switching | On-screen keyboard, menus and overlays |
|---|---|---|---|
| **KDE Plasma** (Wayland or X11) | A small KWin script reports focus changes over D-Bus | Yes | Yes |
| **Sway** | The compositor's IPC socket (`$SWAYSOCK`) | Yes | Yes |
| **Hyprland** | Its event and command sockets | Yes | Yes |
| **GNOME** | GNOME Shell extension | Yes | No |
| **wlroots compositors** (labwc, Wayfire, river, …) | The `wlr-foreign-toplevel-management` protocol. Matches by window class, not executable | Yes | Yes |
| Other desktops | None | Only the fallback below | Only if the compositor supports layer-shell |

## GNOME

Focus tracking works through a small GNOME Shell extension, which padwight installs and enables itself. Mutter has no layer-shell, so the on-screen keyboard, menus and overlays can't be shown.

## Flatpak

In a Flatpak, Sway, labwc and Hyprland hide their layer-shell protocol from the app, so the on-screen keyboard and overlays don't show. Sway and labwc also hide the window list, so focus falls back to matching running processes there. Hyprland's focus still works through its socket. KDE Plasma and GNOME work in a Flatpak. Run the native build on Sway, labwc or Hyprland for overlays, or for per-window switching on Sway and labwc.

## Tested

The desktop checks last ran on 2026-10-09 with kernel 7.2.9. GNOME 51.0, KDE Plasma 6.7.5, Sway 1.12, Hyprland 0.56.2 and labwc 0.20.2 pass the focus, profile and input checks (the overlay is skipped on GNOME). The Flatpak build passes on GNOME and KDE, and the Sway, labwc and Hyprland limits above apply to it.

## Fallback

Without a focus tracker, a game's rules still apply while a process matching the rule is running. This is less precise than focus tracking: the game's profile is used whenever it's running, not only when its window is focused.

## Window identification

For each window, padwight works out the executable, the Steam App ID and the window class from `/proc`. Wine and Proton games are matched by their `.exe` name, not by the Wine loader. See [Games and profiles](Games-and-Profiles#auto-switch-rules).

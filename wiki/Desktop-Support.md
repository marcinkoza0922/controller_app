# Desktop support

Automatic profile switching needs to know which window has focus. padwight picks the right tracker for your desktop by itself, and logs which one it's using. `padwight status` shows it too.

| Desktop | Focus tracking | Automatic switching | On-screen keyboard, menus and overlays |
|---|---|---|---|
| **KDE Plasma** (Wayland or X11) | A small KWin script reports focus changes over D-Bus | Yes | Yes |
| **Sway** | The compositor's IPC socket (`$SWAYSOCK`) | Yes | Yes |
| **Hyprland** | Its event and command sockets | Yes | Yes |
| **GNOME** | None | No | No |
| Other desktops | None | Only the fallback below | Only if the compositor supports layer-shell |

## GNOME

GNOME isn't supported. Mutter has no layer-shell, so the on-screen keyboard, menus and overlays can't be shown. It also doesn't let other programs ask which window is focused.

## Fallback

Without a focus tracker, a game's rules still apply while a process matching the rule is running. This is less precise than focus tracking: the game's profile is used whenever it's running, not only when its window is focused.

## Window identification

For each window, padwight works out the executable, the Steam App ID and the window class from `/proc`. Wine and Proton games are matched by their `.exe` name, not by the Wine loader. See [Games and profiles](Games-and-Profiles#auto-switch-rules).

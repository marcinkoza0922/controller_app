# Troubleshooting

## A controller shows up twice (or the game sees two pads)

If Steam Input manages the controller, the game can see both Steam's virtual pad and padwight's. For a pad that padwight manages, turn off Steam Input for it.

PlayStation and Switch controllers are also readable through hidraw, which SDL (and Steam) use directly. The game may then see the real controller as well as padwight's. Either turn off Steam Input for the pad, or set `SDL_JOYSTICK_HIDAPI=0` for the game.

padwight skips virtual uinput devices, such as Steam Input's pad and its own.

## The controller isn't picked up

Check that your user can open `/dev/uinput` and the gamepad device nodes. If not, add a udev rule or join the `input` group. See [Installation](Installation#permissions).

Run `padwight status` to see which controllers the daemon has found.

## X and Y are swapped on an xpad-driver pad

Pads on the `xpad` driver report X and Y by label rather than by position. padwight corrects this automatically.

## Motion or gyro doesn't work on a PlayStation or Switch pad

Install the motion-sensor udev rule. See [Installation](Installation#gyro).

## The profile didn't change when I focused a game

- Check the game's rules on its **Details** tab. Rules match on the executable, Steam App ID or window class, and for Wine and Proton games the match is on the `.exe`.
- Check that the desktop supports focus tracking. See [Desktop support](Desktop-Support). On GNOME, switching doesn't happen.
- Check that the rule is switched on, and that no earlier rule matches the same game.

## The on-screen keyboard, menus or overlays don't appear

These need a compositor with layer-shell support: KDE Plasma, Sway, Hyprland and similar. GNOME doesn't have it.

## The profile changed after a controller was unplugged or swapped

A disconnect releases everything that controller was holding and closes any menu it had open. A toast names the controller that left or arrived.

When the controller comes back, the profile you were on returns, unless you've changed it since. If the replacement controller lacks a feature the profile needs, such as gyro, the game's first profile that doesn't need it takes over, or the default profile if none does. A toast says so.

The change is made once the controllers have looked the same for two 2-second scans. This stops a motion sensor that appears a moment after its pad from making the profile flicker.

## I edited the config file by hand and nothing changed

Run `padwight reload`. The daemon is the only writer of the file while it runs, and it re-reads the file only on reload.

## My old config was converted

padwight converts configs from before games existed on first load. The original is kept as `config.toml.old` (or `.old.2`, and so on). See [Games and profiles](Games-and-Profiles#older-configs).

## Rumble doesn't reach the controller

Rumble from games is forwarded to the physical controller only if it supports force feedback. Use **Test rumble** on the Overview page to check it.

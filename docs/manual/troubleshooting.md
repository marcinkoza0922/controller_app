# Troubleshooting

## The game sees my controller twice

If Steam Input manages the controller, the game sees Steam's virtual controller as well as Padwight's. Turn off Steam Input for that controller.

PlayStation and Switch controllers can also be read directly, so for those you may need to turn off Steam Input, or set `SDL_JOYSTICK_HIDAPI=0` for the game.

## My controller doesn't show up

Run `padwight status` in a terminal to see what the daemon has found. Your user needs access to the controller and to `/dev/uinput`. The README explains the udev rule, or joining the `input` group, that gives it.

## Why did my profile change?

The Overview page's **What's happening** section lists the last few profile switches, newest first. Each one says what made it, such as the rule that matched the window in front, the default profile taking over when a game lost focus, or a profile you picked yourself. Under the controller picture, the same section shows your latest presses and what each did, next to the buttons Padwight sent for them. If the two differ, the mapping is doing something other than what the label says.

## The profile didn't switch when I opened a game

- Check the setup's rules on its **Details** tab, and that the rule is switched on.
- Check that no other rule also matches the game.
- Check that your desktop supports window tracking: KDE Plasma, GNOME, Hyprland, Sway, or a wlroots compositor such as labwc (which matches by window class only).

## The keyboard, menus or overlays don't appear

They need a desktop that supports them: KDE Plasma, Sway, Hyprland or labwc. GNOME doesn't have the layer-shell they use. In a Flatpak, they don't show on Sway, labwc or Hyprland.

## Gyro does nothing on a PlayStation or Switch controller

Those controllers' motion sensors need a permission rule of their own, which the README explains. Install it, then press **Calibrate gyro**.

## The top bar says the daemon isn't running

Start it with `systemctl --user start padwight`. You can still edit in the meantime. Your changes are kept, and they take effect once it starts.

## I edited the config file by hand, and nothing changed

Run `padwight reload`. The daemon reads the file only when it's told to.

## Rumble doesn't reach my controller

Only controllers with force feedback vibrate. Press **Test rumble** on the Overview page to check.

## The profile changed after I unplugged my controller

Unplugging releases everything the controller was holding, and a message says which controller left. When the same controller comes back, the profile you were using returns, unless you've changed it since.

If a replacement controller lacks something the profile needs, such as gyro, a profile that doesn't need it takes over, and a message says so.

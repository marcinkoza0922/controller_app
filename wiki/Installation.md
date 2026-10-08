# Installation

padwight runs on Linux. It needs a Wayland or X11 desktop for the on-screen overlays, and a desktop with focus tracking (KDE Plasma, Sway or Hyprland) for automatic profile switching. See [Desktop support](Desktop-Support).

## Build and install

From a clone of the repository:

```sh
cargo install --path .
mkdir -p ~/.config/systemd/user
cp dist/padwight.service ~/.config/systemd/user/
systemctl --user enable --now padwight
padwight            # open the settings window
```

The service runs `padwight daemon`. It starts with your user session and restarts itself if it fails.

## Permissions

On most desktops (systemd-logind with `uaccess`), your user can already open `/dev/uinput` and the gamepad device nodes, so no root is needed.

If it can't, either:

- add a udev rule that grants access, or
- add yourself to the `input` group.

## Gyro

PlayStation and Switch controllers keep their motion sensors on a separate device that `uaccess` doesn't cover. For gyro, install the motion-sensor rule:

```sh
sudo cp dist/70-padwight-motion.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

See [Gyro](Gyro) for how to use it.

## Controllers that come and go

Controllers can be unplugged, reconnected or swapped while padwight runs. See [Troubleshooting](Troubleshooting) if a pad isn't picked up.

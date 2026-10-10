# Gyro (motion aiming)

Some controllers have motion sensors, called gyro. Padwight can use them to aim or steer: turn or tilt the controller to move the mouse or a stick. DualShock 4, DualSense, Switch Pro and Joy-Con controllers have gyro.

- **Mouse**: turning the controller moves the mouse, like aiming with a mouse.
- **Gamepad stick**: turning moves a virtual stick, which adds to the real one.
- **Steering**: tilt the controller like a steering wheel.

## When gyro is on

- always
- only while you hold a button, such as LT to aim down the sights
- off while you hold a button, which lets you reposition the controller without turning the view
- switched on and off with a toggle

## Drift

Very small movements are ignored, to hide drift. To set the baseline, keep the controller still and press **Calibrate gyro** in the controller list on the Overview page. It takes 2 seconds.

## Profiles that need gyro

On a profile's **Gyro** tab, tick *Can't be played without gyro* if the profile depends on it. When someone imports a pack, players whose controller has no gyro aren't offered that profile.

> **Tip:** PlayStation and Switch controllers also need a motion-sensor permission rule. See [Troubleshooting](troubleshooting.md).

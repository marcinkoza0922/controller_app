# Gyro

Controllers with motion sensors can use their gyro in each profile. These include the DualShock 4, DualSense, Switch Pro and Joy-Cons. On PlayStation and Switch pads, the motion-sensor udev rule is needed. See [Installation](Installation#gyro).

## What gyro can drive

- **Mouse**: gyro aiming, in pixels per degree turned.
- **Gamepad stick**: rotation speed deflects a virtual-pad stick, added to the physical stick. An anti-deadzone gets past the game's own stick deadzone.
- **Steering**: tilt the controller like a wheel to move a stick.

Horizontal aim can come from yaw (turning), roll (tilting), or both. Either axis can be inverted.

## When gyro is on

- **Always on**.
- **On only while holding** an input, for example LT to aim down sights.
- **Off while holding** an input, which acts as a clutch for repositioning the controller.
- **Toggled**.

A **recenter** input sets the current tilt as straight, for steering.

## Drift

Slow movement below a jitter threshold is scaled down, to hide drift. **Calibrate gyro** in the controller list measures the drift while the controller sits still.

![The Gyro tab of a profile, with gyro off and the "Can't be played without gyro" option unticked](https://raw.githubusercontent.com/marcinkoza0922/padwight/main/docs/images/profile-gyro.png)

## Gyro and profiles

Each profile says whether it needs gyro. Tick **Can't be played without gyro** on the Gyro tab when a profile depends on it. This matters for [packs](Packs-and-the-Library): players whose controller has no gyro aren't offered those profiles.

## Examples in the library

- The PC action template turns on gyro mouse aiming while LT is held.
- StarCraft's Gameplay profile has a "+ Gyro" twin, which moves the cursor by tilting, as an extra to the left stick.
- The library's shooters (Deus Ex, F.E.A.R., Max Payne and Max Payne 2) each have three profiles: one without gyro, one with gyro as an extra, and a flick stick profile that needs gyro. The rules point to the plain one first.

# Sticks and triggers

## Sticks

Each stick can output to one of:

- a gamepad stick (the virtual pad's stick, so games see a normal analog stick),
- the mouse pointer,
- the scroll wheel,
- direction keys (WASD or the arrows),
- a [button ring](#button-rings) or a [flick stick](#flick-stick).

Stick settings:

- **Deadzone**, **response curve** and an adjustable **press threshold** (the point where the stick counts as a button press).
- **Acceleration** for mouse sticks: the pointer speeds up while the stick is held at full push.
- **Advanced response**: a ramp time, an outer-edge boost, a vertical speed ratio and smoothing.

Each stick's **directions also act as buttons**: Left Stick Up, Left Stick Right and so on. They can have any action or gesture and can be part of combos. For example, LB + Right Stick Right.

Gamepad outputs include stick directions too. A button or the D-pad can push the virtual stick, and two directions make a diagonal.

## Button rings

A stick can act as a ring of 4, 8 or 12 sectors instead. Pointing the stick into a sector holds that sector's action. Any action a button can have works here.

You can adjust:

- the angle of the first sector,
- how far out the stick must go before a sector counts,
- how sticky the sector boundaries are.

The GUI shows a preview of the ring as you edit it.

## Flick stick

Pushing the stick out turns the camera to that direction at once, spread over about 0.1 seconds. Rotating the stick then turns the camera by the same angle. This is horizontal only, so aim up and down with [gyro](Gyro) or the other stick.

- **Turn size**: the mouse pixels one full turn takes in the game. Set it so a full turn matches the game's own sensitivity. **Test turn** moves the mouse one turn after 3 seconds, so you can check the result.
- Only one stick per profile can be a flick stick.
- **Advanced**: flick time, turning smoothing, a dead angle straight ahead, and optional looking up and down.

## Triggers

A trigger can output to:

- an analog gamepad trigger, or
- any action a button can have (a gamepad button, key, mouse button, macro, …), pressed once the trigger is pulled past a threshold.

Triggers can also have [zones](Buttons-and-Actions#zones).

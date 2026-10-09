# Layers

A layer changes some of a controller's mappings while it's on, then changes them back. For example: hold LB, and the face buttons type F1 to F4 and the right stick scrolls instead of moving the mouse. It works like Shift on a keyboard.

Layers belong to a game. They're on its **Layers** tab, and they apply over whichever of its profiles is active. Anything a layer doesn't set stays as in the profile.

A layer can override:

- buttons and stick directions,
- gestures,
- sticks and triggers (with their zones),
- gyro,
- combos (add combos, or switch the profile's combos off).

![The Guide layer on the Layers tab, with its bindings and the controller drawing](https://raw.githubusercontent.com/marcinkoza0922/padwight/main/docs/images/layers-guide.png)

## Turning a layer on

Map **Layer…** to any button, trigger, stick direction, zone, gesture or combo. The layer is on while that input is held.

Wrap it in **Toggle** and it stays on until pressed again. A menu item can also switch a layer on and off, because menu items can only toggle layers.

Layers can turn on further layers.

In the [in-game editor](Editing-Controls-in-Game), **Add layer** makes a layer, and **Hold a layer…** on a button's action list makes that button hold it.

## Layers or profiles

Use a layer when a game needs a different control scheme for part of play, such as driving or using a vehicle. Profiles stay separate from each other: a profile should be a complete scheme on its own, and switching between schemes inside a game is what layers are for.

## Several layers at once

Layers stack. Where two layers disagree, the one turned on most recently wins.

## Switching over

- An input that's down when a layer comes on or goes off keeps doing what it started until it's released.
- Sticks, triggers and gyro switch modes right away.
- Held layers end when the profile changes, or when an on-screen menu or keyboard opens. Toggled layers stay on within the game.

## Showing a layer

Each layer can show its name on screen while it's on, with its own position, size and colours. A layer can instead show one of the game's info overlays, such as a cheat sheet. Quick layers, such as "hold Y to lean", can show nothing at all.

The live controller drawing labels each input with the mapping of the layers that are on. See [Info overlays](Info-Overlays).

## Editing

The Layers tab uses the same profile editor as the Profiles tab, shown over one of the game's profiles.

- Inputs a layer doesn't set read **Same as Gameplay: …**. Click **Override** to set one.
- Overridden inputs have **Back to base** to undo that.

Layers use their profile's timings (combo window, tap window and so on).

**Copy from another game…** copies a layer along with the macros, menus and info overlays it uses. This is handy for sequels.

Shared items can't use layers, because layers always belong to one game.

The Guide button holds a default layer, also called Guide. See [Buttons and actions](Buttons-and-Actions#the-guide-button).

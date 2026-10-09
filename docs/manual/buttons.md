# Buttons

Every button in a profile has an action. Click the button's name to change it.

## What a button can do

- press a gamepad button (A can act as B, for example)
- press a key, or a key combination such as Left Ctrl + C
- press a mouse button, or turn the scroll wheel
- switch to the next profile, or do nothing with *Disabled*
- run a macro, open a menu, hold a layer, or show an overlay
- do several of these at once

## Toggle and Turbo

Toggle and Turbo change how an action behaves. Wrap an action in one or both.

- **Toggle**: press once to hold the action down, and press again to let go. Good for crouching or running.
- **Turbo**: repeats the action from 2 to 30 times a second while you hold the button.
- **Both**: Toggle, then Turbo, gives auto-fire that you can switch on and off.

## Combos

Buttons pressed together can make an input of their own, such as LB + RB. After the first button goes down, padwight waits a moment (60 ms by default) for the others before it decides the button was pressed alone.

A button set to *Disabled* works as a modifier and has no time limit.

## Gestures

A button can do something different on a double tap, a triple tap, or a long press. A single tap on a button with gestures fires once padwight is sure there's no second tap. If you hold the button past that moment, its normal action starts at once and continues until you let go.

> **Tip:** because padwight waits to see if another tap follows, a button with gestures responds a little later.

See also [Sticks and triggers](sticks.md) for zones, and [Layers](layers.md) for changing many buttons at once.

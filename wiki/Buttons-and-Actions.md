# Buttons and actions

Each button in a profile has an **action**. An action can be:

- a gamepad button,
- a key or key combo, such as `LEFTCTRL+C`,
- a mouse button,
- the scroll wheel (one notch per press, continuous while held),
- a [macro](Macros), [menu](Menus), [layer](Layers) or info overlay,
- **several of these at once** (Multi),
- **Disabled**.

Triggers and stick directions can take actions too. See [Sticks and triggers](Sticks-and-Triggers).

![The Buttons tab of a profile, with the controller drawing labelling each input with its action](https://raw.githubusercontent.com/marcinkoza0922/padwight/main/docs/images/profile-buttons.png)

## Toggle and Turbo

Any action can be wrapped in one of two modifiers:

- **Toggle**: press once to hold the action down, press again to release it. Use it for hold-to-crouch and similar games.
- **Turbo**: repeats the action 2 to 30 times a second while held. A macro inside a turbo instead plays every set number of milliseconds, never faster than the macro takes to play.

They combine. Toggle → Turbo is auto-fire you can switch on and off.

## Combos

Buttons pressed together act as their own input. For example, LB+RB can be one combo.

Combo members wait a short window (60 ms by default) for the rest of the combo before acting alone. A member whose own action is **Disabled** works as a modifier with no time limit.

## Gestures

Each button can have gestures:

- **double tap**,
- **triple tap**,
- **long press**.

The final tap of a sequence fires on press and holds, so "double-tap and hold" works.

A quick tap on a button with gestures fires once the gesture is decided. If you hold the button past the tap window, its own action presses right away and is held until release. A long press set on the button replaces that behaviour.

## Zones

A zone holds extra actions while a stick or trigger is within part of its travel. For example:

- Left Shift on a partial stick push gives walk with WASD, and a full push gives run.
- A half trigger pull and a full pull can do different things.

Zones are hidden for triggers that are on/off only, such as those on Switch pads.

## Changing buttons while you play

Press Guide and Start together to open the [in-game menu](In-Game-Menu). **Edit Controls** there changes the buttons of the game in front, with every action on this page. See [Editing controls in game](Editing-Controls-in-Game).

## The Guide button

Guide isn't mapped like the other buttons. It opens the Guide overlay, with its shortcuts (keyboard, numpad, media, screenshot, recording and force quit) and mouse mode, the same in every game. See [The Guide button](Guide-Button).

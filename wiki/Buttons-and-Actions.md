# Buttons and actions

Each button in a profile has an **action**. An action can be:

- a gamepad button,
- a key or key combo, such as `LEFTCTRL+C`,
- a mouse button,
- the scroll wheel (one notch per press, continuous while held),
- **next profile**,
- a [macro](Macros), [menu](Menus), [layer](Layers) or info overlay,
- **several of these at once** (Multi),
- **Disabled**.

Triggers and stick directions can take actions too. See [Sticks and triggers](Sticks-and-Triggers).

## Toggle and Turbo

Any action can be wrapped in one of two modifiers:

- **Toggle**: press once to hold the action down, press again to release it. Use it for hold-to-crouch and similar games.
- **Turbo**: repeats the action 2 to 30 times a second while held.

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

## The Guide button

Guide is special because Steam uses it. By default, Guide holds the **Guide layer**, so a plain press doesn't reach Steam or switch profiles. While it's held, the other buttons do system shortcuts:

| With Guide held | Action |
|---|---|
| Double tap Guide | Sends Guide on to Steam (opens the Steam overlay) |
| X | On-screen keyboard |
| Y | On-screen numpad |
| LB | Media controls |
| RB | Screenshot |
| L3 | Start or stop screen recording |
| RT | Left click |
| LT | Right click |
| Right stick | Mouse |
| B (held about 2 seconds) | Force quit the focused window |
| D-pad Right / Down / Left | Enter / Tab / Escape |
| D-pad Up | Next profile |

Anything else pressed with Guide held does nothing, so it never reaches the game underneath. You can edit the Guide layer per game on its Layers tab.

Force quit ends the focused window's process tree (SIGTERM, then SIGKILL after 3 seconds). It never touches the desktop, the compositor or the display server. A toast and rumble show while the 2-second hold counts, and letting go early cancels it.

The Screenshot action saves to `~/Pictures/Screenshots/<game>/`. Recording uses gpu-screen-recorder, saving to `~/Videos/Recordings/<game>/`. If gpu-screen-recorder isn't installed, the action shows a toast saying so.

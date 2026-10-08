# Macros

A macro is a named input sequence. A game's **Macros** tab holds them, each in its own collapsible card. **+ New macro** adds one at the top.

## Steps

Each step is one of:

- **Tap**: press, hold for N milliseconds, release.
- **Hold down**: press and keep held.
- **Release**: let go of something held.
- **Wait**: N milliseconds.
- **Move stick**: move a virtual-pad stick to a direction or a custom position. It stays there until the next stick step, and is recentred when the macro ends.

A step can press a key or key combo, a mouse button, the scroll wheel or a gamepad button.

**Insert motion…** adds a fighting-game motion, one frame apart: quarter circles, dragon punch and half circles, written facing right.

The step editor uses exact millisecond fields.

## Playing a macro

Map a macro with the **Macro…** action. There are two modes:

- **Play once**: each press plays the macro through to the end, even if you let go early.
- **Repeat while held**: loops until you let go, then stops at once.

Each pass of a macro ends by releasing anything it still holds.

To loop a macro without holding anything, wrap a repeating macro in **Toggle**.

## Other things to know

- Renaming a macro updates every mapping that uses it.
- Switching profiles stops any macros that are running.
- A macro that refers to a missing item is flagged, and it blocks saving until it's fixed.

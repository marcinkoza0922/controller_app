# Editing controls in game

Edit Controls, in the [in-game menu](In-Game-Menu) (Guide + Start), changes the controls of the game you're playing. It edits that game's own profiles, so other games and General aren't touched. Changes apply straight away; they're written to the config file when you save.

The editor is a set of pages. Each row is a button, a setting, or a way to add something. A row's chip, such as **A** or **LS**, is the controller button it belongs to. A row that adds something is tinted lime, and a row that removes or deletes something is tinted red (see [colours](In-Game-Menu#colours-and-glyphs)).

## The top page

- The 15 buttons, each with what it does now.
- **Left stick** and **Right stick**.
- **Left trigger** and **Right trigger**.
- **Gyro**.
- **Combos**.
- **Layers**.
- **Macros**.

## A button

Choosing a button opens its action list. The first two rows are the picker rows, then the actions:

- **Gamepad button…** sends another pad button, picked from the next page.
- **Gestures…** (buttons only) opens the double tap, triple tap and long press of that button. Each can be set, or **Not set**.
- **Disabled**, **Next profile**, **On-screen keyboard**, **Numpad**, **Screenshot**, **Start / stop recording**, **Media controls**, **Force quit (hold)**.
- **Left**, **Right**, **Middle**, **Back** and **Forward click**, and **Wheel up**, **down**, **left**, **right**.
- **Keys…** opens a picker of 26 common keys. Tick several to send them together, such as Ctrl and Esc. **Done** leaves the picker.
- **Hold a layer…**, **Play a macro…**, **Open a menu…**, **Show an info overlay…** and **Show a log overlay…** each list what the game has, and pick one.
- **Make it a toggle** or **Make it turbo (10 a second)** wraps the action. Once wrapped, the row reads **Stop toggling** or **Stop turbo**, and choosing it takes the wrapper off.

A wrapped action gets its own rows underneath: **Rate** for a turbo (5, 10, 15, 20 or 30 a second), and **Starts on** for a toggle.

Gestures and layer buttons also have **Not set**, which clears them.

## Sticks

- **Action**: Disabled, Gamepad stick, Mouse, or Scroll.
- **Speed**: for Mouse and Scroll. Left and right change it.
- **Invert Y**.
- **Deadzone**.
- **Zones**: see below.

## Triggers

- **Action**: Disabled, Gamepad trigger, or Button.
- **Acts as**: for a Button trigger, one of six pad buttons, A, B, Y, X, LB or RB.
- **Press at**: how far the trigger must be pulled, in steps of 0.05.
- **Zones**.

## Zones

A zone holds its action while the stick or trigger is pushed within its range. Its page has:

- **From** and **To**: the edges of the range, moved with left and right. The start can't pass the end.
- **Action**: the same choices as a button, including keys and holding a layer.
- **Delete zone**.

A new zone covers the top half of the travel.

## Gyro

- **Mode**: Off, or Mouse. Stick and steering modes can only be set in the pack file.
- **Sensitivity**: for Mouse. Left and right change it.
- **Invert X** and **Invert Y**.
- **Noise threshold**.
- **Activation**: cycles through always on, on while LT or RT is held, off while LT or RT is held, and toggled by LT or RT.

## Combos

- **Add combo** makes an empty combo.
- A combo's page ticks its buttons. A combo needs two or more.
- **Action** sets what the combo does, with the same choices as a button.
- **Delete combo** removes it.

## Layers

- **Add layer** makes a layer called **Layer 1**, **Layer 2** and so on. It does nothing until a button holds it: use **Hold a layer…** on a button's action list.
- A layer's page sets each button's action while the layer is held. Choose **Not set** to leave a button unbound by the layer.

## Macros

- **Add macro** makes an empty macro.
- A macro's steps are listed first. Choosing a step removes it.
- The rows after the steps add to the macro:
  - **Tap…**, **Press…** and **Release…** each pick a gamepad button.
  - **Stick…** adds a stick step: one of nine directions (eight, and centre), for either stick.
  - **Wait 100 ms** adds a pause.
  - **Remove last step** takes off the end.
- **Delete macro** removes the macro. It's refused while a button, layer or menu uses it, and the row says so.

## What the editor doesn't offer

These stay in the pack file:

- **Multi**, which runs several actions from one button.
- Stick and steering gyro modes, and the gyro's other inputs.
- Macro steps beyond taps, presses, releases, stick steps and waits, such as an action's repeat setting. A macro picked from the editor plays once.
- Keys outside the 26-key picker.

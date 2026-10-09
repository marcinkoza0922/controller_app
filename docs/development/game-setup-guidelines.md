# Game setup guidelines

Conventions for choosing where actions go when setting up a game's controller bindings, for pack authors and the built-in library. They're defaults, not rules: a game's own conventions or a clearer layout win.

## Movement and camera

- **WASD movement** should generally be assigned to the **left stick**. Strategy games that use WASD or the arrow keys to pan the camera can also assign that to the left stick.
- **Camera or cursor control** should almost always be assigned to the **right stick**. As a rule of thumb, if the mouse controls it, it goes on the right stick.
- **Walk or slow movement** should be bound to the **left stick** as a zone covering the first **50 to 70 percent** of the stick's travel, so a light push walks and a full push runs. The right value depends on the controller and game. The Deus Ex pack uses 60 percent (see [tutorial-deus-ex-pack.md](../tutorial-deus-ex-pack.md)), which works well enough as a starting point.
- **Sprint** should be bound to a **long press of A** with a short long-press window.

## Actions

- **Primary fire** should almost always be the **right trigger**.
- **Secondary fire** depends on the game. It can be the **left trigger**, or the **right bumper**.
- **Melee** depends on the game. **Right stick click** is generally acceptable, as is one of the **face buttons**.
- **Use** (bound to E, F, or sometimes Space) should be assigned to **A** or **X**.
- **Reload** should typically be assigned to **X** or **Y**. **Right Bumper** can also work.
- **Jump** should be assigned to **A** or **B**. If the game has a *Use* action, Use goes on **A** and Jump goes on **B**.
- **Crouch** should almost always be a **toggle**. It should favour **left stick click**. If **B** is free, it may be preferred there instead.
- **Zoom / aim**, if not on the right mouse button, should be on **right stick press** or **D-pad Up**.
- **Mirrored** actions, such as switching to the next or previous item, or rotating the camera, should typically be on **D-pad Left/Right** or the **shoulder buttons**.
- **Lean** can go on **D-pad Left/Right**. An acceptable alternative is a layer on a face button or shoulder button, where the **left stick** acts as the lean while the layer is held.
- **Scroll** (mouse wheel) can go on the **D-pad** or the **shoulder buttons**.
- **Multiple use actions**, such as open and pick up, should share the same button but use different gestures. For example, if pick up is a long press of **A**, open is a short press of **A**.

## Hotkeys and menus

- If a game uses the number keys or function keys as hotkeys, assign them to a **radial menu** with every relevant hotkey as an item. Prefer putting the menu on one of the **shoulder buttons**.
- If there is a **holster** or **put away** action, assign it to the same input that took the item out, with a **long press**. For example, if a shoulder button brings up a radial menu to take items out, a long press of that shoulder button holsters them.
- Whatever is on the **right mouse button** should go on the **left trigger**. The exception is when the right mouse button is a *Use* action, in which case **A** or **X** is preferred.
- **In-game menus** in PC games without controller support usually only work with the mouse. Some accept the arrow keys. Supporting arrow keys is often hard without a separate profile or layer, which can be unintuitive, but it may still be an acceptable option.

## Vehicles

- Vehicles should work out of the box if the game supports them. If they don't, they probably need **separate toggled layers**, which the player switches on and off themselves.

## System buttons

- **Back / Select / View** should typically open the in-game menu, often **Tab**. If the game has separate keys for the journal, inventory, map, and so on, Select should open a list menu with all of them.
- **Start** should be the system menu, usually **Escape**. If the system menu fails to pause the game, a **triple tap of Start** can act as pause instead. This is rare.
- **Quick save** should be a **double tap of Start**. **Quick load** should be a **long press of Start**.
- **Guide** should always show its own overlay in the corner with pseudo-global options. It should also show an info overlay explaining which keys are bound to what, using glyphs.

## Presentation

- Keyboard, numpad, and menu overlays should be coloured and styled to match the game's theme as closely as possible.
- Animations and sounds should be assigned to match the game as well.

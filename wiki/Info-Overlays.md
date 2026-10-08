# Info overlays

Info overlays put text on screen without taking control of the controller. They're mainly for showing a game's controls. A game's **Info overlays** tab holds them; shared overlays are available to every game.

Each overlay is a grid of cells, with its own position, size and colours. Rows of cells line up in columns.

## Tokens

Cells can contain tokens, written in braces.

**Button glyphs**, drawn the way the controller in use labels them. `{south}` is A on Xbox, ✕ on PlayStation and B on Nintendo.

`{south}` `{east}` `{west}` `{north}` `{lb}` `{rb}` `{lt}` `{rt}` `{select}` `{start}` `{guide}` `{ls}` `{rs}` `{l3}` `{r3}` `{dpad}` `{up}` `{down}` `{left}` `{right}`

Controllers that aren't recognized get the fallback glyph style chosen on the Settings page.

**Live values**, updated every second:

`{time}` `{time12}` `{date}` `{profile}` `{layer}` (the layers that are on, such as "Hotkeys + Build") `{app}` (the focused program's executable) `{title}` `{pid}` `{cpu}` `{ram}` `{gpu}` (AMD only) `{wifi}` (signal strength, with its icon) `{system_battery}` (a laptop's or handheld's charge) `{controller_battery}` (the charge of the controller in use, where it reports one; `~` when estimated from a coarse level)

**Icons**:

- `{pc}`: a monitor, or a laptop or handheld shape.
- `{controller}`: the controller in use, drawn for its kind. Put it beside a battery.
- Add `:icon` to `{system_battery}` or `{controller_battery}` to show a gauge instead of the number.
- `{controller:name}` shows the controller's name.

## When an overlay is shown

- **Always**, while its game is active.
- **At game start**, for a few seconds: the first time the game is focused after launch, for example a "config loaded" note.
- **On an input**, with the **Show info overlay…** action while the input is held. Wrap it in Toggle to keep it up.
- **On game start, until dismissed**: map "Toggle → Show info overlay…" to a button and tick **On when the game starts**. It shows at launch and goes when the button is pressed.

An always-shown overlay can't also be mapped to an action. For an overlay that should appear only sometimes, use an action, typically a Toggle.

An overlay shown by an action can **linger** for a few seconds after it's released or toggled off. Timed overlays fade out at the end.

Overlays at the same screen position stack.

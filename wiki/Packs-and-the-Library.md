# Packs and the library

A game can be shared as a **pack**: a single `.padpack` file (TOML) holding the game's profiles, macros, menus, info overlays and layers, plus any shared items it uses.

For a step-by-step walkthrough that builds a pack from scratch, see [Tutorial: making a pack](Tutorial-Deus-Ex-Pack).

The format itself is documented in the repository: [pack format reference](https://github.com/marcinkoza0922/padwight/blob/main/docs/pack-format.md). A step-by-step example is in the [Deus Ex pack tutorial](https://github.com/marcinkoza0922/padwight/blob/main/docs/tutorial-deus-ex-pack.md).

## Exporting

**Details → Export…** on a game writes a `.padpack` file. It copies in any shared items the game uses. The export dialog warns about references to missing items.

Each profile declares what it needs beyond a plain controller, today only gyro. Tick **Can't be played without gyro** on the profile's Gyro tab when the profile depends on it. Nothing is detected; the export dialog only lists what you've declared.

Re-exporting your own game keeps its pack ID. Exporting someone else's game makes a fork with its own ID, which credits the original.

## Importing

**+ Add game → Import a file…** opens a preview before anything changes. The preview shows:

- what's inside the pack,
- what your controller lacks,
- a game with the same name: add under a new name, or replace yours,
- shared items that will be renamed to avoid clashes,
- other games' rules for the same window. The imported game takes over unless you keep yours.

Nothing changes until **Save & apply**.

Imported games are ordinary, editable games. They remember their pack, so:

- importing a newer version of the same pack, or
- an app update that ships a newer library version (marked **update** in the sidebar)

offers to update the game, and lists what you've changed since.

A pack made by a newer version of padwight is refused, with a request to update.

Players whose controller lacks a feature are only offered the profiles that don't need it.

## The built-in library

**+ Add game** also lists the built-in library. Games you have installed in Steam, Heroic or Lutris, or that are running, come first.

The library is the `packs/*.padpack` files in the repository, embedded at build time. See [`packs/README.md`](https://github.com/marcinkoza0922/padwight/blob/main/packs/README.md). Adding a file to `packs/` is enough for it to appear in the picker, as long as it passes the checks that `cargo test` runs.

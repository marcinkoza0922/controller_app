# Games, packs and the built-in library

Status: draft spec, from interviews on 2026-10-06. Nothing here is implemented yet.

## Goals

1. **Organize around games.** Each game holds its own profiles, macros, menus, info overlays and
   auto-switch rules, so supporting many games doesn't turn into endless flat lists.
2. **Share games as packs.** Export a game to a file; import games others made.
3. **Ship a library of pre-baked games** for PC titles without proper controller support,
   authored by the maintainer in the app itself.

Breaking the config format is fine: there are no users to migrate yet.

## Decisions at a glance

| Topic | Decision |
|---|---|
| Main concept | A **game** is a container: name, rules, profiles, macros, menus, info overlays. |
| Game vs pack | The same thing. Exporting a game makes a pack; importing a pack makes a game. |
| Non-game profiles | Live in **General**, a built-in game without rules (Desktop, Gamepad passthrough). |
| Reuse across games | A global **Shared** section for items every profile can use; otherwise copy between games. |
| Name uniqueness | Per game. Game items also can't reuse a Shared item's name. |
| Scope inside a game | The whole game: every profile in a game can use all its macros, menus and info overlays. Nothing is scoped per profile. |
| Navigation | Sidebar: Overview, Settings, General, the games (with search), "+ Add game". |
| Game page | Sub-tabs: Profiles · Macros · Menus · Info overlays · Details. |
| Sharing channel | Files only (`.padpack`, TOML). No server. |
| Import | Preview, then confirm. |
| Built-in library | `packs/*.padpack` in the repo, embedded in the binary. Adding one gives an editable copy that tracks its origin. |
| Library picker | Search, installed games marked (Steam, Heroic, Lutris, running processes), preview, "Empty game", "Import file". |
| Updates | Recognized by pack ID; offer to update, warning about local edits. |
| Re-export of someone else's pack | A fork: new ID, exporter as author, "based on" credit. |
| Controller features | Baseline is a generic XInput pad. Exporters are warned when they go beyond it; importers when their pad can't do something. Neither blocks. |
| Format versioning | `format` number; older apps refuse newer packs with a clear message. |

---

## Part 1: Game-centric app

### Data model

```text
Config
├── app-wide settings (enabled, devices, keyboard/numpad style, fallback glyphs, auto-switch on/off)
├── general: Game        built in, can't be deleted or exported, has no rules
├── shared               macros, menus, info overlays usable by every profile in every game
└── games: [Game]

Game
├── name                 unique among games
├── origin?              pack metadata if it came from a pack or the library (see Part 2)
├── rules                auto-switch rules; each points at one of this game's profiles
├── profiles             names unique within the game
├── macros               usable by every profile in the game
├── menus                usable by every profile in the game
├── info overlays        usable by every profile in the game
│     └── always: bool         shown whenever any of the game's profiles is active
└── export metadata      author, version, description, made with (pre-fills the next export)
```

**Name resolution.** A reference (`Macro`, `OpenMenu`, `ShowInfo`, a menu's submenu) from a
game's profile or item looks first in that game, then in Shared. Shared items can only reference other Shared items.

**Names.** Unique per kind within a game, so "Dodge" can exist in Elden Ring and Sekiro.
A game item may not use a Shared item's name: it's a validation error shown on the game item.
That keeps one meaning per name, with no overriding.

**Scope inside a game.** Macros, menus and info overlays belong to the game as a whole: every
profile in the game can use them, and nothing is scoped per profile. An info overlay set to
**"always show"** is shown whenever any of the game's profiles is active; otherwise it shows
only while a "Show info overlay" action holds it up.

**Profiles across games.**
- The active profile is identified by (game, profile).
- `NextProfile` cycles through the current game's profiles.
- The auto-switch default profile (for windows no rule matches) points at a (game, profile),
  normally one in General.

### Navigation

A left **sidebar**:

1. **Overview**: live devices and status (today's Overview tab).
2. **Settings**: keyboard and numpad overlay style, fallback glyphs, auto-switch on/off and
   default profile, ignored devices.
3. **General**: pinned. Its page has the same sub-tabs as a game, plus the **Shared** items.
   It has no Details actions such as export or delete.
4. **Games**: alphabetical, with a search field above. Games with an available library
   update show a badge.
5. **+ Add game**: opens the library picker (Part 3).

### Game page

Sub-tabs:

- **Profiles**: a picker for the game's profiles, with new, duplicate, rename and delete, and
  today's profile editor (Buttons, Sticks, Combos, Gyro) for the picked one.
- **Macros**, **Menus**, **Info overlays**: all of the game's macros, menus or info overlays.
  An info overlay's editor has an "Always show while this game is active" checkbox.
- On those three:
  - A collapsed **Shared** section at the bottom lists the Shared items the game can also use,
    read-only, with a link to edit them under General.
  - **"Browse other games…"** opens a read-only view of other games' items (including
    library games not yet added) with previews. **"Copy here"** makes an independent copy in
    this game, renamed if the name is taken.
- **Details**: name, auto-switch rules, pack metadata (author, version, description, made
  with), origin and update status, **Export…**, **Delete game**.

Each list holds one game's items, which keeps lists short. Today's global
"Show:" filter and the keyboard and numpad cards leave the per-game pages.

---

## Part 2: Packs (import and export)

A pack is a game written to a file.

### What a pack contains

- The game's name, rules, profiles, macros, menus and info overlays.
- **Shared items the game references**, followed transitively through button actions,
  gestures, combos, nested actions (`Multi`, `Toggle`, `Turbo`), menu items and submenus.
  They're written into the pack as game items, so the pack works on its own. Shared items
  that nothing references aren't included.
- Pack metadata (below).

**Never exported** (they belong to the user or device, not the game): app-wide settings,
`gyro_calibration`, General, and unreferenced Shared items.

Dangling references (e.g. a button set to a macro that no longer exists) are listed as a warning
in the export dialog. The export goes ahead; the reference stays as it is.

### File format

TOML in the same shapes as the config, extension **`.padpack`**.

```toml
format = 1                       # pack format version, see "Versioning"

[pack]
id = "6f1c9e0a-…"                # UUID, generated on first export, stable across versions
name = "Elden Ring"              # becomes the game name
version = "1.2"
author = "someone"
description = """
Gyro aim while holding LT. Back of the menu is on Select.
"""
made_with = "DualSense"          # free text, informational
requires = ["gyro"]              # computed on export, see "Controller features"

[pack.based_on]                  # only on forks
id = "…"
name = "Elden Ring"
author = "someone else"
version = "1.1"

[[rules]]
kind = "steam_app_id"
value = "1245620"
profile = "Gameplay"

[[profiles]]
name = "Gameplay"
# … same as a profile in the config

[[macros]]
name = "Dodge"
# …

[[menus]]
# …

[[info_overlays]]
name = "Controls"
always = true
# …
```

Game IDs aren't stored separately: they're the `rules` (executable, Steam App ID, window class),
and previews and the library show them from there.

### Export

From the game's **Details → Export…**:

1. **Review contents**: the game's items, the Shared items pulled in, and any dangling
   references.
2. **Controller check** (below): non-blocking warnings.
3. **Metadata**: pre-filled from the game's export metadata, with the version field
   highlighted to bump.
4. **Save** to a `.padpack` file.

Pack ID:
- The first export of a game the user made generates a new UUID, stored in the game's origin.
- Re-exporting it keeps the ID: it's a new version of the user's pack.
- Re-exporting a game that came from someone else's pack, or from the library, makes a
  **fork**: a new UUID, the exporter as author, and `based_on` from the original. A fork is
  never offered as an update to the original.
- Exception: the maintainer exporting into `packs/` keeps library IDs (see Part 3).

### Controller features (XInput baseline)

A generic XInput pad is the minimum a pack can expect: South, East, West, North, both
bumpers, both analog triggers, Select, Start, Guide, both sticks with clicks, and the D-pad.
Anything beyond that is a **feature**:

- `gyro`: any profile has gyro enabled. Today this is the only input the app supports beyond
  the baseline.
- Future inputs go here as the app adds them: back paddles, touchpad, extra buttons.

**On export**, the dialog lists each feature used and where ("Gyro aim in profile
*Gameplay*") and says players on a plain XInput pad won't get it. It doesn't block.
`requires` is computed from this; authors don't type it.

**On import** (and in the library preview), `requires` is compared with the connected
controllers. Anything missing is listed as a warning ("Your controller has no gyro: gyro aim
won't work"). The import goes ahead regardless, and those bindings never fire. Adapting a pack
to a simpler controller is up to the user. With no controller connected, requirements are
listed without a verdict.

### Import

From the library picker's **Import file…** (and later, opening a `.padpack` with the app).
Nothing changes until the user confirms the preview.

The **preview** shows:

- Name, version, author, description, game IDs, made with, and "based on" for forks.
- Profiles, macros, menus, info overlays and auto-switch rules it will add.
- Controller warnings.
- **Game name clash**: a game with this name already exists, from a different pack or none.
  **Rename** (default, e.g. `Elden Ring (2)`) or **Replace**.
- **Shared name clashes**: a pack item named like one of the user's Shared items. The
  imported item is renamed (`Dodge (2)`) and its references are rewritten to match.
- **Rule clashes**: an imported rule with the same kind and value as a rule in another game.
  By default the imported rule takes over and the other game's rule is disabled, since the
  user is importing a setup for that game. They can keep their own rule instead.
- If the pack ID is already installed: an update notice (see "Updates").

On **Import**, the game is added with `origin` set and a hash of each item's content, so local
edits can be detected later. Its rules are active. The change goes into the GUI's unsaved
config, like other edits; "Save & apply" commits it and "Revert" backs out.

### Updates

Importing a pack whose ID matches an installed game's origin, or the library shipping a
newer version (Part 3), offers an update:

- Show the old and new versions. When the incoming one is the same or older, offer it as
  "Reinstall" or "Downgrade" rather than "Update".
- List the items the user has edited since import (hash differs), with a warning that the
  update replaces them.
- On confirm, the game's contents are replaced by the new version's. The game's name and
  the user's choices about rule clashes are kept.

### Deleting a game

**Details → Delete game** removes the game's profiles, rules and items, after a confirmation
that lists them. Nothing outside the game references its items, so nothing else breaks. If
the active profile was in it, switch to the auto-switch default profile, or else General's
first profile.

### Versioning

- `format` is an integer, starting at 1, and changes only for breaking changes to the pack's
  shape.
- An app reading a pack with a **newer** `format` refuses it: "This pack was made with a
  newer version of the app. Update to import it."
- A newer app reads **older** formats by upgrading them on load.
- Within a `format`, packs are parsed strictly: a malformed pack fails with the parse error and
  line rather than half-importing.

### Safety

Packs come from strangers. Today every action is input-only (keys, mouse, gamepad, the app's own
overlays and menus); nothing runs programs or touches files, so importing is safe. If an action
that runs commands or opens files is ever added, the import preview must highlight it
prominently, and it should be off by default for imported packs.

---

## Part 3: Built-in library

### Source and build

- Pre-baked games live in the repo as `packs/<game>.padpack`.
- The build embeds them in the binary, so they need no installation and work in every
  packaging (see `dist/`).
- **Authoring flow:** the maintainer builds the game in the app and exports it into `packs/`.
  Exporting a game whose origin is a library pack, into `packs/`, keeps its ID (a new version
  rather than a fork). The export dialog offers this as "Save as library pack" in debug or
  developer builds.
- **Tests** run over every file in `packs/`:
  - parses at the current `format`;
  - pack IDs and game names are unique across the library;
  - no dangling references;
  - every profile named by a rule exists;
  - `requires` matches what the contents actually use;
  - has at least one auto-switch rule.

### Library picker ("+ Add game")

A dialog with a search field and a list of entries:

- **Empty game**: a new game with one profile, made from a template as today.
- **Import file…**: a `.padpack` from disk (Part 2).
- **Library games**: name, short description, requirement badges (e.g. gyro), and:
  - **Installed** markers, sorted to the top, from:
    - **Steam**: library folders and app manifests, matched against `steam_app_id` rules;
    - **Heroic** (Epic and GOG): installed-games lists, matched against executable rules;
    - **Lutris**: its game database, matched against executable rules;
    - **running processes**: the daemon's process scan, matched against executable and
      window-class rules.
  - **Already added** for games whose origin ID is installed (opens the game instead).

Picking a library game shows the same **preview** as an import (contents, requirements,
description). **Add** creates an editable copy with `origin` pointing at the library pack.

### Library updates

When an app update ships a library pack with a newer version than an installed game's
origin, that game gets a badge in the sidebar and an **Update** button in Details, which
runs the normal update flow (warning about local edits). Updates are never applied without
the user's say.

---

## Open questions

- **Guide button in the baseline.** It's part of XInput, but Steam and some desktops capture
  it. Should using Guide trigger the export warning?
- **Updates over local edits.** Should an update offer "Keep my edited version as a copy"
  instead of only warning?
- **Pack version comparison.** Free-form strings make "older/newer" a guess. Require
  `major.minor[.patch]`?
- **Rules for several profiles.** A game whose profiles need different rules (e.g. a
  launcher window versus the game itself) works, since each rule names its profile. Is a
  game-level "launch profile" also needed for when no rule's details are known?
- **Shared names.** Making a new Shared item whose name an existing game item already uses
  turns the game item into an error. Should creating the Shared item be blocked instead?
- **Library ↔ running game.** When a running game matches a library pack the user hasn't
  added, should the app suggest adding it (a notification), or only mark it in the picker?
- **File association** for `.padpack` (double-click to import) depends on packaging.

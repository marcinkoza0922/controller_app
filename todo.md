# GUI polish

From a click-through of the GUI on the light theme (dark mode not checked).

## Clarity

- [x] **Enabled grey buttons look disabled.** `button::secondary` (53 uses) is a flat mid-grey with dark text, so "Duplicate", "Find by pressing", "+ Add game", "Test rumble" and "+ Rule" look like the disabled "Revert". Give it a lighter fill with a border, like the dropdowns in `src/style.rs`.
- [x] **Labels on the controller picture cross each other.** The right column in `src/pad_svg.rs` (`RIGHT_LABELS`) lists West before East and Start after South, and `place_labels` stacks labels in list order, so the X, Start and right-stick lines cross other buttons. Sort each column by the anchor's y.
- [ ] **Long labels wrap on the controller picture.** "Menu “Augmentat…" wraps to two lines and runs into "Next profile". `place_labels` counts characters, but the box is narrower than that. Cut shorter or stop the box wrapping.
- [ ] **"—" for a button that does nothing is cryptic**, especially "— +" on the picture and "— · Double tap: …" in summaries (`actions.rs`, `profile.rs`, `items.rs`). Use "Disabled", or "(nothing)" on the picture.
- [ ] **Single-character keys are hard to spot** in button summaries (`;`, `]`, `'`). Show key names as keycap chips or in quotes. Also write "Keypad PLUS" / "Keypad SLASH" in normal case.
- [ ] **Remapped buttons don't stand out from unchanged ones.** Rows that just pass the button through ("Pad A" on A) look like real changes. Dim them.

## Polish

- [ ] **A red "Delete" on every closed card** (menus, macros, info overlays: `src/gui/items.rs`). Move it inside the open card, or make it a quiet ✕ that turns red on hover.
- [x] **"Recently focused" list formatting gaps** on the Details tab (`src/gui/games.rs`): a window with no title shows only its grey details line, an empty executable leaves a leading " · class …", and the app lists its own window.
- [ ] **Empty tabs aren't consistent.** Layers has a bare button plus "This game has no layers."; Macros has a card with a one-line explanation and no empty-state text. Pick one pattern.
- [ ] **The header and controller picture take a lot of height.** The title, remapping switch and profile picker repeat on every page, and the full controller picture (~330 px) sits above the button editor (and is also on Overview). Allow collapsing the picture on the Profiles tab.
- [ ] **The Save & apply bar has its own left border** next to the sidebar's divider, making a double line. Start it flush with the content area.

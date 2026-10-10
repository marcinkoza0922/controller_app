# Settings window polish

A list of visual inconsistencies, polish points and improvements found by going through every
page of the settings window (Overview, App settings, Manual, a setup's tabs and profile
sub-tabs, the add, import, export and delete dialogs, and dark mode). Work through it one item
at a time, with a before and after screenshot for each. Screenshots should fall in an untracked directory inside the repository.

Tick an item when it's done and merged.

## Broken layout

- [x] 1. Rows overflow their cards on the Sticks tab, Macros and Menus. Wide action editors ran
  off the card edge into a sideways scrollbar, and the right-hand controls were cut off. Editors
  now wrap onto a second line instead.
- [x] 2. The Mapping guide's columns don't line up. The D-pad row's text field is wider than the
  others, the "Lean" row has an "Add an input" list and a blue "Split" button, and the hold and
  ×2 rows have no "Share with" column. The Show and controls columns now have fixed widths on
  every row, and Split and Remove are grey buttons of one size.
- [x] 3. The "daemon not running" note on the Overview prints the backticks around
  `systemctl --user start padwight` and `padwight daemon` (`src/gui.rs`). The commands are now
  set in monospace instead.
- [x] 4. The Guide tab shows the raw token `{keyboard:apostrophe}` as the label for "X hold".
  A row's field now holds only text the author wrote, and the default shows as plain words in
  its placeholder ("Press “'”"), since a text field can't draw glyph tokens.

## Contradictory or misleading states

- [x] 5. With the daemon down, Overview says "No profile has switched since the daemon started."
  It now says "Needs the daemon.", like the two feeds above it.
- [x] 6. The "Remapping enabled" toggle is greyed out while the daemon is down, so it looks off,
  but its label still says "enabled". It now reads "Remapping on" or "Remapping off", and is off
  while the daemon is down, since nothing is remapped then.
- [x] 7. "Revert" looks clickable when there is nothing to revert, while "Save & apply" is greyed
  out next to it. Every disabled button now has one look, whatever its style: no fill, a faint
  edge and grey text.
- [x] 8. The disabled "Save & apply" is a pale blue that is hard to read, and "Find by pressing"
  barely looks different when disabled. Both use the disabled look from item 7, which keeps the
  text readable.
- [x] 9. The sidebar says "No setups yet. Use + Add setup…" while General is listed just above
  it. The search box shows even when there is nothing to search. It now says "No game setups
  yet", and the search box shows only once there is a game setup.
- [x] 10. On a profile with no bindings, the controller picture shows a lone "Layer "Guide""
  callout and nothing else. Guide's usual mapping (which the engine ignores) is no longer drawn,
  and a picture with no callouts says "Every input passes through unchanged" under it.

## The same thing worded differently

- [x] 11. "Needs the daemon" has four wordings in two sizes: "Live input needs the daemon",
  "Needs the daemon.", "Unavailable while the daemon is not running." and "Automatic switching
  needs the daemon to be running." The Controllers one is in a larger font. All four now say
  "Needs the daemon." in small grey text.
- [x] 12. An unbound input is "(nothing)" in the picture callouts but "Disabled" in the button
  list (`src/gui/profile.rs`). The callouts say "Disabled" too.
- [x] 13. The same key is written three ways: "SPACE", "Space" and "“;”". The key field now
  shows the keyboard names the summaries use ("Space", "Left Ctrl+C") and takes them typed in,
  as well as the old code names. Arrows read "Up", "Left" and so on, and keys whose name has a
  "+" (Vol +) keep their code name, so the field can be typed back. Summaries still quote a lone
  punctuation key (“;”) so it stays visible in a sentence.
- [x] 14. Button names follow different patterns: "A (South)", "Left Bumper (LB)", "Back (View)",
  "Left Stick Click", then "L3" and "R3" on the Guide tab and "Stick clicks" in log overlays.
  Names follow "Name (abbreviation)": "Left Stick Click (L3)", and "Stick clicks (L3, R3)" in log
  overlays. Glyphs stay short, like every other glyph.
- [x] 15. Stick names are capitalised differently: "Left Stick" in lists, "right stick" in hints.
  Sticks, bumpers, triggers and D-pad directions are capitalized everywhere in the window, the
  overlays, the controller editor and the manual.
- [x] 16. The app name has two casings: "Padwight" in the window title and page header,
  "padwight" in the manual and in "by padwight" and "Exported by padwight". It is "Padwight" in
  prose, the library packs' author included, and `padwight` only as a command.
- [x] 17. The settings page has two names: "Same as Settings" on the Details tab, and
  "App settings" in the sidebar and hints. It is "App settings" everywhere.
- [x] 18. British and American spelling are mixed: "Menu colours" and "Colour-blind mode" in the
  UI, "color" in `src/gui/widgets.rs` and `src/gui/tab_icons.rs`. Everything is American now
  (color, center, gray, labeled), in the UI, the docs and the code. The config file keeps its
  `colourblind_tones` key, so existing files still load.
- [x] 19. Unit spacing varies: "250 ms" on sliders, "40ms" in the log preview, and "0 ° from up"
  with a space before the degree sign. Units take a space ("40 ms") and the degree sign doesn't
  ("0° from up", "120°/s").

## Controls that look or behave differently for the same job

- [x] 20. "Use its own appearance in this setup" is a checkbox on four cards on the Details tab,
  while Button layout is a segmented control, Overlay font a dropdown, and Overlay sounds a full
  editor.
- [x] 21. Toggles and checkboxes are mixed for on/off settings: toggles for "Controllers this
  setup supports" and Overlay sounds, checkboxes for "Can't be played without…" and rule enables.
- [x] 22. Button sizes vary: "Make active", "Hide", "Expand all" and "Collapse all" are small or
  unstyled, while "Duplicate", "Find by pressing" and "+ New macro" are full size.
- [x] 23. The top of each items tab differs: Layers has a bare "+ New layer" button, Macros, Info
  and Log overlays have a card with a description, and Menus has an "Add a menu:" row. The
  "Copy from another setup…" link sits in a different place on each.
- [x] 24. Disclosure triangles come in two sizes: a large ▾ on item cards and a small ▸ on
  "Appearance" and "Shared … usable here (0)".
- [x] 25. "Remove" is shown three ways: a small red underlined link under a gesture label, "×"
  buttons on rules, cells and macros, and "×" on the card corner to delete a whole item.
- [x] 26. Some cards lack the ⓘ icon. "Controller requirements" and the four appearance cards on
  Details have none, and every other card has one.
- [x] 27. "Expand all" and "Collapse all" appear on tabs with nothing to expand (Combos, Guide)
  but are hidden on Gyro.
- [x] 28. On the Details tab, "Name" sits outside any card, while everything else is in one.
- [x] 29. The profile dropdown and the rename field next to it are both unlabelled and both
  read "Deus Ex".
- [x] 30. Mapping summaries use colours that look like errors or links with no legend: "Menu" in
  red, "Toggle" in green, "Right click" in teal.

## Spacing and density

- [x] 31. The header (title, remapping toggle, active profile, daemon note) repeats on every page,
  including the Manual. With the daemon down it pushes content about 230 px down. The header is
  now one row (the window title already names the app), with "○ Daemon not running" beside the
  switch it explains. The note on starting the daemon shows on the Overview only, and the Manual
  has no header.
- [x] 32. The radial menu preview is about 750 × 800 px and pushes the item list well below the
  fold. Menu previews are drawn at 80%, and sit after the items, or under the Appearance editor
  while it's open, so each change shows.
- [x] 33. Overlay sounds is a long wall of small sliders: four sounds with three sliders each, for
  every overlay type, repeated on App settings and on every setup's Details tab. Each overlay is
  a switch with an "Adjust" disclosure, closed at first, and each sound is one line: kind, Play,
  volume, pitch and length.
- [x] 34. Sound labels mix case ("Step", "Pick" against "volume", "pitch", "length") and use a
  smaller font. They're all capitalized and at the body size; only the percentages are small.
- [x] 35. Long explanations are in very small grey text: the controller-requirements note, the
  "Shown" note on info overlays, and the export notes. They use a new `note` style: 14 px, in the
  theme's text color softened, rather than the faint caption grey.
- [x] 36. Dropdowns have different widths on the same page: 280, 200 and 160 px on App settings.
  Settings dropdowns and the header's share one width (`SETTING_WIDTH`, 260 px).
- [x] 37. In the Add a setup dialog the "installed" tags touch the scrollbar, and the right pane
  is mostly empty. The list leaves a gap for the scrollbar, and the first entry is shown until
  another is picked.
- [x] 38. In the Add and Export dialogs the item counts include zeros ("0 layers · 0 macros …")
  and wrap so that "overlays" sits alone on a line. Kinds with none are left out, and each count
  is held together by no-break spaces.
- [x] 39. In the Export dialog the Description field is one line and cuts off its text, the
  Author placeholder repeats its label, and "Library pack: keep the library's ID" uses an
  internal concept. The description is a box that wraps, the placeholders are examples ("Your
  name"), and the box reads "Release it as the built-in Deus Ex: people who added the built-in
  one get it as an update".

## Dark mode

- [x] 40. With the daemon down, the controller picture is barely visible against the background,
  and its callout lines disappear. The leader lines are a layer of their own, never dimmed, and
  a little stronger. The controller is dimmed less in dark mode, since it's drawn dark.
- [x] 41. The selected tab is only slightly darker than the tab bar, so it is hard to see which
  tab is active. The selected segment is lighter than the bar in both themes, with an accent
  edge.
- [x] 42. The menu and overlay previews are always dark navy: fine in dark mode, but a heavy slab
  in light mode. The "screen" behind a preview follows the theme; the overlay itself keeps its
  real colors.

## Picture callouts

- [x] 43. Leader lines cross each other around the face buttons and the right stick. Each label
  sits as near its part's height as the rows allow, and labels whose lines cross swap places
  until none do (a test checks every input on each layout).
- [x] 44. Long labels are cut on the right ("Menu "Augme…") but not on the left
  ("Menu "Belt" +"). The label columns are wider, so most names fit, and a name that's still
  cut keeps its closing quote ("Menu “Augmentat…”").
- [x] 45. A trailing "+" ("Esc +", "(nothing) +") means there is more, but nothing explains it.
  When a callout has one, a line under the picture says what it means.

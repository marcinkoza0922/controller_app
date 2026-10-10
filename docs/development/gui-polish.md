# Settings window polish

A list of visual inconsistencies, polish points and improvements found by going through every
page of the settings window (Overview, App settings, Manual, a setup's tabs and profile
sub-tabs, the add, import, export and delete dialogs, and dark mode). Work through it one item
at a time, with a before and after screenshot for each.

Tick an item when it's done and merged.

## Broken layout

- [x] 1. Rows overflow their cards on the Sticks tab, Macros and Menus. Wide action editors ran
  off the card edge into a sideways scrollbar, and the right-hand controls were cut off. Editors
  now wrap onto a second line instead.
- [ ] 2. The Mapping guide's columns don't line up. The D-pad row's text field is wider than the
  others, the "Lean" row has an "Add an input" list and a blue "Split" button, and the hold and
  ×2 rows have no "Share with" column.
- [ ] 3. The "daemon not running" note on the Overview prints the backticks around
  `systemctl --user start padwight` and `padwight daemon` (`src/gui.rs`).
- [ ] 4. The Guide tab shows the raw token `{keyboard:apostrophe}` as the label for "X hold".

## Contradictory or misleading states

- [ ] 5. With the daemon down, Overview says "No profile has switched since the daemon started."
- [ ] 6. The "Remapping enabled" toggle is greyed out while the daemon is down, so it looks off,
  but its label still says "enabled".
- [ ] 7. "Revert" looks clickable when there is nothing to revert, while "Save & apply" is greyed
  out next to it.
- [ ] 8. The disabled "Save & apply" is a pale blue that is hard to read, and "Find by pressing"
  barely looks different when disabled.
- [ ] 9. The sidebar says "No setups yet. Use + Add setup…" while General is listed just above
  it. The search box shows even when there is nothing to search.
- [ ] 10. On a profile with no bindings, the controller picture shows a lone "Layer "Guide""
  callout and nothing else.

## The same thing worded differently

- [ ] 11. "Needs the daemon" has four wordings in two sizes: "Live input needs the daemon",
  "Needs the daemon.", "Unavailable while the daemon is not running." and "Automatic switching
  needs the daemon to be running." The Controllers one is in a larger font.
- [ ] 12. An unbound input is "(nothing)" in the picture callouts but "Disabled" in the button
  list (`src/gui/profile.rs`).
- [ ] 13. The same key is written three ways: "SPACE", "Space" and "“;”".
- [ ] 14. Button names follow different patterns: "A (South)", "Left Bumper (LB)", "Back (View)",
  "Left Stick Click", then "L3" and "R3" on the Guide tab and "Stick clicks" in log overlays.
- [ ] 15. Stick names are capitalised differently: "Left Stick" in lists, "right stick" in hints.
- [ ] 16. The app name has two casings: "Padwight" in the window title and page header,
  "padwight" in the manual and in "by padwight" and "Exported by padwight".
- [ ] 17. The settings page has two names: "Same as Settings" on the Details tab, and
  "App settings" in the sidebar and hints.
- [ ] 18. British and American spelling are mixed: "Menu colours" and "Colour-blind mode" in the
  UI, "color" in `src/gui/widgets.rs` and `src/gui/tab_icons.rs`.
- [ ] 19. Unit spacing varies: "250 ms" on sliders, "40ms" in the log preview, and "0 ° from up"
  with a space before the degree sign.

## Controls that look or behave differently for the same job

- [ ] 20. "Use its own appearance in this setup" is a checkbox on four cards on the Details tab,
  while Button layout is a segmented control, Overlay font a dropdown, and Overlay sounds a full
  editor.
- [ ] 21. Toggles and checkboxes are mixed for on/off settings: toggles for "Controllers this
  setup supports" and Overlay sounds, checkboxes for "Can't be played without…" and rule enables.
- [ ] 22. Button sizes vary: "Make active", "Hide", "Expand all" and "Collapse all" are small or
  unstyled, while "Duplicate", "Find by pressing" and "+ New macro" are full size.
- [ ] 23. The top of each items tab differs: Layers has a bare "+ New layer" button, Macros, Info
  and Log overlays have a card with a description, and Menus has an "Add a menu:" row. The
  "Copy from another setup…" link sits in a different place on each.
- [ ] 24. Disclosure triangles come in two sizes: a large ▾ on item cards and a small ▸ on
  "Appearance" and "Shared … usable here (0)".
- [ ] 25. "Remove" is shown three ways: a small red underlined link under a gesture label, "×"
  buttons on rules, cells and macros, and "×" on the card corner to delete a whole item.
- [ ] 26. Some cards lack the ⓘ icon. "Controller requirements" and the four appearance cards on
  Details have none, and every other card has one.
- [ ] 27. "Expand all" and "Collapse all" appear on tabs with nothing to expand (Combos, Guide)
  but are hidden on Gyro.
- [ ] 28. On the Details tab, "Name" sits outside any card, while everything else is in one.
- [ ] 29. The profile dropdown and the rename field next to it are both unlabelled and both
  read "Deus Ex".
- [ ] 30. Mapping summaries use colours that look like errors or links with no legend: "Menu" in
  red, "Toggle" in green, "Right click" in teal.

## Spacing and density

- [ ] 31. The header (title, remapping toggle, active profile, daemon note) repeats on every page,
  including the Manual. With the daemon down it pushes content about 230 px down.
- [ ] 32. The radial menu preview is about 750 × 800 px and pushes the item list well below the
  fold.
- [ ] 33. Overlay sounds is a long wall of small sliders: four sounds with three sliders each, for
  every overlay type, repeated on App settings and on every setup's Details tab.
- [ ] 34. Sound labels mix case ("Step", "Pick" against "volume", "pitch", "length") and use a
  smaller font.
- [ ] 35. Long explanations are in very small grey text: the controller-requirements note, the
  "Shown" note on info overlays, and the export notes.
- [ ] 36. Dropdowns have different widths on the same page: 280, 200 and 160 px on App settings.
- [ ] 37. In the Add a setup dialog the "installed" tags touch the scrollbar, and the right pane
  is mostly empty.
- [ ] 38. In the Add and Export dialogs the item counts include zeros ("0 layers · 0 macros …")
  and wrap so that "overlays" sits alone on a line.
- [ ] 39. In the Export dialog the Description field is one line and cuts off its text, the
  Author placeholder repeats its label, and "Library pack: keep the library's ID" uses an
  internal concept.

## Dark mode

- [ ] 40. With the daemon down, the controller picture is barely visible against the background,
  and its callout lines disappear.
- [ ] 41. The selected tab is only slightly darker than the tab bar, so it is hard to see which
  tab is active.
- [ ] 42. The menu and overlay previews are always dark navy: fine in dark mode, but a heavy slab
  in light mode.

## Picture callouts

- [ ] 43. Leader lines cross each other around the face buttons and the right stick.
- [ ] 44. Long labels are cut on the right ("Menu "Augme…") but not on the left
  ("Menu "Belt" +").
- [ ] 45. A trailing "+" ("Esc +", "(nothing) +") means there is more, but nothing explains it.

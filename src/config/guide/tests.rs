use super::*;
use crate::config::{ButtonAction, Combo, Gestures, MouseButton, Profile, StickAction, TriggerAction, combo_key};

fn profile() -> Profile {
    let mut p = Profile::passthrough("p");
    p.set_button(Button::South, ButtonAction::Keys(vec!["KEY_E".into()]));
    p.set_button(Button::DpadLeft, ButtonAction::Keys(vec!["KEY_Q".into()]));
    p.set_button(Button::DpadRight, ButtonAction::Keys(vec!["KEY_E".into()]));
    p.set_button(Button::Select, ButtonAction::Disabled);
    p
}

fn btn(b: Button) -> GuideInput {
    GuideInput::Button(b)
}

/// A combo's inputs in the order its row is keyed by.
fn combo_inputs(buttons: &[Button]) -> Vec<GuideInput> {
    combo_key(buttons).into_iter().map(GuideInput::Button).collect()
}

fn input_key(i: GuideInput) -> RowKey {
    RowKey::Row(RowId::Input(i))
}

fn row_of(rows: &[MappingRow], inputs: &[GuideInput]) -> Option<MappingRow> {
    rows.iter().find(|r| r.inputs == inputs).cloned()
}

#[test]
fn rows_follow_the_mappings_and_skip_guide_and_disabled_buttons() {
    let rows = mapping_rows(&profile());
    assert!(rows.iter().all(|r| r.inputs.iter().all(|i| *i != btn(Button::Guide) && *i != btn(Button::Select))));
    assert_eq!(row_of(&rows, &[btn(Button::South)]).unwrap().text, "{keyboard:e}", "default text is the action's description");
}

#[test]
fn default_text_shows_pad_buttons_and_mouse_clicks_as_glyph_tokens() {
    let mut p = profile();
    p.set_button(Button::East, ButtonAction::Gamepad(Button::South));
    p.set_button(Button::West, ButtonAction::Mouse(MouseButton::Left));
    let rows = mapping_rows(&p);
    assert_eq!(row_of(&rows, &[btn(Button::East)]).unwrap().text, "{pad:south}");
    assert_eq!(row_of(&rows, &[btn(Button::West)]).unwrap().text, "{mouse:leftclick}");
}

#[test]
fn the_default_label_is_the_default_text_without_tokens() {
    let mut p = profile();
    p.set_button(Button::East, ButtonAction::Gamepad(Button::South));
    p.set_button(Button::West, ButtonAction::Keys(vec!["KEY_APOSTROPHE".into()]));
    let rows = editable_rows(&p);
    let label = |b| rows.iter().find(|r| r.inputs == [btn(b)]).unwrap().default_label.clone();
    assert_eq!(label(Button::East), "A");
    assert_eq!(label(Button::West), "Press “'”");
}

#[test]
fn triggers_and_sticks_get_rows_when_they_do_something() {
    let mut p = profile();
    p.left_stick.action = StickAction::mouse(1000.0);
    p.left_trigger.action = TriggerAction::Button { action: ButtonAction::Keys(vec!["KEY_Z".into()]), threshold: 0.5 };
    let rows = mapping_rows(&p);
    assert_eq!(row_of(&rows, &[GuideInput::Stick(Stick::Left)]).unwrap().text, "Move the mouse");
    assert_eq!(row_of(&rows, &[GuideInput::Trigger(Trigger::Left)]).unwrap().text, "{keyboard:z}");
    assert!(row_of(&rows, &[GuideInput::Stick(Stick::Right)]).is_none(), "a pass-through stick isn't listed");
}

#[test]
fn a_label_override_replaces_the_action_text() {
    let mut p = profile();
    p.guide.set_text(input_key(btn(Button::South)), "Use".into());
    assert_eq!(row_of(&mapping_rows(&p), &[btn(Button::South)]).unwrap().text, "Use");
}

#[test]
fn a_hidden_row_leaves_the_overlay_but_keeps_the_mapping() {
    let mut p = profile();
    p.guide.set_hidden(input_key(btn(Button::South)), true);
    assert!(row_of(&mapping_rows(&p), &[btn(Button::South)]).is_none());
    assert_eq!(p.button(Button::South), &ButtonAction::Keys(vec!["KEY_E".into()]), "the mapping itself is untouched");
}

#[test]
fn merged_inputs_share_one_row_at_the_first_of_them() {
    let mut p = profile();
    p.guide.merge(vec![btn(Button::DpadRight), btn(Button::DpadLeft)], "Lean".into());
    let rows = mapping_rows(&p);
    let merged: Vec<&MappingRow> = rows.iter().filter(|r| r.text == "Lean").collect();
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].inputs, [btn(Button::DpadLeft), btn(Button::DpadRight)], "in mapping order");
    assert!(row_of(&rows, &[btn(Button::DpadLeft)]).is_none());
}

#[test]
fn a_hidden_merged_row_is_left_off() {
    let mut p = profile();
    p.guide.merge(vec![btn(Button::DpadLeft), btn(Button::DpadRight)], "Lean".into());
    p.guide.set_hidden(RowKey::Merged(0), true);
    assert!(mapping_rows(&p).iter().all(|r| r.text != "Lean"));
}

#[test]
fn rows_join_their_glyphs_by_shape() {
    let parts = vec!["A".to_string(), "B".to_string()];
    assert_eq!(RowShape::Merged.join(&parts), "A, B");
    assert_eq!(RowShape::Combo.join(&parts), "A + B");
    assert_eq!(RowShape::Input.join(&parts[..1]), "A");
    assert_eq!(RowShape::Gesture(GestureKind::DoubleTap).join(&parts[..1]), "A ×2");
    assert_eq!(RowShape::Gesture(GestureKind::TripleTap).join(&parts[..1]), "A ×3");
    assert_eq!(RowShape::Gesture(GestureKind::LongPress).join(&parts[..1]), "A hold");
}

#[test]
fn gestures_and_combos_are_listed_after_the_inputs() {
    let mut p = profile();
    p.gestures.insert(Button::South, Gestures { double_tap: Some(ButtonAction::Keys(vec!["KEY_F".into()])), ..Default::default() });
    p.combos.push(Combo { buttons: vec![Button::North, Button::West], action: ButtonAction::Keys(vec!["KEY_R".into()]) });
    p.combos.push(Combo { buttons: vec![Button::North, Button::Guide], action: ButtonAction::Keys(vec!["KEY_X".into()]) });
    let rows = mapping_rows(&p);
    assert_eq!(row_of(&rows, &[btn(Button::South)]).map(|r| r.text), Some("{keyboard:e}".into()));
    assert!(rows.iter().any(|r| r.inputs == [btn(Button::South)] && r.text == "{keyboard:f}" && r.shape == RowShape::Gesture(GestureKind::DoubleTap)));
    assert_eq!(row_of(&rows, &[btn(Button::North), btn(Button::West)]).unwrap().text, "{keyboard:r}");
    assert_eq!(rows.iter().filter(|r| r.inputs.contains(&btn(Button::Guide))).count(), 0, "Guide is never listed");
    assert_eq!(rows.last().unwrap().inputs.len(), 2, "combos come last");
}

#[test]
fn a_gesture_or_combo_row_can_be_renamed_and_hidden() {
    let mut p = profile();
    p.gestures.insert(Button::South, Gestures { double_tap: Some(ButtonAction::Keys(vec!["KEY_F".into()])), ..Default::default() });
    p.combos.push(Combo { buttons: vec![Button::North, Button::West], action: ButtonAction::Keys(vec!["KEY_R".into()]) });
    let gesture = RowKey::Row(RowId::Gesture(btn(Button::South), GestureKind::DoubleTap));
    let combo = RowKey::Row(RowId::Combo(combo_inputs(&[Button::North, Button::West])));
    p.guide.set_text(gesture.clone(), "Reload".into());
    p.guide.set_hidden(combo.clone(), true);
    let rows = mapping_rows(&p);
    assert!(rows.iter().any(|r| r.inputs == [btn(Button::South)] && r.text == "Reload"));
    assert!(rows.iter().all(|r| r.inputs != combo_inputs(&[Button::North, Button::West])), "hidden combo is off");
    let shown = editable_rows(&p).into_iter().find(|r| r.key == combo).unwrap();
    assert!(shown.hidden, "the tab still lists it, hidden");
}

#[test]
fn edits_to_gestures_and_combos_roundtrip_under_readable_keys() {
    let mut p = profile();
    p.gestures.insert(Button::South, Gestures { double_tap: Some(ButtonAction::Keys(vec!["KEY_F".into()])), ..Default::default() });
    p.combos.push(Combo { buttons: vec![Button::North, Button::West], action: ButtonAction::Keys(vec!["KEY_R".into()]) });
    p.guide.set_text(RowKey::Row(RowId::Gesture(btn(Button::South), GestureKind::DoubleTap)), "Reload".into());
    p.guide.set_hidden(RowKey::Row(RowId::Combo(combo_inputs(&[Button::North, Button::West]))), true);
    let text = toml::to_string_pretty(&p.guide).unwrap();
    assert!(text.contains("gesture:button:South:double_tap"), "{text}");
    assert!(text.contains("combo:button:") && text.contains("+button:"), "{text}");
    let back: GuideSettings = toml::from_str(&text).unwrap();
    assert_eq!(back, p.guide);
}

#[test]
fn inputs_roundtrip_through_the_file_under_readable_keys() {
    let mut p = profile();
    p.guide.set_text(RowKey::Row(RowId::Input(GuideInput::Trigger(Trigger::Left))), "Aim".into());
    p.left_trigger.action = TriggerAction::Button { action: ButtonAction::Keys(vec!["KEY_Z".into()]), threshold: 0.5 };
    let text = toml::to_string_pretty(&p.guide).unwrap();
    assert!(text.contains("trigger:Left"), "{text}");
    let back: GuideSettings = toml::from_str(&text).unwrap();
    assert_eq!(back, p.guide);
}

#[test]
fn empty_text_goes_back_to_the_default() {
    let mut p = profile();
    let key = input_key(btn(Button::DpadLeft));
    p.guide.set_text(key.clone(), "Lean".into());
    let row = editable_rows(&p).into_iter().find(|r| r.key == key).unwrap();
    assert_eq!((row.text.as_str(), row.overridden), ("Lean", true));
    p.guide.set_text(key, String::new());
    assert!(p.guide.is_default());
}

#[test]
fn showing_a_row_again_leaves_no_edit_behind() {
    let mut p = profile();
    let key = input_key(btn(Button::DpadLeft));
    p.guide.set_hidden(key.clone(), true);
    assert!(editable_rows(&p).iter().any(|r| r.key == key && r.hidden), "the tab still lists it");
    p.guide.set_hidden(key, false);
    assert!(p.guide.is_default());
}

#[test]
fn merging_again_moves_an_input_to_the_new_row() {
    let mut p = profile();
    let (left, right, south) = (btn(Button::DpadLeft), btn(Button::DpadRight), btn(Button::South));
    p.guide.merge(vec![left, right], "Lean".into());
    p.guide.merge(vec![left, south], "Other".into());
    assert_eq!(p.guide.merged.len(), 1, "the first row is left with one input, so it goes");
    assert_eq!(p.guide.merged[0].inputs, [left, south]);
}

#[test]
fn separate_merges_stay_separate() {
    let mut p = profile();
    let (left, right, south) = (btn(Button::DpadLeft), btn(Button::DpadRight), btn(Button::South));
    p.guide.merge(vec![left, right], "Lean".into());
    p.guide.merge(vec![south, btn(Button::West)], "Use".into());
    assert_eq!(p.guide.merged.len(), 2);
    let rows = mapping_rows(&p);
    assert!(rows.iter().any(|r| r.inputs == [left, right]));
    assert!(rows.iter().any(|r| r.inputs.contains(&south) && r.inputs.len() == 2));
}

#[test]
fn adding_an_input_to_a_merged_row_takes_it_off_its_old_row() {
    let mut p = profile();
    let (left, right, south) = (btn(Button::DpadLeft), btn(Button::DpadRight), btn(Button::South));
    p.guide.merge(vec![left, right], "Lean".into());
    p.guide.merge(vec![south, btn(Button::West)], "Use".into());
    p.guide.add_to_merge(0, south);
    assert_eq!(p.guide.merged.len(), 1, "the other row is left with one input, so it goes");
    assert_eq!(p.guide.merged[0].inputs, [left, right, south]);
}

#[test]
fn custom_rows_list_after_everything_else_and_take_inputs() {
    let mut p = profile();
    p.guide.add_custom("Assign group instead".into());
    p.guide.add_custom_input(0, btn(Button::LeftBumper));
    p.guide.add_custom_input(0, btn(Button::RightBumper));
    p.guide.add_custom_input(0, btn(Button::RightBumper));
    let rows = mapping_rows(&p);
    let custom = rows.last().unwrap();
    assert_eq!(custom.text, "Assign group instead");
    assert_eq!(custom.inputs, [btn(Button::LeftBumper), btn(Button::RightBumper)], "each input once");
    assert_eq!(editable_rows(&p).last().unwrap().shape, RowShape::Combo);
    p.guide.set_text(RowKey::Custom(0), "Groups".into());
    assert_eq!(mapping_rows(&p).last().unwrap().text, "Groups");
    p.guide.set_hidden(RowKey::Custom(0), true);
    assert!(p.guide.custom.len() == 1, "a custom row isn't hidden, only removed");
    p.guide.remove_custom(0);
    assert!(p.guide.custom.is_empty());
}

#[test]
fn custom_rows_roundtrip_through_the_file() {
    let mut p = profile();
    p.guide.add_custom("Assign group instead".into());
    p.guide.add_custom_input(0, btn(Button::LeftBumper));
    let text = toml::to_string_pretty(&p.guide).unwrap();
    let back: GuideSettings = toml::from_str(&text).unwrap();
    assert_eq!(back, p.guide);
}

#[test]
fn split_gives_the_inputs_back_their_own_rows() {
    let mut p = profile();
    p.guide.merge(vec![btn(Button::DpadLeft), btn(Button::DpadRight)], "Lean".into());
    p.guide.split(0);
    assert!(editable_rows(&p).iter().any(|r| r.key == input_key(btn(Button::DpadLeft))));
}

//! Tests of the Edit Controls pages, through the editor's own steps.

use super::*;
use super::buttons::action_rows;
use super::macros;
use super::values;
use crate::config::{Analog, GyroActivation, GyroInput, Trigger};

fn top_index(row: Top) -> usize {
    top_rows().iter().position(|&r| r == row).unwrap()
}

#[test]
fn choosing_a_gamepad_button_maps_it_and_returns_to_the_top() {
    let mut config = Config::default();
    let mut editor = LayoutEditor::new();
    editor.cursor = top_index(Top::Button(Button::East));
    editor.choose(&mut config);
    editor.cursor = 0; // "Gamepad button…"
    assert_eq!(editor.choose(&mut config), EditStep::Stay);
    editor.cursor = Button::ALL.iter().position(|&b| b == Button::North).unwrap();
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    assert_eq!(config.active().unwrap().button(Button::East), &ButtonAction::Gamepad(Button::North));
    assert_eq!(editor.page, Page::Top);
}

#[test]
fn a_choice_sets_the_button_and_back_leaves_the_editor() {
    let mut config = Config::default();
    let mut editor = LayoutEditor::new();
    editor.cursor = 0; // South
    editor.choose(&mut config);
    editor.cursor = action_rows(Slot::Button(Button::South)).iter().position(|r| matches!(r, ActionRow::Choice(1))).unwrap();
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    assert_eq!(config.active().unwrap().button(Button::South), &ButtonAction::NextProfile);
    assert_eq!(editor.back(), EditStep::Leave);
}

#[test]
fn a_double_tap_gesture_is_set_from_the_gestures_page() {
    let mut config = Config::default();
    let mut editor = LayoutEditor::new();
    editor.cursor = 0; // South
    editor.choose(&mut config);
    editor.cursor = 1; // Gestures…
    editor.choose(&mut config);
    assert_eq!(editor.page, Page::Gestures(Button::South));
    editor.cursor = 0; // Double tap
    editor.choose(&mut config);
    editor.cursor = action_rows(Slot::Gesture(Button::South, GestureKind::DoubleTap))
        .iter()
        .position(|r| matches!(r, ActionRow::Choice(1)))
        .unwrap();
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    let profile = config.active().unwrap();
    assert_eq!(profile.gestures[&Button::South].double_tap, Some(ButtonAction::NextProfile));
    // Back on the gestures page, then the button, then the top.
    assert_eq!(editor.page, Page::Gestures(Button::South));
}

#[test]
fn a_layer_button_is_bound_from_the_layers_page() {
    let mut config = Config::default();
    assert!(!config.general.layers.is_empty(), "General should have the Guide layer");
    let mut editor = LayoutEditor::new();
    editor.cursor = top_index(Top::Layers);
    editor.choose(&mut config);
    editor.cursor = 0; // the first layer
    editor.choose(&mut config);
    assert_eq!(editor.page, Page::Layer(0));
    editor.cursor = Button::ALL.iter().position(|&b| b == Button::South).unwrap();
    editor.choose(&mut config);
    let shot = action_rows(Slot::Layer(0, Button::South)).iter().position(|r| matches!(r, ActionRow::Choice(3))).unwrap();
    editor.cursor = shot;
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    assert_eq!(config.general.layers[0].buttons.get(&Button::South), Some(&ButtonAction::Screenshot));
}

#[test]
fn a_combo_is_added_given_buttons_and_an_action_and_then_deleted() {
    let mut config = Config::default();
    let mut editor = LayoutEditor::new();
    editor.cursor = top_index(Top::Combos);
    editor.choose(&mut config);
    editor.cursor = combos(&config).len(); // Add combo
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    assert_eq!(editor.page, Page::Combo(0));
    // Members: South and East.
    editor.cursor = Button::ALL.iter().position(|&b| b == Button::South).unwrap();
    editor.choose(&mut config);
    editor.cursor = Button::ALL.iter().position(|&b| b == Button::East).unwrap();
    editor.choose(&mut config);
    assert_eq!(config.active().unwrap().combos[0].buttons, [Button::South, Button::East]);
    // Action: Screenshot.
    editor.cursor = Button::ALL.len();
    editor.choose(&mut config);
    let shot = action_rows(Slot::Combo(0)).iter().position(|r| matches!(r, ActionRow::Choice(3))).unwrap();
    editor.cursor = shot;
    editor.choose(&mut config);
    assert_eq!(config.active().unwrap().combos[0].action, ButtonAction::Screenshot);
    // Delete.
    editor.page = Page::Combo(0);
    editor.cursor = Button::ALL.len() + 1;
    editor.choose(&mut config);
    assert!(combos(&config).is_empty());
}

#[test]
fn a_button_can_send_a_key_from_the_key_picker() {
    let mut config = Config::default();
    let mut editor = LayoutEditor::new();
    editor.cursor = 0; // South
    editor.choose(&mut config);
    editor.cursor = action_rows(Slot::Button(Button::South)).iter().position(|r| *r == ActionRow::Keys).unwrap();
    editor.choose(&mut config);
    assert_eq!(editor.page, Page::Keys(Slot::Button(Button::South)));
    editor.cursor = buttons::KEY_PICKS.iter().position(|&k| k == "KEY_ENTER").unwrap();
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    assert_eq!(config.active().unwrap().button(Button::South), &ButtonAction::Keys(vec!["KEY_ENTER".into()]));
    // The picker stays open, so more keys can be added; Done leaves it.
    assert_eq!(editor.page, Page::Keys(Slot::Button(Button::South)));
    editor.cursor = buttons::KEY_PICKS.len();
    assert_eq!(editor.choose(&mut config), EditStep::Stay);
    assert_eq!(editor.page, Page::Top);
}

#[test]
fn key_combinations_toggle_keys_in_and_out() {
    let mut config = Config::default();
    let mut editor = LayoutEditor::new();
    editor.cursor = 0; // South
    editor.choose(&mut config);
    editor.cursor = action_rows(Slot::Button(Button::South)).iter().position(|r| *r == ActionRow::Keys).unwrap();
    editor.choose(&mut config);
    let key = |editor: &mut LayoutEditor, config: &mut Config, code: &str| {
        editor.cursor = buttons::KEY_PICKS.iter().position(|&k| k == code).unwrap();
        editor.choose(config);
    };
    key(&mut editor, &mut config, "KEY_LEFTCTRL");
    key(&mut editor, &mut config, "KEY_ESC");
    assert_eq!(config.active().unwrap().button(Button::South), &ButtonAction::Keys(vec!["KEY_LEFTCTRL".into(), "KEY_ESC".into()]));
    // Choosing a key that is already pressed takes it out.
    key(&mut editor, &mut config, "KEY_LEFTCTRL");
    assert_eq!(config.active().unwrap().button(Button::South), &ButtonAction::Keys(vec!["KEY_ESC".into()]));
}

#[test]
fn breadcrumbs_follow_the_pages_opened() {
    let mut config = Config::default();
    let parents = ["Menu".to_string()];
    let mut editor = LayoutEditor::new();
    assert!(editor.view(&config, &parents).crumbs == ["Menu"]);
    editor.cursor = 0; // South
    editor.choose(&mut config);
    let view = editor.view(&config, &parents);
    assert_eq!(view.title, "A button");
    assert_eq!(view.crumbs, ["Menu", "Edit Controls"]);
    editor.cursor = 1; // Gestures…
    editor.choose(&mut config);
    assert_eq!(editor.view(&config, &parents).crumbs, ["Menu", "Edit Controls", "A button"]);
}

#[test]
fn a_button_can_be_made_a_toggle_and_back() {
    let mut config = Config::default();
    let mut editor = LayoutEditor::new();
    editor.cursor = 0; // South
    editor.choose(&mut config);
    editor.cursor = action_rows(Slot::Button(Button::South)).iter().position(|r| *r == ActionRow::Wrap(buttons::Wrap::Toggle)).unwrap();
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    let now = config.active().unwrap().button(Button::South).clone();
    assert!(matches!(now, ButtonAction::Toggle(_)));
    // Choosing it again unwraps the same action.
    editor.page = Page::Action(Slot::Button(Button::South));
    editor.cursor = action_rows(Slot::Button(Button::South)).iter().position(|r| *r == ActionRow::Wrap(buttons::Wrap::Toggle)).unwrap();
    editor.choose(&mut config);
    // South was Gamepad(South) before it was wrapped.
    assert_eq!(config.active().unwrap().button(Button::South), &ButtonAction::Gamepad(Button::South));
}

#[test]
fn a_turbo_rate_and_a_toggles_start_state_can_be_set() {
    let mut config = Config::default();
    let mut editor = LayoutEditor::new();
    editor.cursor = 0; // South
    editor.choose(&mut config);
    editor.cursor = action_rows(Slot::Button(Button::South)).iter().position(|r| *r == ActionRow::Wrap(buttons::Wrap::Turbo)).unwrap();
    editor.choose(&mut config);
    // The turbo's rate row is the one after the page's own rows.
    let rate = page_rows(Slot::Button(Button::South), &config.active().unwrap().button(Button::South).clone())
        .iter()
        .position(|r| *r == ActionRow::Tuning(buttons::Tuning::TurboRate))
        .unwrap();
    editor.cursor = rate;
    editor.choose(&mut config);
    assert!(matches!(config.active().unwrap().button(Button::South), ButtonAction::Turbo { rate, .. } if *rate == 15.0));

    // A toggle's start-on row flips the flag.
    editor.cursor = action_rows(Slot::Button(Button::South)).iter().position(|r| *r == ActionRow::Wrap(buttons::Wrap::Toggle)).unwrap();
    editor.choose(&mut config);
    editor.cursor = page_rows(Slot::Button(Button::South), &config.active().unwrap().button(Button::South).clone())
        .iter()
        .position(|r| *r == ActionRow::Tuning(buttons::Tuning::ToggleStart))
        .unwrap();
    editor.choose(&mut config);
    assert!(matches!(config.active().unwrap().button(Button::South), ButtonAction::Toggle(t) if t.start_on));
}

#[test]
fn gyro_activation_cycles_through_its_presets() {
    let mut config = Config::default();
    let mut editor = LayoutEditor::new();
    editor.cursor = top_index(Top::Gyro);
    editor.choose(&mut config);
    editor.cursor = values::GYRO_ACTIVATION;
    editor.choose(&mut config);
    assert_eq!(config.active().unwrap().gyro.activation, GyroActivation::WhileHeld(GyroInput::LeftTrigger));
}

#[test]
fn a_layer_can_be_added_and_held_from_a_button() {
    let mut config = Config::default();
    let before = config.general.layers.len();
    let mut editor = LayoutEditor::new();
    editor.cursor = top_index(Top::Layers);
    editor.choose(&mut config);
    editor.cursor = before; // Add layer
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    assert_eq!(config.general.layers.len(), before + 1);
    let name = config.general.layers.last().unwrap().name.clone();
    // Back on the top page, bind South to hold that layer.
    editor.page = Page::Top;
    editor.cursor = 0;
    editor.choose(&mut config);
    editor.cursor = action_rows(Slot::Button(Button::South)).iter().position(|r| *r == ActionRow::Pick(buttons::Picker::Layer)).unwrap();
    editor.choose(&mut config);
    editor.cursor = config.general.layers.len() - 1;
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    assert_eq!(config.active().unwrap().button(Button::South), &ButtonAction::Layer(name));
}

#[test]
fn a_macro_in_use_is_kept_and_an_unused_one_is_deleted() {
    let mut config = Config::default();
    let unused = macros::add_macro(&mut config).unwrap();
    let used = macros::add_macro(&mut config).unwrap();
    let used_name = config.general.macros[used].name.clone();
    active_profile_mut(&mut config).unwrap().buttons.insert(Button::East, ButtonAction::Macro { name: used_name, repeat: false });
    assert!(!macros::delete_macro(&mut config, used));
    assert!(macros::delete_macro(&mut config, unused));
    assert_eq!(config.general.macros.len(), 1);
}

#[test]
fn a_trigger_zone_is_added_and_given_an_action() {
    let mut config = Config::default();
    let mut editor = LayoutEditor::new();
    editor.cursor = top_index(Top::Trigger(Trigger::Left));
    editor.choose(&mut config);
    // The zones row comes after the trigger's own rows.
    editor.cursor = values::TRIGGER_ROWS;
    editor.choose(&mut config);
    assert_eq!(editor.page, Page::Zones(Analog::Trigger(Trigger::Left)));
    editor.cursor = 0; // Add zone
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    assert_eq!(editor.page, Page::Zone(Analog::Trigger(Trigger::Left), 0));
    editor.cursor = zones::ZONE_ACTION;
    editor.choose(&mut config);
    let shot = action_rows(Slot::Zone(Analog::Trigger(Trigger::Left), 0)).iter().position(|r| matches!(r, ActionRow::Choice(3))).unwrap();
    editor.cursor = shot;
    assert_eq!(editor.choose(&mut config), EditStep::Changed);
    assert_eq!(zones::zone_action(&config, Analog::Trigger(Trigger::Left), 0), ButtonAction::Screenshot);
}

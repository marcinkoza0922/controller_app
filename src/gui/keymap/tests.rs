use super::*;
use crate::config::{Button, MotionDirection, MotionTarget, Trigger};
use crate::gui::tests::app;

/// The app editing a new keyboard profile in General.
fn keyboard_app() -> App {
    let mut app = app();
    app.config.general.profiles.push(Profile::keyboard("Keys"));
    app.editing = app.config.general.profiles.len() - 1;
    app
}

/// Picks keys in the on-screen keyboard, closing a single-key pick the way the runtime would.
fn pick(app: &mut App, field: KeyField, single: bool, keys: &[&'static str]) {
    crate::gui::tests::pick(app, field, single, keys);
    if single {
        let _ = app.update(Message::PickerClose { apply: true });
    }
}

fn map(app: &App) -> &KeyboardMap {
    &app.profile().unwrap().keyboard
}

#[test]
fn a_picked_key_is_added_and_can_be_pointed_at_another_key() {
    let mut app = keyboard_app();
    pick(&mut app, KeyField::KbNew, true, &["KEY_W"]);
    assert_eq!(map(&app).keys["KEY_W"], ButtonAction::Keys(vec![]));
    assert!(keyboard_problem(map(&app)).unwrap().contains("no key to press"));

    pick(&mut app, KeyField::KbTarget(KbInput::Key("KEY_W".into()), vec![]), false, &["KEY_E"]);
    assert_eq!(map(&app).keys["KEY_W"], ButtonAction::Keys(vec!["KEY_E".into()]));
    assert_eq!(keyboard_problem(map(&app)), None);
}

#[test]
fn super_and_repeated_keys_are_refused_with_a_message() {
    let mut app = keyboard_app();
    pick(&mut app, KeyField::KbNew, true, &["KEY_LEFTMETA"]);
    assert!(map(&app).keys.is_empty());
    assert!(app.message.as_ref().unwrap().0.contains("can't be remapped"));
    pick(&mut app, KeyField::KbNew, true, &["KEY_W"]);
    pick(&mut app, KeyField::KbNew, true, &["KEY_W"]);
    assert!(app.message.as_ref().unwrap().0.contains("already remapped"));
    assert_eq!(map(&app).keys.len(), 1);
}

#[test]
fn repicking_a_source_moves_its_mapping() {
    let mut app = keyboard_app();
    app.profile_mut().unwrap().keyboard = Profile::esdf_layout("x").keyboard;
    app.profile_mut().unwrap().keyboard.keys.clear();
    app.profile_mut().unwrap().keyboard.keys.insert("KEY_E".into(), ButtonAction::Keys(vec!["KEY_W".into()]));
    pick(&mut app, KeyField::KbSource(KbInput::Key("KEY_E".into())), true, &["KEY_1"]);
    assert_eq!(map(&app).keys["KEY_1"], ButtonAction::Keys(vec!["KEY_W".into()]));
    assert!(!map(&app).keys.contains_key("KEY_E"));
}

#[test]
fn mouse_buttons_can_be_remapped_to_keys_and_removed() {
    let mut app = keyboard_app();
    let _ = app.update(Message::KbAddMouse);
    assert_eq!(map(&app).mouse[&MouseButton::Left], ButtonAction::Disabled);
    let left = KbInput::Mouse(MouseButton::Left);
    let _ = app.update(Message::KbSetAction(left.clone(), ButtonAction::Keys(vec!["KEY_LEFTCTRL".into(), "KEY_Z".into()])));
    assert_eq!(map(&app).mouse[&MouseButton::Left], ButtonAction::Keys(vec!["KEY_LEFTCTRL".into(), "KEY_Z".into()]));
    let _ = app.update(Message::KbMouseSource(MouseButton::Left, MouseButton::Back));
    assert!(map(&app).mouse.contains_key(&MouseButton::Back));
    let _ = app.update(Message::KbRemove(KbInput::Mouse(MouseButton::Back)));
    assert!(map(&app).mouse.is_empty());
}

#[test]
fn the_panic_chord_cannot_be_blank_or_unknown() {
    let mut app = app();
    assert_eq!(app.validate(), None);
    app.config.panic_chord.clear();
    assert!(app.validate().unwrap().contains("can't be empty"));
    app.config.panic_chord = vec!["KEY_NOPE".into()];
    assert!(app.validate().unwrap().contains("unknown key"));
    pick(&mut app, KeyField::PanicChord, false, &["KEY_LEFTCTRL", "KEY_PAUSE"]);
    assert_eq!(app.config.panic_chord, ["KEY_LEFTCTRL", "KEY_PAUSE"]);
    assert_eq!(app.validate(), None);
}

#[test]
fn pad_outputs_wheel_and_direction_rows_are_valid_and_need_a_pad() {
    let mut app = keyboard_app();
    assert!(!map(&app).uses_pad());
    pick(&mut app, KeyField::KbNew, true, &["KEY_SPACE"]);
    let space = KbInput::Key("KEY_SPACE".into());
    let _ = app.update(Message::KbSetAction(space.clone(), ButtonAction::Gamepad(Button::LeftStickUp)));
    let _ = app.update(Message::KbSetAction(space, ButtonAction::PadTrigger(Trigger::Right)));
    assert_eq!(map(&app).keys["KEY_SPACE"], ButtonAction::PadTrigger(Trigger::Right));
    assert!(map(&app).uses_pad());

    let _ = app.update(Message::KbAddWheel);
    let _ = app.update(Message::KbAddMotion);
    assert_eq!(map(&app).wheel[&WheelDirection::Up], ButtonAction::Disabled);
    assert_eq!(map(&app).motion_buttons[&MotionDirection::Up], ButtonAction::Disabled);
    let _ = app.update(Message::KbWheelSource(WheelDirection::Up, WheelDirection::Down));
    let _ = app.update(Message::KbMotionSource(MotionDirection::Up, MotionDirection::Left));
    assert!(map(&app).wheel.contains_key(&WheelDirection::Down) && map(&app).motion_buttons.contains_key(&MotionDirection::Left));
    assert_eq!(keyboard_problem(map(&app)), None);
}

#[test]
fn mouse_movement_settings_edit_the_profile() {
    let mut app = keyboard_app();
    let _ = app.update(Message::KbMotion(MotionEdit::Target(MotionChoice::Stick(Stick::Left))));
    let _ = app.update(Message::KbMotion(MotionEdit::Counts(250.0)));
    let _ = app.update(Message::KbMotion(MotionEdit::DecayMs(40.0)));
    let _ = app.update(Message::KbMotion(MotionEdit::InvertY(true)));
    let _ = app.update(Message::KbMotion(MotionEdit::Deadzone(0.2)));
    let p = app.profile().unwrap();
    assert_eq!(p.keyboard.motion.target, MotionTarget::Stick(Stick::Left));
    assert_eq!((p.keyboard.motion.counts, p.keyboard.motion.decay_ms, p.keyboard.motion.invert_y), (250.0, 40, true));
    assert_eq!(p.left_stick.deadzone, 0.2);
    assert!(p.keyboard.uses_pad());
}

#[test]
fn the_pad_template_is_valid() {
    let p = Profile::keyboard_to_pad("p");
    assert_eq!(keyboard_problem(&p.keyboard), None);
    assert!(p.keyboard.uses_pad());
}

#[test]
fn gestures_can_be_added_set_and_dropped() {
    use crate::config::GestureKind;
    let mut app = keyboard_app();
    pick(&mut app, KeyField::KbGestureNew, true, &["KEY_CAPSLOCK"]);
    assert!(map(&app).gestures.contains_key("KEY_CAPSLOCK"));
    let _ = app.update(Message::KbGestureMouse(MouseButton::Back));
    assert!(map(&app).gestures.contains_key("BTN_SIDE"));

    let set = ButtonAction::Keys(vec!["KEY_ESC".into()]);
    let _ = app.update(Message::KbGestureSet("KEY_CAPSLOCK".into(), GestureKind::DoubleTap, Some(set.clone())));
    assert_eq!(map(&app).gestures["KEY_CAPSLOCK"].double_tap, Some(set));
    assert_eq!(keyboard_problem(map(&app)), None);

    // A key picked for the nested action lands in it.
    let field = KeyField::KbGesture("KEY_CAPSLOCK".into(), GestureKind::DoubleTap, vec![]);
    pick(&mut app, field, false, &["KEY_F5"]);
    assert_eq!(map(&app).gestures["KEY_CAPSLOCK"].double_tap, Some(ButtonAction::Keys(vec!["KEY_F5".into()])));

    let _ = app.update(Message::KbGestureSet("KEY_CAPSLOCK".into(), GestureKind::DoubleTap, None));
    assert!(map(&app).gestures["KEY_CAPSLOCK"].is_empty());
    let _ = app.update(Message::KbGestureDrop("KEY_CAPSLOCK".into()));
    assert!(!map(&app).gestures.contains_key("KEY_CAPSLOCK"));
}

#[test]
fn combos_need_two_inputs_and_take_keys_and_mouse_buttons() {
    let mut app = keyboard_app();
    let _ = app.update(Message::KbAddCombo);
    assert!(keyboard_problem(map(&app)).unwrap().contains("at least two"));
    pick(&mut app, KeyField::KbComboKeys(0), false, &["KEY_J"]);
    let _ = app.update(Message::KbComboMouse(0, MouseButton::Left, true));
    assert_eq!(map(&app).combos[0].inputs, ["KEY_J", "BTN_LEFT"]);
    let _ = app.update(Message::KbComboAction(0, ButtonAction::Keys(vec!["KEY_F9".into()])));
    assert_eq!(keyboard_problem(map(&app)), None);

    // Re-picking the keys keeps the mouse button.
    pick(&mut app, KeyField::KbComboKeys(0), false, &["KEY_K", "KEY_L"]);
    assert_eq!(map(&app).combos[0].inputs, ["KEY_K", "KEY_L", "BTN_LEFT"]);
    pick(&mut app, KeyField::KbCombo(0, vec![]), false, &["KEY_F10"]);
    assert_eq!(map(&app).combos[0].action, ButtonAction::Keys(vec!["KEY_F10".into()]));
    let _ = app.update(Message::KbRemoveCombo(0));
    assert!(map(&app).combos.is_empty());
}

#[test]
fn menus_and_the_on_screen_keyboard_are_rejected_in_keyboard_profiles() {
    let mut app = keyboard_app();
    pick(&mut app, KeyField::KbNew, true, &["KEY_M"]);
    let m = KbInput::Key("KEY_M".into());
    for action in [ButtonAction::OpenMenu("Wheel".into()), ButtonAction::ToggleOverlay, ButtonAction::toggle(ButtonAction::ToggleNumpad)] {
        let _ = app.update(Message::KbSetAction(m.clone(), action));
        assert!(keyboard_problem(map(&app)).unwrap().contains("keyboard profiles can't"), "{:?}", keyboard_problem(map(&app)));
    }
    // Everything else a button can do is fine.
    for action in [ButtonAction::NextProfile, ButtonAction::Macro { name: "x".into(), repeat: true }, ButtonAction::Layer("L".into())] {
        let _ = app.update(Message::KbSetAction(m.clone(), action));
        assert_eq!(keyboard_problem(map(&app)), None);
    }
}

#[test]
fn a_layer_over_a_keyboard_profile_records_only_what_changed() {
    let base = Profile::esdf_layout("base");
    let mut view = base.keyboard.clone();
    view.keys.insert("KEY_Q".into(), ButtonAction::Keys(vec!["KEY_1".into()]));
    view.keys.insert("KEY_W".into(), ButtonAction::Keys(vec!["KEY_UP".into()]));
    view.keys.remove("KEY_E");
    let mut layer = KeyboardMap::default();
    // KEY_E was overridden before and removing it goes back to base.
    layer.keys.insert("KEY_E".into(), ButtonAction::Keys(vec!["KEY_9".into()]));
    super::super::layers::write_back_keyboard(&mut layer, &base.keyboard, &view);
    assert_eq!(layer.keys.len(), 2);
    assert_eq!(layer.keys["KEY_Q"], ButtonAction::Keys(vec!["KEY_1".into()]));
    assert_eq!(layer.keys["KEY_W"], ButtonAction::Keys(vec!["KEY_UP".into()]));
}

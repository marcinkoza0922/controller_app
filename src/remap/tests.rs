use super::*;
use std::time::Instant;
use crate::config::{Button, Config};

fn run(remapper: &mut Remapper, profile: &Profile, events: &[RawEvent]) -> Vec<OutEvent> {
    let mut out = Vec::new();
    for ev in events {
        remapper.event(profile, *ev, &mut out);
    }
    out
}

fn key(k: KeyCode, down: bool) -> RawEvent {
    RawEvent::Key(k, down)
}

fn esdf() -> Profile {
    Profile::esdf_layout("p")
}

/// An empty keyboard profile with `edit` applied to its mappings.
fn profile(edit: impl FnOnce(&mut KeyboardMap)) -> Profile {
    let mut p = Profile::keyboard("p");
    edit(&mut p.keyboard);
    p
}

fn pad_axis(out: &[OutEvent], axis: Axis) -> Option<f32> {
    out.iter().rev().find_map(|e| match e {
        OutEvent::PadAxis(a, v) if *a == axis => Some(*v),
        _ => None,
    })
}

#[test]
fn a_remapped_key_sends_its_target_and_releases_it() {
    let mut r = Remapper::default();
    let out = run(&mut r, &esdf(), &[key(KeyCode::KEY_E, true), key(KeyCode::KEY_E, false)]);
    assert_eq!(out, [OutEvent::Key(KeyCode::KEY_W, true), OutEvent::Key(KeyCode::KEY_W, false)]);
}

#[test]
fn the_letter_rows_shift_one_key_and_the_rest_pass_through() {
    let mut r = Remapper::default();
    let events = [KeyCode::KEY_S, KeyCode::KEY_D, KeyCode::KEY_F, KeyCode::KEY_G, KeyCode::KEY_X, KeyCode::KEY_1, KeyCode::KEY_SPACE];
    let out = run(&mut r, &esdf(), &events.map(|k| key(k, true)));
    let sent = [KeyCode::KEY_A, KeyCode::KEY_S, KeyCode::KEY_D, KeyCode::KEY_F, KeyCode::KEY_Z, KeyCode::KEY_1, KeyCode::KEY_SPACE];
    assert_eq!(out, sent.map(|k| OutEvent::Key(k, true)));
}

#[test]
fn the_left_end_of_each_row_sends_the_right_end_so_nothing_is_lost() {
    let mut r = Remapper::default();
    let out = run(&mut r, &esdf(), &[KeyCode::KEY_Q, KeyCode::KEY_A, KeyCode::KEY_Z].map(|k| key(k, true)));
    let sent = [KeyCode::KEY_BACKSLASH, KeyCode::KEY_APOSTROPHE, KeyCode::KEY_SLASH];
    assert_eq!(out, sent.map(|k| OutEvent::Key(k, true)));
}

#[test]
fn blocking_swallows_unmapped_keys_and_buttons_but_not_movement() {
    let p = profile(|m| m.other_keys = OtherKeys::Block);
    let mut r = Remapper::default();
    let events = [key(KeyCode::KEY_Q, true), RawEvent::Button(MouseButton::Left, true), RawEvent::Move(3, -2)];
    assert_eq!(run(&mut r, &p, &events), [OutEvent::MouseMove(3, -2)]);
}

#[test]
fn a_mouse_button_can_become_a_key_and_a_key_a_mouse_button() {
    let p = profile(|m| {
        m.mouse.insert(MouseButton::Back, ButtonAction::Keys(vec!["KEY_LEFTCTRL".into(), "KEY_Z".into()]));
        m.keys.insert("KEY_F".into(), ButtonAction::Mouse(MouseButton::Right));
    });
    let mut r = Remapper::default();
    let events = [RawEvent::Button(MouseButton::Back, true), RawEvent::Button(MouseButton::Back, false), key(KeyCode::KEY_F, true)];
    assert_eq!(
        run(&mut r, &p, &events),
        [
            OutEvent::Key(KeyCode::KEY_LEFTCTRL, true),
            OutEvent::Key(KeyCode::KEY_Z, true),
            OutEvent::Key(KeyCode::KEY_Z, false),
            OutEvent::Key(KeyCode::KEY_LEFTCTRL, false),
            OutEvent::MouseButton(MouseButton::Right, true),
        ]
    );
}

#[test]
fn super_is_never_remapped_or_blocked() {
    let p = profile(|m| {
        m.other_keys = OtherKeys::Block;
        m.keys.insert("KEY_LEFTMETA".into(), ButtonAction::Keys(vec!["KEY_A".into()]));
    });
    let mut r = Remapper::default();
    assert_eq!(run(&mut r, &p, &[key(KeyCode::KEY_LEFTMETA, true)]), [OutEvent::Key(KeyCode::KEY_LEFTMETA, true)]);
}

#[test]
fn console_switches_pass_only_with_ctrl_and_alt() {
    let p = profile(|m| m.other_keys = OtherKeys::Block);
    let mut r = Remapper::default();
    assert!(run(&mut r, &p, &[key(KeyCode::KEY_F2, true), key(KeyCode::KEY_F2, false)]).is_empty());
    // Ctrl and Alt are blocked here, but they are physically down, so the switch still works.
    let modifiers = run(&mut r, &p, &[key(KeyCode::KEY_LEFTCTRL, true), key(KeyCode::KEY_RIGHTALT, true)]);
    assert!(modifiers.is_empty());
    assert_eq!(run(&mut r, &p, &[key(KeyCode::KEY_F2, true)]), [OutEvent::Key(KeyCode::KEY_F2, true)]);
}

#[test]
fn letting_go_undoes_what_the_press_did_even_if_the_profile_changed() {
    let mut r = Remapper::default();
    run(&mut r, &esdf(), &[key(KeyCode::KEY_E, true)]);
    let out = run(&mut r, &Profile::keyboard("empty"), &[key(KeyCode::KEY_E, false)]);
    assert_eq!(out, [OutEvent::Key(KeyCode::KEY_W, false)]);
}

#[test]
fn releasing_everything_lets_go_of_all_held_outputs() {
    let mut r = Remapper::default();
    run(&mut r, &esdf(), &[key(KeyCode::KEY_E, true), key(KeyCode::KEY_1, true)]);
    let mut out = Vec::new();
    r.release_all(&mut out);
    assert!(out.contains(&OutEvent::Key(KeyCode::KEY_W, false)) && out.contains(&OutEvent::Key(KeyCode::KEY_1, false)), "{out:?}");
    // The physical release afterwards has nothing left to undo.
    assert!(run(&mut r, &esdf(), &[key(KeyCode::KEY_E, false)]).is_empty());
}

#[test]
fn the_panic_chord_lets_go_and_reports_itself() {
    let mut r = Remapper::default();
    r.set_chord(&Config::default().panic_chord);
    let p = esdf();
    let mut out = Vec::new();
    for k in [KeyCode::KEY_E, KeyCode::KEY_RIGHTCTRL, KeyCode::KEY_LEFTALT, KeyCode::KEY_LEFTSHIFT] {
        assert!(!r.event(&p, key(k, true), &mut out));
    }
    out.clear();
    assert!(r.event(&p, key(KeyCode::KEY_ESC, true), &mut out));
    assert!(out.contains(&OutEvent::Key(KeyCode::KEY_W, false)), "{out:?}");
    assert!(out.contains(&OutEvent::Key(KeyCode::KEY_RIGHTCTRL, false)), "{out:?}");
}

#[test]
fn an_incomplete_chord_does_nothing() {
    let mut r = Remapper::default();
    r.set_chord(&Config::default().panic_chord);
    let mut out = Vec::new();
    let p = Profile::keyboard("p");
    assert!(!r.event(&p, key(KeyCode::KEY_LEFTCTRL, true), &mut out));
    assert!(!r.event(&p, key(KeyCode::KEY_ESC, true), &mut out));
}

#[test]
fn keys_press_pad_buttons_triggers_and_push_the_stick() {
    let p = Profile::keyboard_to_pad("p");
    let mut r = Remapper::default();
    let out = run(&mut r, &p, &[key(KeyCode::KEY_SPACE, true), RawEvent::Button(MouseButton::Left, true)]);
    assert_eq!(out, [OutEvent::PadButton(Button::South, true), OutEvent::PadAxis(Axis::RightTrigger, 1.0)]);

    let out = run(&mut r, &p, &[key(KeyCode::KEY_W, true)]);
    assert_eq!(pad_axis(&out, Axis::LeftY), Some(-1.0));
    // Two directions make a diagonal of the same length.
    let out = run(&mut r, &p, &[key(KeyCode::KEY_D, true)]);
    let (x, y) = (pad_axis(&out, Axis::LeftX).unwrap(), pad_axis(&out, Axis::LeftY).unwrap());
    assert!((x.hypot(y) - 1.0).abs() < 1e-5 && x > 0.0 && y < 0.0, "{x} {y}");
    let out = run(&mut r, &p, &[key(KeyCode::KEY_W, false), key(KeyCode::KEY_D, false)]);
    assert_eq!((pad_axis(&out, Axis::LeftX), pad_axis(&out, Axis::LeftY)), (Some(0.0), Some(0.0)));
}

#[test]
fn the_pad_template_blocks_other_keys() {
    let mut r = Remapper::default();
    assert!(run(&mut r, &Profile::keyboard_to_pad("p"), &[key(KeyCode::KEY_X, true)]).is_empty());
}

#[test]
fn mouse_movement_pushes_a_stick_that_fades_back() {
    let p = Profile::keyboard_to_pad("p");
    let mut r = Remapper::default();
    let out = run(&mut r, &p, &[RawEvent::Move(30, -60)]);
    assert!(!out.iter().any(|e| matches!(e, OutEvent::MouseMove(..))), "the pointer stays put: {out:?}");
    let (x, y) = (pad_axis(&out, Axis::RightX).unwrap(), pad_axis(&out, Axis::RightY).unwrap());
    assert!(x > 0.2 && y < -0.4, "{x} {y}");
    assert!(r.needs_tick(&p));

    let mut faded = Vec::new();
    for _ in 0..200 {
        r.tick(&p, 0.01, &mut faded);
    }
    assert_eq!((pad_axis(&faded, Axis::RightX), pad_axis(&faded, Axis::RightY)), (Some(0.0), Some(0.0)));
    assert!(!r.needs_tick(&p));
}

#[test]
fn a_big_move_saturates_the_stick_and_inversion_flips_it() {
    let mut p = Profile::keyboard_to_pad("p");
    p.keyboard.motion.invert_y = true;
    let mut r = Remapper::default();
    let out = run(&mut r, &p, &[RawEvent::Move(0, 5000)]);
    assert_eq!(pad_axis(&out, Axis::RightY), Some(-1.0));
}

#[test]
fn the_pointer_can_be_scaled_and_inverted() {
    let p = profile(|m| m.motion.pointer_scale = 0.5);
    let mut r = Remapper::default();
    let out = run(&mut r, &p, &[RawEvent::Move(3, 4), RawEvent::Move(3, 0)]);
    // 1.5 + 1.5 = 3 across two moves, with the half carried over.
    assert_eq!(out, [OutEvent::MouseMove(1, 2), OutEvent::MouseMove(2, 0)]);
    let p = profile(|m| m.motion.invert_x = true);
    assert_eq!(run(&mut Remapper::default(), &p, &[RawEvent::Move(5, 2)]), [OutEvent::MouseMove(-5, 2)]);
}

#[test]
fn the_pointer_is_untouched_by_default() {
    let mut r = Remapper::default();
    assert_eq!(run(&mut r, &Profile::keyboard("p"), &[RawEvent::Move(7, -3)]), [OutEvent::MouseMove(7, -3)]);
    assert!(!r.needs_tick(&Profile::keyboard("p")));
}

#[test]
fn wheel_notches_can_press_keys_and_the_rest_scrolls_on() {
    let p = profile(|m| {
        m.wheel.insert(WheelDirection::Up, ButtonAction::Keys(vec!["KEY_PAGEUP".into()]));
    });
    let mut r = Remapper::default();
    let out = run(&mut r, &p, &[RawEvent::Wheel { vertical: 120, horizontal: 0 }]);
    assert_eq!(out, [OutEvent::Key(KeyCode::KEY_PAGEUP, true), OutEvent::Key(KeyCode::KEY_PAGEUP, false)]);
    // Scrolling down has no mapping and goes on; small steps add up to a notch.
    assert_eq!(run(&mut r, &p, &[RawEvent::Wheel { vertical: -120, horizontal: 0 }]), [OutEvent::Wheel { vertical: -120, horizontal: 0 }]);
    assert!(run(&mut r, &p, &[RawEvent::Wheel { vertical: 40, horizontal: 0 }; 2]).is_empty());
    assert_eq!(run(&mut r, &p, &[RawEvent::Wheel { vertical: 40, horizontal: 0 }]).len(), 2);
}

#[test]
fn moving_the_mouse_a_way_can_hold_a_key() {
    let p = profile(|m| {
        m.motion_buttons.insert(MotionDirection::Left, ButtonAction::Keys(vec!["KEY_Q".into()]));
    });
    let mut r = Remapper::default();
    let out = run(&mut r, &p, &[RawEvent::Move(-80, 0)]);
    assert!(out.contains(&OutEvent::MouseMove(-80, 0)) && out.contains(&OutEvent::Key(KeyCode::KEY_Q, true)), "{out:?}");
    let mut later = Vec::new();
    for _ in 0..100 {
        r.tick(&p, 0.01, &mut later);
    }
    assert_eq!(later, [OutEvent::Key(KeyCode::KEY_Q, false)]);
}

fn at(t0: Instant, ms: u64) -> Instant {
    t0 + std::time::Duration::from_millis(ms)
}

fn key_at(r: &mut Remapper, p: &Profile, k: KeyCode, down: bool, t: Instant) -> Vec<OutEvent> {
    let mut out = Vec::new();
    r.event_at(p, key(k, down), t, &mut out);
    out
}

fn fire_timers(r: &mut Remapper, p: &Profile, t: Instant) -> Vec<OutEvent> {
    let mut out = Vec::new();
    r.timers(p, t, &mut out);
    out
}

fn tap(k: KeyCode) -> Vec<OutEvent> {
    vec![OutEvent::Key(k, true), OutEvent::Key(k, false)]
}

#[test]
fn a_key_can_have_a_double_tap_and_a_long_press() {
    use crate::config::Gestures;
    let mut p = Profile::keyboard("p");
    p.keyboard.gestures.insert(
        "KEY_CAPSLOCK".into(),
        Gestures {
            double_tap: Some(ButtonAction::Keys(vec!["KEY_ESC".into()])),
            long_press: Some(ButtonAction::Keys(vec!["KEY_F5".into()])),
            ..Gestures::default()
        },
    );
    let t0 = Instant::now();
    let mut r = Remapper::default();

    // Two quick taps: the double tap, once the final tap holds.
    assert!(key_at(&mut r, &p, KeyCode::KEY_CAPSLOCK, true, at(t0, 0)).is_empty());
    assert!(key_at(&mut r, &p, KeyCode::KEY_CAPSLOCK, false, at(t0, 50)).is_empty());
    let out = key_at(&mut r, &p, KeyCode::KEY_CAPSLOCK, true, at(t0, 100));
    assert_eq!(out, [OutEvent::Key(KeyCode::KEY_ESC, true)]);
    assert_eq!(key_at(&mut r, &p, KeyCode::KEY_CAPSLOCK, false, at(t0, 150)), [OutEvent::Key(KeyCode::KEY_ESC, false)]);

    // One tap that nothing follows is a normal press of the key.
    key_at(&mut r, &p, KeyCode::KEY_CAPSLOCK, true, at(t0, 1000));
    key_at(&mut r, &p, KeyCode::KEY_CAPSLOCK, false, at(t0, 1050));
    assert_eq!(fire_timers(&mut r, &p, at(t0, 1400)), tap(KeyCode::KEY_CAPSLOCK));

    // Held long enough: the long press, held until release.
    key_at(&mut r, &p, KeyCode::KEY_CAPSLOCK, true, at(t0, 2000));
    assert_eq!(fire_timers(&mut r, &p, at(t0, 2700)), [OutEvent::Key(KeyCode::KEY_F5, true)]);
    assert_eq!(key_at(&mut r, &p, KeyCode::KEY_CAPSLOCK, false, at(t0, 2800)), [OutEvent::Key(KeyCode::KEY_F5, false)]);
}

#[test]
fn keys_pressed_together_act_as_a_combo() {
    use crate::config::KeyboardCombo;
    let mut p = Profile::keyboard("p");
    p.keyboard.combos.push(KeyboardCombo {
        inputs: vec!["KEY_J".into(), "BTN_LEFT".into()],
        action: ButtonAction::Keys(vec!["KEY_F9".into()]),
    });
    let t0 = Instant::now();
    let mut r = Remapper::default();
    assert!(key_at(&mut r, &p, KeyCode::KEY_J, true, at(t0, 0)).is_empty());
    let mut out = Vec::new();
    r.event_at(&p, RawEvent::Button(MouseButton::Left, true), at(t0, 20), &mut out);
    assert_eq!(out, [OutEvent::Key(KeyCode::KEY_F9, true)]);
    assert_eq!(key_at(&mut r, &p, KeyCode::KEY_J, false, at(t0, 90)), [OutEvent::Key(KeyCode::KEY_F9, false)]);

    // Alone, the key acts after the combo window.
    key_at(&mut r, &p, KeyCode::KEY_J, true, at(t0, 1000));
    assert_eq!(fire_timers(&mut r, &p, at(t0, 1100)), [OutEvent::Key(KeyCode::KEY_J, true)]);
    assert_eq!(key_at(&mut r, &p, KeyCode::KEY_J, false, at(t0, 1200)), [OutEvent::Key(KeyCode::KEY_J, false)]);
}

#[test]
fn toggle_turbo_and_next_profile_work_on_keys() {
    let p = profile(|m| {
        m.keys.insert("KEY_G".into(), ButtonAction::toggle(ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into()])));
        m.keys.insert("KEY_N".into(), ButtonAction::NextProfile);
    });
    let mut r = Remapper::default();
    assert_eq!(run(&mut r, &p, &[key(KeyCode::KEY_G, true), key(KeyCode::KEY_G, false)]), [OutEvent::Key(KeyCode::KEY_LEFTSHIFT, true)]);
    assert_eq!(run(&mut r, &p, &[key(KeyCode::KEY_G, true)]), [OutEvent::Key(KeyCode::KEY_LEFTSHIFT, false)]);
    assert!(!r.take_switch());
    run(&mut r, &p, &[key(KeyCode::KEY_N, true)]);
    assert!(r.take_switch());
    assert!(!r.take_switch());
}

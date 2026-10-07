use super::*;
use crate::config::{Config, Profile};

fn run(remapper: &mut Remapper, map: &KeyboardMap, events: &[RawEvent]) -> Vec<OutEvent> {
    let mut out = Vec::new();
    for ev in events {
        remapper.event(map, *ev, &mut out);
    }
    out
}

fn key(k: KeyCode, down: bool) -> RawEvent {
    RawEvent::Key(k, down)
}

fn esdf() -> KeyboardMap {
    Profile::wasd_to_esdf("p").keyboard
}

#[test]
fn a_remapped_key_sends_its_target_and_releases_it() {
    let mut r = Remapper::default();
    let out = run(&mut r, &esdf(), &[key(KeyCode::KEY_W, true), key(KeyCode::KEY_W, false)]);
    assert_eq!(out, [OutEvent::Key(KeyCode::KEY_E, true), OutEvent::Key(KeyCode::KEY_E, false)]);
}

#[test]
fn the_old_keys_are_disabled_and_the_rest_pass_through() {
    let mut r = Remapper::default();
    let out = run(&mut r, &esdf(), &[key(KeyCode::KEY_E, true), key(KeyCode::KEY_E, false), key(KeyCode::KEY_Q, true)]);
    assert_eq!(out, [OutEvent::Key(KeyCode::KEY_Q, true)]);
}

#[test]
fn blocking_swallows_unmapped_keys_and_buttons_but_not_movement() {
    let mut map = esdf();
    map.other_keys = OtherKeys::Block;
    let mut r = Remapper::default();
    let events = [key(KeyCode::KEY_Q, true), RawEvent::Button(MouseButton::Left, true), RawEvent::Move(3, -2)];
    assert_eq!(run(&mut r, &map, &events), [OutEvent::MouseMove(3, -2)]);
}

#[test]
fn a_mouse_button_can_become_a_key_and_a_key_a_mouse_button() {
    let mut map = KeyboardMap::default();
    map.mouse.insert(MouseButton::Back, ButtonAction::Keys(vec!["KEY_LEFTCTRL".into(), "KEY_Z".into()]));
    map.keys.insert("KEY_F".into(), ButtonAction::Mouse(MouseButton::Right));
    let mut r = Remapper::default();
    let events = [RawEvent::Button(MouseButton::Back, true), RawEvent::Button(MouseButton::Back, false), key(KeyCode::KEY_F, true)];
    assert_eq!(
        run(&mut r, &map, &events),
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
    let mut map = KeyboardMap { other_keys: OtherKeys::Block, ..KeyboardMap::default() };
    map.keys.insert("KEY_LEFTMETA".into(), ButtonAction::Keys(vec!["KEY_A".into()]));
    let mut r = Remapper::default();
    assert_eq!(run(&mut r, &map, &[key(KeyCode::KEY_LEFTMETA, true)]), [OutEvent::Key(KeyCode::KEY_LEFTMETA, true)]);
}

#[test]
fn console_switches_pass_only_with_ctrl_and_alt() {
    let map = KeyboardMap { other_keys: OtherKeys::Block, ..KeyboardMap::default() };
    let mut r = Remapper::default();
    assert!(run(&mut r, &map, &[key(KeyCode::KEY_F2, true), key(KeyCode::KEY_F2, false)]).is_empty());
    // Ctrl and Alt are blocked here, but they are physically down, so the switch still works.
    let modifiers = run(&mut r, &map, &[key(KeyCode::KEY_LEFTCTRL, true), key(KeyCode::KEY_RIGHTALT, true)]);
    assert!(modifiers.is_empty());
    assert_eq!(run(&mut r, &map, &[key(KeyCode::KEY_F2, true)]), [OutEvent::Key(KeyCode::KEY_F2, true)]);
}

#[test]
fn letting_go_undoes_what_the_press_did_even_if_the_profile_changed() {
    let mut r = Remapper::default();
    run(&mut r, &esdf(), &[key(KeyCode::KEY_W, true)]);
    let mut out = Vec::new();
    r.event(&KeyboardMap::default(), key(KeyCode::KEY_W, false), &mut out);
    assert_eq!(out, [OutEvent::Key(KeyCode::KEY_E, false)]);
}

#[test]
fn releasing_everything_lets_go_of_all_held_outputs() {
    let mut r = Remapper::default();
    run(&mut r, &esdf(), &[key(KeyCode::KEY_W, true), key(KeyCode::KEY_Q, true)]);
    let mut out = Vec::new();
    r.release_all(&mut out);
    out.sort_by_key(|e| format!("{e:?}"));
    assert_eq!(out, [OutEvent::Key(KeyCode::KEY_E, false), OutEvent::Key(KeyCode::KEY_Q, false)]);
    // The physical release afterwards has nothing left to undo.
    assert!(run(&mut r, &esdf(), &[key(KeyCode::KEY_W, false)]).is_empty());
}

#[test]
fn the_panic_chord_lets_go_and_reports_itself() {
    let mut r = Remapper::default();
    r.set_chord(&Config::default().panic_chord);
    let map = esdf();
    let mut out = Vec::new();
    for k in [KeyCode::KEY_W, KeyCode::KEY_RIGHTCTRL, KeyCode::KEY_LEFTALT, KeyCode::KEY_LEFTSHIFT] {
        assert!(!r.event(&map, key(k, true), &mut out));
    }
    out.clear();
    assert!(r.event(&map, key(KeyCode::KEY_ESC, true), &mut out));
    assert!(out.contains(&OutEvent::Key(KeyCode::KEY_E, false)), "{out:?}");
    assert!(out.contains(&OutEvent::Key(KeyCode::KEY_RIGHTCTRL, false)), "{out:?}");
}

#[test]
fn an_incomplete_chord_does_nothing() {
    let mut r = Remapper::default();
    r.set_chord(&Config::default().panic_chord);
    let mut out = Vec::new();
    assert!(!r.event(&KeyboardMap::default(), key(KeyCode::KEY_LEFTCTRL, true), &mut out));
    assert!(!r.event(&KeyboardMap::default(), key(KeyCode::KEY_ESC, true), &mut out));
}

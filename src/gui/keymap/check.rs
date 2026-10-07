//! What saving rejects in a keyboard profile.

use super::*;
use crate::config::is_reserved_key;
use crate::engine::{RawInput, input_name, parse_input};

/// The first thing saving would reject in a keyboard profile's mappings.
pub(in crate::gui) fn keyboard_problem(map: &KeyboardMap) -> Option<String> {
    let known = |k: &str| KeyCode::from_str(k).is_ok();
    if let Some(bad) = map.keys.keys().find(|k| !known(k)) {
        return Some(format!("unknown key {:?}", short_key(bad)));
    }
    if let Some(key) = map.keys.keys().chain(map.gestures.keys()).find(|k| is_reserved_key(k)) {
        return Some(format!("{} can't be remapped", keyboard::label(key)));
    }
    if let Some(bad) = map.gestures.keys().find(|n| parse_input(n).is_none()) {
        return Some(format!("unknown key or button {:?} has gestures", short_key(bad)));
    }
    for c in &map.combos {
        if c.inputs.len() < 2 {
            return Some("a combo needs at least two keys or buttons".into());
        }
        if let Some(bad) = c.inputs.iter().find(|n| parse_input(n).is_none() || is_reserved_key(n)) {
            return Some(format!("{} can't be in a combo", keyboard::label(bad)));
        }
    }
    let inputs = map
        .keys
        .iter()
        .map(|(k, a)| (kb_input_name(&KbInput::Key(k.clone())), a))
        .chain(map.mouse.iter().map(|(b, a)| (kb_input_name(&KbInput::Mouse(*b)), a)))
        .chain(map.wheel.iter().map(|(d, a)| (kb_input_name(&KbInput::Wheel(*d)), a)))
        .chain(map.motion_buttons.iter().map(|(d, a)| (kb_input_name(&KbInput::Motion(*d)), a)))
        .chain(map.gestures.iter().flat_map(|(n, g)| {
            GestureKind::ALL.into_iter().filter_map(|k| g.get(k)).map(move |a| (format!("{} (gesture)", input_label(n)), a))
        }))
        .chain(map.combos.iter().map(|c| ("A combo".to_string(), &c.action)));
    inputs.into_iter().find_map(|(name, action)| action_problem_for(&name, action, known))
}

/// What a key or mouse button is called in a gesture or combo (`KEY_W`, `BTN_LEFT`).
pub(super) fn input_label(name: &str) -> String {
    match MouseButton::ALL.into_iter().find(|b| input_name(RawInput::Button(*b)).as_deref() == Some(name)) {
        Some(b) => format!("{b} click"),
        None => keyboard::label(name),
    }
}

/// What's wrong with one input's action, with the input's name in front.
fn action_problem_for(name: &str, action: &ButtonAction, known: impl Fn(&str) -> bool) -> Option<String> {
    let mut problem = None;
    action.walk(&mut |a| {
        if problem.is_some() {
            return;
        }
        problem = match a {
            ButtonAction::Keys(keys) if keys.is_empty() => Some(format!("{name} has no key to press")),
            ButtonAction::Keys(keys) => keys
                .iter()
                .find(|k| !known(k))
                .map(|bad| format!("unknown key {:?}", short_key(bad)))
                .or_else(|| keys.iter().find(|k| is_reserved_key(k)).map(|key| format!("{} can't be pressed by a remap", keyboard::label(key)))),
            ButtonAction::OpenMenu(_) | ButtonAction::ToggleOverlay | ButtonAction::ToggleNumpad => {
                Some(format!("{name} opens a menu or on-screen keyboard, which a controller drives; keyboard profiles can't"))
            }
            _ => None,
        };
    });
    problem
}

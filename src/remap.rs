//! Remaps the physical keyboard and mouse through the active keyboard profile.
//!
//! The daemon grabs the devices and feeds their events here as [`RawEvent`]s; what comes out
//! goes to the virtual keyboard and mouse. Nothing here touches a device, so it is all testable.

use std::collections::{HashMap, HashSet};

use evdev::KeyCode;

use crate::{
    config::{ButtonAction, KeyboardMap, MouseButton, OtherKeys, default_panic_chord, is_reserved_key},
    engine::parse_key,
    output::OutEvent,
};

mod device;

pub use device::{classify, read_device};

/// One thing the keyboard or mouse did. Key repeats are left out; the desktop repeats keys
/// itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawEvent {
    Key(KeyCode, bool),
    Button(MouseButton, bool),
    Move(i32, i32),
    /// High-resolution wheel units (120 = one notch).
    Wheel { vertical: i32, horizontal: i32 },
}

/// What a held input was turned into, so letting go undoes exactly that, whatever the profile
/// says by then.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Source {
    Key(KeyCode),
    Button(MouseButton),
}

#[derive(Default)]
pub struct Remapper {
    /// Keys physically down, for the panic chord and the console-switch exception.
    down: HashSet<KeyCode>,
    held: HashMap<Source, Vec<OutEvent>>,
    chord: Vec<KeyCode>,
}

/// The other half of a modifier, so either Ctrl, Alt or Shift satisfies a chord.
fn sibling(key: KeyCode) -> Option<KeyCode> {
    Some(match key {
        KeyCode::KEY_LEFTCTRL => KeyCode::KEY_RIGHTCTRL,
        KeyCode::KEY_RIGHTCTRL => KeyCode::KEY_LEFTCTRL,
        KeyCode::KEY_LEFTALT => KeyCode::KEY_RIGHTALT,
        KeyCode::KEY_RIGHTALT => KeyCode::KEY_LEFTALT,
        KeyCode::KEY_LEFTSHIFT => KeyCode::KEY_RIGHTSHIFT,
        KeyCode::KEY_RIGHTSHIFT => KeyCode::KEY_LEFTSHIFT,
        _ => return None,
    })
}

fn is_function_key(key: KeyCode) -> bool {
    (KeyCode::KEY_F1.0..=KeyCode::KEY_F10.0).contains(&key.0) || matches!(key, KeyCode::KEY_F11 | KeyCode::KEY_F12)
}

/// The outputs an action presses, in order. Only keys and mouse buttons can be remapped to so far.
fn presses(action: &ButtonAction) -> Vec<OutEvent> {
    match action {
        ButtonAction::Keys(names) => names.iter().filter_map(|n| parse_key(n)).map(|k| OutEvent::Key(k, true)).collect(),
        ButtonAction::Mouse(b) => vec![OutEvent::MouseButton(*b, true)],
        ButtonAction::Multi(actions) => actions.iter().flat_map(presses).collect(),
        _ => Vec::new(),
    }
}

fn released(ev: &OutEvent) -> OutEvent {
    match ev {
        OutEvent::Key(k, _) => OutEvent::Key(*k, false),
        OutEvent::MouseButton(b, _) => OutEvent::MouseButton(*b, false),
        other => other.clone(),
    }
}

impl Remapper {
    pub fn set_chord(&mut self, chord: &[String]) {
        self.chord = chord.iter().filter_map(|n| parse_key(n)).collect();
        // Never leave the escape hatch empty.
        if self.chord.is_empty() {
            self.chord = default_panic_chord().iter().filter_map(|n| parse_key(n)).collect();
        }
    }

    fn is_down(&self, key: KeyCode) -> bool {
        self.down.contains(&key) || sibling(key).is_some_and(|s| self.down.contains(&s))
    }

    /// Handles one event. True when it completed the panic chord: everything is already
    /// released, and the caller should turn remapping off.
    pub fn event(&mut self, map: &KeyboardMap, ev: RawEvent, out: &mut Vec<OutEvent>) -> bool {
        match ev {
            RawEvent::Move(dx, dy) => out.push(OutEvent::MouseMove(dx, dy)),
            RawEvent::Wheel { vertical, horizontal } => out.push(OutEvent::Wheel { vertical, horizontal }),
            RawEvent::Key(key, pressed) => {
                if pressed {
                    self.down.insert(key);
                } else {
                    self.down.remove(&key);
                }
                if pressed && self.chord.contains(&key) && self.chord.iter().all(|k| self.is_down(*k)) {
                    self.release_all(out);
                    return true;
                }
                let presses = pressed.then(|| self.key_outputs(map, key));
                self.input(Source::Key(key), presses, out);
            }
            RawEvent::Button(button, pressed) => {
                let presses = pressed.then(|| Self::lookup(map.mouse.get(&button), map.other_keys, vec![OutEvent::MouseButton(button, true)]));
                self.input(Source::Button(button), presses, out);
            }
        }
        false
    }

    /// A source went down (with what it presses) or up (`None`).
    fn input(&mut self, source: Source, presses: Option<Vec<OutEvent>>, out: &mut Vec<OutEvent>) {
        let Some(presses) = presses else {
            if let Some(held) = self.held.remove(&source) {
                out.extend(held.iter().rev().map(released));
            }
            return;
        };
        if self.held.contains_key(&source) {
            return;
        }
        out.extend(presses.iter().cloned());
        self.held.insert(source, presses);
    }

    /// What a key press turns into: itself when reserved, else what the profile says.
    fn key_outputs(&self, map: &KeyboardMap, key: KeyCode) -> Vec<OutEvent> {
        let name = format!("{key:?}");
        let console_switch = is_function_key(key) && self.is_down(KeyCode::KEY_LEFTCTRL) && self.is_down(KeyCode::KEY_LEFTALT);
        if is_reserved_key(&name) || console_switch {
            return vec![OutEvent::Key(key, true)];
        }
        Self::lookup(map.keys.get(&name), map.other_keys, vec![OutEvent::Key(key, true)])
    }

    fn lookup(mapped: Option<&ButtonAction>, other: OtherKeys, itself: Vec<OutEvent>) -> Vec<OutEvent> {
        match (mapped, other) {
            (Some(action), _) => presses(action),
            (None, OtherKeys::Pass) => itself,
            (None, OtherKeys::Block) => Vec::new(),
        }
    }

    /// Lets go of everything being held, and forgets which keys are down (on a profile
    /// change, or when the devices are released).
    pub fn release_all(&mut self, out: &mut Vec<OutEvent>) {
        for (_, presses) in self.held.drain() {
            out.extend(presses.iter().rev().map(released));
        }
        self.down.clear();
    }
}

#[cfg(test)]
mod tests;

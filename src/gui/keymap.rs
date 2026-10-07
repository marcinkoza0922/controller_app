//! The keyboard profile editor: which keys and mouse inputs become what.

use super::*;
pub(super) use check::keyboard_problem;
use crate::config::{GestureKind, KeyboardMap, MotionDirection, MotionTarget, is_reserved_key};

mod check;
mod extras;
mod view;

/// An input of a keyboard profile.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum KbInput {
    /// A key, by evdev name (`KEY_W`).
    Key(String),
    Mouse(MouseButton),
    /// One wheel notch.
    Wheel(WheelDirection),
    /// The mouse moving that way.
    Motion(MotionDirection),
}

/// What a keyboard profile's input can do: anything a gamepad button can, except what a
/// controller drives on screen (menus, the on-screen keyboard and numpad).
pub(super) const KB_ACTION_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Gamepad,
    ActionKind::PadTrigger,
    ActionKind::Wheel,
    ActionKind::NextProfile,
    ActionKind::Toggle,
    ActionKind::Turbo,
    ActionKind::Macro,
    ActionKind::Info,
    ActionKind::Log,
    ActionKind::Layer,
    ActionKind::Multiple,
];

/// What mouse movement does, as a dropdown choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MotionChoice {
    Pointer,
    Stick(Stick),
}

impl MotionChoice {
    const ALL: [MotionChoice; 3] = [MotionChoice::Pointer, MotionChoice::Stick(Stick::Left), MotionChoice::Stick(Stick::Right)];
}

impl fmt::Display for MotionChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MotionChoice::Pointer => "Moves the pointer",
            MotionChoice::Stick(Stick::Left) => "Pushes the left stick",
            MotionChoice::Stick(Stick::Right) => "Pushes the right stick",
        })
    }
}

/// One change to how mouse movement is handled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum MotionEdit {
    Target(MotionChoice),
    PointerScale(f32),
    Counts(f32),
    DecayMs(f32),
    InvertX(bool),
    InvertY(bool),
    /// Of the stick the mouse pushes.
    Deadzone(f32),
}

fn map_action<'a>(map: &'a mut KeyboardMap, input: &KbInput) -> Option<&'a mut ButtonAction> {
    match input {
        KbInput::Key(k) => map.keys.get_mut(k),
        KbInput::Mouse(b) => map.mouse.get_mut(b),
        KbInput::Wheel(d) => map.wheel.get_mut(d),
        KbInput::Motion(d) => map.motion_buttons.get_mut(d),
    }
}

/// The action a keyboard profile key field points into.
fn field_action<'a>(map: &'a mut KeyboardMap, field: &KeyField) -> Option<&'a mut ButtonAction> {
    match field {
        KeyField::KbTarget(input, path) => action_at(map_action(map, input)?, path),
        KeyField::KbGesture(name, kind, path) => action_at(map.gestures.get_mut(name)?.slot(*kind).as_mut()?, path),
        KeyField::KbCombo(i, path) => action_at(&mut map.combos.get_mut(*i)?.action, path),
        _ => None,
    }
}

fn remove_input(map: &mut KeyboardMap, input: &KbInput) {
    match input {
        KbInput::Key(k) => drop(map.keys.remove(k)),
        KbInput::Mouse(b) => drop(map.mouse.remove(b)),
        KbInput::Wheel(d) => drop(map.wheel.remove(d)),
        KbInput::Motion(d) => drop(map.motion_buttons.remove(d)),
    }
}

/// Moves a mapping from `old` to `new` in `map`, unless `new` already has one.
fn move_mapping<K: Ord + Copy>(map: &mut std::collections::BTreeMap<K, ButtonAction>, old: K, new: K) {
    if old != new
        && !map.contains_key(&new)
        && let Some(action) = map.remove(&old)
    {
        map.insert(new, action);
    }
}

impl App {
    pub(super) fn update_keymap(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SetPanicChord(chord) => self.config.panic_chord = chord,
            Message::KbOtherKeys(other) => self.edit_keymap(|map| map.other_keys = other),
            Message::KbMotion(edit) => self.edit_motion(edit),
            Message::KbAddMouse => self.edit_keymap(|map| {
                if let Some(free) = MouseButton::ALL.into_iter().find(|b| !map.mouse.contains_key(b)) {
                    map.mouse.insert(free, ButtonAction::Disabled);
                }
            }),
            Message::KbAddWheel => self.edit_keymap(|map| {
                if let Some(free) = WheelDirection::ALL.into_iter().find(|d| !map.wheel.contains_key(d)) {
                    map.wheel.insert(free, ButtonAction::Disabled);
                }
            }),
            Message::KbAddMotion => self.edit_keymap(|map| {
                if let Some(free) = MotionDirection::ALL.into_iter().find(|d| !map.motion_buttons.contains_key(d)) {
                    map.motion_buttons.insert(free, ButtonAction::Disabled);
                }
            }),
            Message::KbRemove(input) => self.edit_keymap(|map| remove_input(map, &input)),
            Message::KbSetAction(input, new) => self.edit_keymap(|map| {
                if let Some(action) = map_action(map, &input) {
                    *action = new;
                }
            }),
            Message::KbMouseSource(old, new) => self.move_source(&KbInput::Mouse(new), |map| move_mapping(&mut map.mouse, old, new)),
            Message::KbWheelSource(old, new) => self.move_source(&KbInput::Wheel(new), |map| move_mapping(&mut map.wheel, old, new)),
            Message::KbMotionSource(old, new) => self.move_source(&KbInput::Motion(new), |map| move_mapping(&mut map.motion_buttons, old, new)),
            other => {
                if let Some(other) = self.update_extras(other) {
                    return self.update_actions(other);
                }
            }
        }
        Task::none()
    }

    fn edit_keymap(&mut self, edit: impl FnOnce(&mut KeyboardMap)) {
        if let Some(p) = self.profile_mut() {
            edit(&mut p.keyboard);
        }
    }

    /// Moves a mapping to another input, saying so if that one is taken.
    fn move_source(&mut self, new: &KbInput, edit: impl FnOnce(&mut KeyboardMap)) {
        let taken = self.profile_mut().is_some_and(|p| {
            let before = p.keyboard.clone();
            edit(&mut p.keyboard);
            p.keyboard == before
        });
        if taken {
            self.message = Some((format!("{} is already remapped.", kb_input_name(new)), true));
        }
    }

    fn edit_motion(&mut self, edit: MotionEdit) {
        let Some(p) = self.profile_mut() else { return };
        let motion = &mut p.keyboard.motion;
        match edit {
            MotionEdit::Target(MotionChoice::Pointer) => motion.target = MotionTarget::Pointer,
            MotionEdit::Target(MotionChoice::Stick(s)) => motion.target = MotionTarget::Stick(s),
            MotionEdit::PointerScale(v) => motion.pointer_scale = v,
            MotionEdit::Counts(v) => motion.counts = v,
            MotionEdit::DecayMs(v) => motion.decay_ms = v as u32,
            MotionEdit::InvertX(on) => motion.invert_x = on,
            MotionEdit::InvertY(on) => motion.invert_y = on,
            MotionEdit::Deadzone(v) => {
                if let MotionTarget::Stick(s) = motion.target {
                    p.stick_mut(s).deadzone = v;
                }
            }
        }
    }

    /// Writes what the on-screen keyboard picked into a keyboard profile.
    pub(super) fn apply_keymap_keys(&mut self, field: &KeyField, keys: Vec<String>) {
        match field {
            KeyField::PanicChord => self.config.panic_chord = keys,
            KeyField::KbSource(KbInput::Key(old)) => {
                let Some(new) = keys.into_iter().next().filter(|k| k != old) else { return };
                if let Some(problem) = self.new_source_problem(&new) {
                    self.message = Some((problem, true));
                    return;
                }
                self.edit_keymap(|map| {
                    if let Some(action) = map.keys.remove(old) {
                        map.keys.insert(new, action);
                    }
                });
            }
            KeyField::KbNew => {
                let Some(new) = keys.into_iter().next() else { return };
                if let Some(problem) = self.new_source_problem(&new) {
                    self.message = Some((problem, true));
                    return;
                }
                self.edit_keymap(|map| drop(map.keys.insert(new, ButtonAction::Keys(Vec::new()))));
            }
            KeyField::KbGestureNew => {
                let Some(new) = keys.into_iter().next().filter(|k| !is_reserved_key(k)) else { return };
                self.edit_keymap(|map| {
                    map.gestures.entry(new).or_default();
                });
            }
            KeyField::KbComboKeys(i) => self.edit_keymap(|map| {
                if let Some(combo) = map.combos.get_mut(*i) {
                    let mice: Vec<String> = combo.inputs.iter().filter(|n| n.starts_with("BTN_")).cloned().collect();
                    combo.inputs = keys.into_iter().filter(|k| !is_reserved_key(k)).chain(mice).collect();
                }
            }),
            nested => self.edit_keymap(|map| {
                if let Some(action) = field_action(map, nested) {
                    *action = ButtonAction::Keys(keys);
                }
            }),
        }
    }

    /// Why `key` can't be remapped now, if it can't.
    fn new_source_problem(&self, key: &str) -> Option<String> {
        if is_reserved_key(key) {
            return Some(format!("{} can't be remapped: it always works as normal.", keyboard::label(key)));
        }
        let taken = self.profile().is_some_and(|p| p.keyboard.keys.contains_key(key));
        taken.then(|| format!("{} is already remapped.", keyboard::label(key)))
    }
}

pub(super) fn kb_input_name(input: &KbInput) -> String {
    match input {
        KbInput::Key(k) => keyboard::label(k),
        KbInput::Mouse(b) => format!("The {b} mouse button"),
        KbInput::Wheel(d) => d.to_string(),
        KbInput::Motion(d) => d.to_string(),
    }
}

#[cfg(test)]
mod tests;

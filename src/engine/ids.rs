//! The inputs gestures and combos are detected for: gamepad buttons, and the keys and mouse
//! buttons of a keyboard profile. This is where an input's mappings are looked up.

use std::borrow::Cow;

use evdev::KeyCode;

use crate::config::{Button, ButtonAction, Gestures, MotionDirection, MouseButton, OtherKeys, Profile, WheelDirection};

/// A keyboard or mouse input, which a keyboard profile gives actions to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RawInput {
    Key(KeyCode),
    Button(MouseButton),
    Wheel(WheelDirection),
    Motion(MotionDirection),
}

/// Any input with its own action: a gamepad button, or a key or mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Id {
    Pad(Button),
    Raw(RawInput),
}

/// The name a key or mouse button has in a keyboard profile's gestures and combos (`KEY_W`,
/// `BTN_LEFT`); `None` for inputs that can't have either.
pub fn input_name(input: RawInput) -> Option<String> {
    match input {
        RawInput::Key(k) => Some(format!("{k:?}")),
        RawInput::Button(b) => Some(mouse_name(b).to_string()),
        RawInput::Wheel(_) | RawInput::Motion(_) => None,
    }
}

fn mouse_name(b: MouseButton) -> &'static str {
    match b {
        MouseButton::Left => "BTN_LEFT",
        MouseButton::Right => "BTN_RIGHT",
        MouseButton::Middle => "BTN_MIDDLE",
        MouseButton::Back => "BTN_SIDE",
        MouseButton::Forward => "BTN_EXTRA",
    }
}

/// The input a name in a keyboard profile's gestures and combos stands for.
pub fn parse_input(name: &str) -> Option<RawInput> {
    MouseButton::ALL
        .into_iter()
        .find(|b| mouse_name(*b) == name)
        .map(RawInput::Button)
        .or_else(|| name.starts_with("KEY_").then(|| name.parse().ok().map(RawInput::Key)).flatten())
}

impl Profile {
    /// What `id` does on its own. A key without a mapping does what it always does, or nothing
    /// if the profile blocks unmapped keys.
    pub(super) fn own_action(&self, id: Id) -> Cow<'_, ButtonAction> {
        let Id::Raw(input) = id else {
            let Id::Pad(b) = id else { unreachable!() };
            return Cow::Borrowed(self.button(b));
        };
        let map = &self.keyboard;
        let mapped = match input {
            RawInput::Key(k) => map.keys.get(&format!("{k:?}")),
            RawInput::Button(b) => map.mouse.get(&b),
            RawInput::Wheel(d) => map.wheel.get(&d),
            RawInput::Motion(d) => map.motion_buttons.get(&d),
        };
        match (mapped, map.other_keys, input) {
            (Some(action), _, _) => Cow::Borrowed(action),
            (None, OtherKeys::Pass, RawInput::Key(k)) => Cow::Owned(ButtonAction::Keys(vec![format!("{k:?}")])),
            (None, OtherKeys::Pass, RawInput::Button(b)) => Cow::Owned(ButtonAction::Mouse(b)),
            _ => Cow::Borrowed(&ButtonAction::Disabled),
        }
    }

    /// The double-tap, triple-tap and long-press actions of `id`, if it has any.
    pub(super) fn gestures_of(&self, id: Id) -> Option<&Gestures> {
        match id {
            Id::Pad(b) => self.gestures(b),
            Id::Raw(input) => self.keyboard.gestures.get(&input_name(input)?).filter(|g| !g.is_empty()),
        }
    }

    /// Whether `id` belongs to a combo, so its own action is held back briefly.
    pub(super) fn in_combo_of(&self, id: Id) -> bool {
        match id {
            Id::Pad(b) => self.in_combo(b),
            Id::Raw(input) => input_name(input).is_some_and(|name| {
                self.keyboard.combos.iter().any(|c| c.inputs.len() >= 2 && c.inputs.contains(&name))
            }),
        }
    }

    /// Every combo of two or more inputs: gamepad buttons and keyboard profile inputs.
    pub(super) fn combos_of(&self) -> Vec<(Vec<Id>, &ButtonAction)> {
        let pad = self.combos.iter().filter(|c| c.buttons.len() >= 2).map(|c| (c.buttons.iter().map(|b| Id::Pad(*b)).collect(), &c.action));
        let raw = self.keyboard.combos.iter().filter(|c| c.inputs.len() >= 2).filter_map(|c| {
            let ids: Option<Vec<Id>> = c.inputs.iter().map(|n| parse_input(n).map(Id::Raw)).collect();
            Some((ids?, &c.action))
        });
        pad.chain(raw).collect()
    }
}

impl Id {
    /// The gamepad button, if this is one.
    pub(super) fn pad(self) -> Option<Button> {
        match self {
            Id::Pad(b) => Some(b),
            Id::Raw(_) => None,
        }
    }
}

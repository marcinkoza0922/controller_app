//! What a keyboard profile remaps: keys and mouse buttons to other keys and mouse buttons.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Button, ButtonAction, Gestures, MouseButton, Profile, ProfileKind, Stick, Trigger, WheelDirection};

/// What a keyboard profile does with a key or mouse button it has no mapping for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OtherKeys {
    /// Forwarded unchanged, so a profile only lists the keys it changes.
    #[default]
    Pass,
    /// Swallowed, for games where stray keys are unwanted. Super and the Ctrl+Alt+Fn console
    /// switches still pass, and so do mouse movement and the wheel.
    Block,
}

impl OtherKeys {
    pub const ALL: [OtherKeys; 2] = [OtherKeys::Pass, OtherKeys::Block];
}

impl std::fmt::Display for OtherKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OtherKeys::Pass => "Pass through",
            OtherKeys::Block => "Block",
        })
    }
}

/// A way the mouse can move, as an input that presses while the pointer goes that way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MotionDirection {
    Up,
    Down,
    Left,
    Right,
}

impl MotionDirection {
    pub const ALL: [MotionDirection; 4] = [MotionDirection::Up, MotionDirection::Down, MotionDirection::Left, MotionDirection::Right];
}

impl std::fmt::Display for MotionDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mouse {}", format!("{self:?}").to_lowercase())
    }
}

/// What the mouse's movement becomes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionTarget {
    /// The pointer keeps moving, scaled.
    #[default]
    Pointer,
    /// A virtual-pad stick: movement pushes it, and it drifts back to the middle.
    Stick(Stick),
}

/// How mouse movement is handled.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MouseMotion {
    #[serde(default)]
    pub target: MotionTarget,
    /// Pointer: how much faster (or slower) the pointer moves.
    #[serde(default = "default_pointer_scale")]
    pub pointer_scale: f32,
    /// Stick: how far the mouse has to move, in counts, to push the stick all the way.
    #[serde(default = "default_counts")]
    pub counts: f32,
    /// How long a push takes to fade back to the middle, in milliseconds.
    #[serde(default = "default_decay_ms")]
    pub decay_ms: u32,
    #[serde(default)]
    pub invert_x: bool,
    #[serde(default)]
    pub invert_y: bool,
}

fn default_pointer_scale() -> f32 {
    1.0
}

fn default_counts() -> f32 {
    100.0
}

fn default_decay_ms() -> u32 {
    100
}

impl Default for MouseMotion {
    fn default() -> Self {
        MouseMotion {
            target: MotionTarget::Pointer,
            pointer_scale: default_pointer_scale(),
            counts: default_counts(),
            decay_ms: default_decay_ms(),
            invert_x: false,
            invert_y: false,
        }
    }
}

/// Keys and mouse buttons pressed together, acting as an input of their own. Members are named
/// like the keys of [`KeyboardMap::keys`], and mouse buttons `BTN_LEFT`, `BTN_RIGHT`,
/// `BTN_MIDDLE`, `BTN_SIDE` and `BTN_EXTRA`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyboardCombo {
    pub inputs: Vec<String>,
    pub action: ButtonAction,
}

/// A keyboard profile's mappings. Keys are evdev names such as `KEY_W`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyboardMap {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, ButtonAction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mouse: BTreeMap<MouseButton, ButtonAction>,
    /// Wheel notches, each a press.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub wheel: BTreeMap<WheelDirection, ButtonAction>,
    #[serde(default, skip_serializing_if = "motion_is_default")]
    pub motion: MouseMotion,
    /// Double tap, triple tap and long press of keys and mouse buttons, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gestures: BTreeMap<String, Gestures>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub combos: Vec<KeyboardCombo>,
    /// Held while the mouse moves that way.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub motion_buttons: BTreeMap<MotionDirection, ButtonAction>,
    #[serde(default)]
    pub other_keys: OtherKeys,
}

fn motion_is_default(m: &MouseMotion) -> bool {
    *m == MouseMotion::default()
}

impl KeyboardMap {
    pub fn is_default(&self) -> bool {
        *self == KeyboardMap::default()
    }

    /// Every action the profile maps something to.
    pub fn actions(&self) -> impl Iterator<Item = &ButtonAction> {
        let gestures = self.gestures.values().flat_map(|g| super::GestureKind::ALL.into_iter().filter_map(|k| g.get(k)));
        self.keys
            .values()
            .chain(self.mouse.values())
            .chain(self.wheel.values())
            .chain(self.motion_buttons.values())
            .chain(gestures)
            .chain(self.combos.iter().map(|c| &c.action))
    }

    pub fn actions_mut(&mut self) -> Vec<&mut ButtonAction> {
        let mut all: Vec<&mut ButtonAction> = self.keys.values_mut().collect();
        all.extend(self.mouse.values_mut());
        all.extend(self.wheel.values_mut());
        all.extend(self.motion_buttons.values_mut());
        for g in self.gestures.values_mut() {
            all.extend([&mut g.double_tap, &mut g.triple_tap, &mut g.long_press].into_iter().filter_map(|s| s.as_mut()));
        }
        all.extend(self.combos.iter_mut().map(|c| &mut c.action));
        all
    }

    /// Puts `layer`'s keys, buttons, gestures and combos on top of these. A combo of the same
    /// inputs replaces this one's; a gestures entry with none set turns the input's off.
    pub fn apply_layer(&mut self, layer: &KeyboardMap) {
        self.keys.extend(layer.keys.iter().map(|(k, a)| (k.clone(), a.clone())));
        self.mouse.extend(layer.mouse.iter().map(|(b, a)| (*b, a.clone())));
        self.wheel.extend(layer.wheel.iter().map(|(d, a)| (*d, a.clone())));
        self.motion_buttons.extend(layer.motion_buttons.iter().map(|(d, a)| (*d, a.clone())));
        for (name, g) in &layer.gestures {
            if g.is_empty() {
                self.gestures.remove(name);
            } else {
                self.gestures.insert(name.clone(), g.clone());
            }
        }
        let key = |inputs: &[String]| {
            let mut key = inputs.to_vec();
            key.sort();
            key
        };
        self.combos.retain(|c| !layer.combos.iter().any(|l| key(&l.inputs) == key(&c.inputs)));
        self.combos.extend(layer.combos.iter().cloned());
    }

    /// How many things a layer's map overrides, for summaries.
    pub fn overrides(&self) -> usize {
        self.keys.len() + self.mouse.len() + self.wheel.len() + self.motion_buttons.len() + self.gestures.len() + self.combos.len()
    }

    /// Whether the profile outputs to a gamepad, which needs a virtual pad while it is active.
    pub fn uses_pad(&self) -> bool {
        let mut pad = matches!(self.motion.target, MotionTarget::Stick(_));
        for action in self.actions() {
            action.walk(&mut |a| pad |= matches!(a, ButtonAction::Gamepad(_) | ButtonAction::PadTrigger(_)));
        }
        pad
    }
}

/// Keys that always pass through untouched: they can't be remapped or used in combos.
pub fn is_reserved_key(name: &str) -> bool {
    matches!(name, "KEY_LEFTMETA" | "KEY_RIGHTMETA")
}

/// The chord that turns remapping off, if the user hasn't chosen another.
pub fn default_panic_chord() -> Vec<String> {
    ["KEY_LEFTCTRL", "KEY_LEFTALT", "KEY_LEFTSHIFT", "KEY_ESC"].map(String::from).into()
}

impl Profile {
    /// An empty keyboard profile: every key and mouse button passes through.
    pub fn keyboard(name: &str) -> Self {
        Profile { kind: ProfileKind::Keyboard, buttons: BTreeMap::new(), ..Profile::passthrough(name) }
    }

    /// For games that only support a controller: WASD is the left stick, the mouse the right
    /// one, and the usual keys press the face buttons. Everything else is blocked.
    pub fn keyboard_to_pad(name: &str) -> Self {
        let pad = |b: Button| ButtonAction::Gamepad(b);
        let keys = [
            ("KEY_W", pad(Button::LeftStickUp)),
            ("KEY_A", pad(Button::LeftStickLeft)),
            ("KEY_S", pad(Button::LeftStickDown)),
            ("KEY_D", pad(Button::LeftStickRight)),
            ("KEY_SPACE", pad(Button::South)),
            ("KEY_LEFTCTRL", pad(Button::East)),
            ("KEY_E", pad(Button::West)),
            ("KEY_R", pad(Button::North)),
            ("KEY_Q", pad(Button::LeftBumper)),
            ("KEY_F", pad(Button::RightBumper)),
            ("KEY_LEFTSHIFT", pad(Button::LeftStick)),
            ("KEY_TAB", pad(Button::Select)),
            ("KEY_ESC", pad(Button::Start)),
            ("KEY_1", pad(Button::DpadUp)),
            ("KEY_2", pad(Button::DpadRight)),
            ("KEY_3", pad(Button::DpadDown)),
            ("KEY_4", pad(Button::DpadLeft)),
        ];
        let mouse = [
            (MouseButton::Left, ButtonAction::PadTrigger(Trigger::Right)),
            (MouseButton::Right, ButtonAction::PadTrigger(Trigger::Left)),
            (MouseButton::Middle, pad(Button::RightStick)),
        ];
        let keyboard = KeyboardMap {
            keys: keys.into_iter().map(|(k, a)| (k.to_string(), a)).collect(),
            mouse: mouse.into_iter().collect(),
            motion: MouseMotion { target: MotionTarget::Stick(Stick::Right), ..MouseMotion::default() },
            other_keys: OtherKeys::Block,
            ..KeyboardMap::default()
        };
        Profile { keyboard, ..Profile::keyboard(name) }
    }

    /// Shifts the three letter rows one key to the right, for playing a WASD game with the
    /// hand on ESDF: pressing E sends W, S sends A, D sends S, F sends D, G sends F, and so
    /// on. The key at the left end of each row (Q, A, Z) sends what the row's last key does,
    /// so nothing is lost.
    pub fn esdf_layout(name: &str) -> Self {
        const ROWS: [&[&str]; 3] = [
            &["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "LEFTBRACE", "RIGHTBRACE", "BACKSLASH"],
            &["A", "S", "D", "F", "G", "H", "J", "K", "L", "SEMICOLON", "APOSTROPHE"],
            &["Z", "X", "C", "V", "B", "N", "M", "COMMA", "DOT", "SLASH"],
        ];
        let mut keys = BTreeMap::new();
        for row in ROWS {
            for (i, key) in row.iter().enumerate() {
                let sends = row[(i + row.len() - 1) % row.len()];
                keys.insert(format!("KEY_{key}"), ButtonAction::Keys(vec![format!("KEY_{sends}")]));
            }
        }
        Profile { keyboard: KeyboardMap { keys, ..KeyboardMap::default() }, ..Profile::keyboard(name) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Layer};

    #[test]
    fn a_layer_overrides_keys_gestures_and_combos_of_a_keyboard_profile() {
        let base = Profile::esdf_layout("p");
        let mut layer = Layer::new("Nav");
        layer.keyboard.keys.insert("KEY_W".into(), ButtonAction::Keys(vec!["KEY_UP".into()]));
        layer.keyboard.keys.insert("KEY_Q".into(), ButtonAction::Disabled);
        layer.keyboard.combos.push(KeyboardCombo { inputs: vec!["KEY_J".into(), "KEY_K".into()], action: ButtonAction::Disabled });
        let on = base.with_layers([&layer]);
        assert_eq!(on.keyboard.keys["KEY_W"], ButtonAction::Keys(vec!["KEY_UP".into()]));
        assert_eq!(on.keyboard.keys["KEY_A"], base.keyboard.keys["KEY_A"], "the rest stays");
        assert_eq!(on.keyboard.combos.len(), 1);

        // A layer combo of the same inputs replaces the profile's; gestures with none turn off.
        let mut second = Layer::new("Other");
        second.keyboard.combos.push(KeyboardCombo { inputs: vec!["KEY_K".into(), "KEY_J".into()], action: ButtonAction::NextProfile });
        second.keyboard.gestures.insert("KEY_W".into(), Gestures::default());
        let mut with_gesture = on.clone();
        with_gesture.keyboard.gestures.insert("KEY_W".into(), Gestures { double_tap: Some(ButtonAction::Disabled), ..Gestures::default() });
        let both = with_gesture.with_layers([&second]);
        assert_eq!(both.keyboard.combos.len(), 1);
        assert_eq!(both.keyboard.combos[0].action, ButtonAction::NextProfile);
        assert!(both.keyboard.gestures.is_empty());
    }

    #[test]
    fn actions_of_a_keyboard_profile_and_its_layers_are_found_and_renamed_with_the_rest() {
        let mut p = Profile::keyboard("p");
        p.keyboard.keys.insert("KEY_G".into(), ButtonAction::Macro { name: "Heal".into(), repeat: false });
        p.keyboard.combos.push(KeyboardCombo { inputs: vec!["KEY_J".into(), "KEY_K".into()], action: ButtonAction::Macro { name: "Heal".into(), repeat: false } });
        p.keyboard.gestures.insert("KEY_G".into(), Gestures { long_press: Some(ButtonAction::Macro { name: "Heal".into(), repeat: false }), ..Gestures::default() });
        assert_eq!(p.actions().into_iter().filter(|a| matches!(a, ButtonAction::Macro { .. })).count(), 3);
        for a in p.actions_mut() {
            if let ButtonAction::Macro { name, .. } = a {
                *name = "Mend".into();
            }
        }
        assert!(p.actions().iter().all(|a| !matches!(a, ButtonAction::Macro { name, .. } if name == "Heal")));

        let mut layer = Layer::new("L");
        layer.keyboard.keys.insert("KEY_G".into(), ButtonAction::Macro { name: "Heal".into(), repeat: false });
        assert_eq!(layer.actions().len(), 1);
        assert_eq!(layer.keyboard.overrides(), 1);
    }

    #[test]
    fn keyboard_settings_round_trip_through_the_config_file() {
        let mut config = Config::default();
        let mut p = Profile::keyboard_to_pad("Pad");
        p.keyboard.wheel.insert(WheelDirection::Up, ButtonAction::Keys(vec!["KEY_PAGEUP".into()]));
        p.keyboard.motion_buttons.insert(MotionDirection::Left, ButtonAction::Keys(vec!["KEY_Q".into()]));
        p.keyboard.gestures.insert("BTN_LEFT".into(), Gestures { double_tap: Some(ButtonAction::Disabled), ..Gestures::default() });
        p.keyboard.combos.push(KeyboardCombo { inputs: vec!["KEY_J".into(), "BTN_LEFT".into()], action: ButtonAction::NextProfile });
        config.general.profiles.push(p.clone());
        let text = toml::to_string_pretty(&config).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.general.profiles.last().unwrap(), &p);
    }
}

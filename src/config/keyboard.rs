//! What a keyboard profile remaps: keys and mouse buttons to other keys and mouse buttons.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ButtonAction, MouseButton, Profile, ProfileKind};

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

/// A keyboard profile's mappings. Keys are evdev names such as `KEY_W`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyboardMap {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, ButtonAction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mouse: BTreeMap<MouseButton, ButtonAction>,
    #[serde(default)]
    pub other_keys: OtherKeys,
}

impl KeyboardMap {
    pub fn is_default(&self) -> bool {
        *self == KeyboardMap::default()
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
        Profile { kind: ProfileKind::Keyboard, ..Profile::passthrough(name) }
    }

    /// Moves the movement keys from WASD to ESDF. The old E, S, D and F are turned off so a game
    /// doesn't see both the old and the new key on them.
    pub fn wasd_to_esdf(name: &str) -> Self {
        let key = |k: &str| ButtonAction::Keys(vec![k.into()]);
        let shift = [("KEY_W", "KEY_E"), ("KEY_A", "KEY_S"), ("KEY_S", "KEY_D"), ("KEY_D", "KEY_F")];
        let mut keys: BTreeMap<String, ButtonAction> = shift.iter().map(|(from, to)| (from.to_string(), key(to))).collect();
        for old in ["KEY_E", "KEY_F"] {
            keys.insert(old.into(), ButtonAction::Disabled);
        }
        Profile { keyboard: KeyboardMap { keys, ..KeyboardMap::default() }, ..Profile::keyboard(name) }
    }
}

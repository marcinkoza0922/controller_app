//! A profile's touchpad settings: what finger movement does, and what the click does.

use serde::{Deserialize, Serialize};

use super::{ButtonAction, MouseButton};

/// Default pointer speed: pixels the pointer moves when a finger crosses the whole pad.
pub const DEFAULT_TOUCHPAD_SPEED: f32 = 1200.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchpadMotion {
    /// Finger movement does nothing. The pad is still taken from the desktop while the
    /// controller is managed, so the pointer doesn't move either.
    Off,
    /// Finger movement moves the mouse pointer. `speed` is the pixels across the whole pad.
    Mouse { speed: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TouchpadConfig {
    pub motion: TouchpadMotion,
    /// What pressing the pad does, like any button's action.
    pub click: ButtonAction,
}

impl Default for TouchpadConfig {
    /// Works as a laptop trackpad does, so the pad keeps serving the desktop pointer.
    fn default() -> Self {
        TouchpadConfig {
            motion: TouchpadMotion::Mouse { speed: DEFAULT_TOUCHPAD_SPEED },
            click: ButtonAction::Mouse(MouseButton::Left),
        }
    }
}

impl TouchpadConfig {
    /// Left out of saved configs and packs when it is the default, so they stay as they were.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_section_is_the_default_and_round_trips() {
        #[derive(Deserialize)]
        struct Wrap {
            #[serde(default)]
            touchpad: TouchpadConfig,
        }
        let w: Wrap = toml::from_str("").unwrap();
        assert!(w.touchpad.is_default());

        let custom = TouchpadConfig { motion: TouchpadMotion::Off, click: ButtonAction::Disabled };
        let text = toml::to_string(&custom).unwrap();
        let back: TouchpadConfig = toml::from_str(&text).unwrap();
        assert_eq!(back, custom);
        assert!(!custom.is_default());
    }

    #[test]
    fn motion_reads_off_or_mouse_with_speed() {
        let off: TouchpadConfig = toml::from_str("motion = \"off\"").unwrap();
        assert_eq!(off.motion, TouchpadMotion::Off);
        let mouse: TouchpadConfig = toml::from_str("[motion.mouse]\nspeed = 800.0").unwrap();
        assert_eq!(mouse.motion, TouchpadMotion::Mouse { speed: 800.0 });
        assert_eq!(mouse.click, TouchpadConfig::default().click, "a missing click keeps the default");
    }
}

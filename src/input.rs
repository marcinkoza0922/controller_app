//! Turns raw evdev events from a physical gamepad into normalized [`InputEvent`]s.

use std::{collections::HashMap, path::Path};

use evdev::{AbsoluteAxisCode as Abs, Device, EventSummary, KeyCode};

use crate::config::Button;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    LeftX,
    LeftY,
    RightX,
    RightY,
    LeftTrigger,
    RightTrigger,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    Button(Button, bool),
    /// Sticks are -1.0..1.0 (Y positive = down), triggers 0.0..1.0.
    Axis(Axis, f32),
}

#[derive(Clone, Copy)]
struct AxisRange {
    axis: Axis,
    min: f32,
    max: f32,
}

impl AxisRange {
    fn normalize(&self, raw: i32) -> f32 {
        let span = (self.max - self.min).max(1.0);
        let unit = (raw as f32 - self.min) / span;
        match self.axis {
            Axis::LeftTrigger | Axis::RightTrigger => unit.clamp(0.0, 1.0),
            _ => (unit * 2.0 - 1.0).clamp(-1.0, 1.0),
        }
    }
}

pub struct Normalizer {
    axes: HashMap<Abs, AxisRange>,
    analog_triggers: bool,
    hat: (i32, i32),
    /// See [`uses_xbox_labels`].
    xbox_labels: bool,
}

/// The kernel defines BTN_X == BTN_NORTH and BTN_Y == BTN_WEST, so drivers that emit
/// buttons by their Xbox *label* report X (physically west) as "north" and Y as "west".
/// Positional drivers (hid-playstation, hid-nintendo, ...) follow the gamepad spec instead.
pub fn uses_xbox_labels(dev_path: &Path) -> bool {
    let Some(node) = dev_path.file_name() else { return false };
    let driver = Path::new("/sys/class/input").join(node).join("device/device/driver");
    std::fs::canonicalize(driver)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .is_some_and(|name| name == "xpad")
}

/// Returns true if the device looks like a gamepad per the kernel gamepad spec
/// (BTN_SOUTH is the same code as BTN_GAMEPAD).
pub fn is_gamepad(dev: &Device) -> bool {
    let has_buttons = dev
        .supported_keys()
        .is_some_and(|k| k.contains(KeyCode::BTN_SOUTH));
    let has_stick = dev
        .supported_absolute_axes()
        .is_some_and(|a| a.contains(Abs::ABS_X) && a.contains(Abs::ABS_Y));
    has_buttons && has_stick
}

/// False for pads whose triggers are only on/off buttons (Switch Pro Controller, Joy-Cons,
/// many retro-style pads): they report BTN_TL2/BTN_TR2 but no trigger axis.
pub fn has_analog_triggers(dev: &Device) -> bool {
    Normalizer::new(dev, false).analog_triggers
}

impl Normalizer {
    pub fn new(dev: &Device, xbox_labels: bool) -> Self {
        let info: HashMap<Abs, (i32, i32)> = dev
            .get_absinfo()
            .map(|it| it.map(|(code, i)| (code, (i.minimum(), i.maximum()))).collect())
            .unwrap_or_default();

        // Most pads report the right stick on RX/RY and triggers on Z/RZ. Some generic HID
        // pads have no RX/RY and use Z/RZ for the right stick, with triggers on BRAKE/GAS.
        let right_on_z = !info.contains_key(&Abs::ABS_RX) && info.contains_key(&Abs::ABS_RZ);
        let mut layout = vec![
            (Abs::ABS_X, Axis::LeftX),
            (Abs::ABS_Y, Axis::LeftY),
            (Abs::ABS_BRAKE, Axis::LeftTrigger),
            (Abs::ABS_GAS, Axis::RightTrigger),
        ];
        if right_on_z {
            layout.extend([(Abs::ABS_Z, Axis::RightX), (Abs::ABS_RZ, Axis::RightY)]);
        } else {
            layout.extend([
                (Abs::ABS_RX, Axis::RightX),
                (Abs::ABS_RY, Axis::RightY),
                (Abs::ABS_Z, Axis::LeftTrigger),
                (Abs::ABS_RZ, Axis::RightTrigger),
            ]);
        }

        let mut axes = HashMap::new();
        for (code, axis) in layout {
            if let Some(&(min, max)) = info.get(&code) {
                axes.insert(code, AxisRange { axis, min: min as f32, max: max as f32 });
            }
        }
        let analog_triggers = axes
            .values()
            .any(|r| matches!(r.axis, Axis::LeftTrigger | Axis::RightTrigger));

        Normalizer { axes, analog_triggers, hat: (0, 0), xbox_labels }
    }

    pub fn translate(&mut self, ev: evdev::InputEvent, out: &mut Vec<InputEvent>) {
        match ev.destructure() {
            EventSummary::Key(_, code, value) => {
                // value 2 is autorepeat; ignore it.
                if value > 1 {
                    return;
                }
                let pressed = value == 1;
                if let Some(b) = key_to_button(code, self.xbox_labels) {
                    out.push(InputEvent::Button(b, pressed));
                } else if !self.analog_triggers {
                    let level = if pressed { 1.0 } else { 0.0 };
                    match code {
                        KeyCode::BTN_TL2 => out.push(InputEvent::Axis(Axis::LeftTrigger, level)),
                        KeyCode::BTN_TR2 => out.push(InputEvent::Axis(Axis::RightTrigger, level)),
                        _ => {}
                    }
                }
            }
            EventSummary::AbsoluteAxis(_, Abs::ABS_HAT0X, v) => {
                let old = self.hat.0;
                self.hat.0 = v.signum();
                hat_change(old, self.hat.0, Button::DpadLeft, Button::DpadRight, out);
            }
            EventSummary::AbsoluteAxis(_, Abs::ABS_HAT0Y, v) => {
                let old = self.hat.1;
                self.hat.1 = v.signum();
                hat_change(old, self.hat.1, Button::DpadUp, Button::DpadDown, out);
            }
            EventSummary::AbsoluteAxis(_, code, v) => {
                if let Some(range) = self.axes.get(&code) {
                    out.push(InputEvent::Axis(range.axis, range.normalize(v)));
                }
            }
            _ => {}
        }
    }
}

fn hat_change(old: i32, new: i32, neg: Button, pos: Button, out: &mut Vec<InputEvent>) {
    if old == new {
        return;
    }
    match old {
        -1 => out.push(InputEvent::Button(neg, false)),
        1 => out.push(InputEvent::Button(pos, false)),
        _ => {}
    }
    match new {
        -1 => out.push(InputEvent::Button(neg, true)),
        1 => out.push(InputEvent::Button(pos, true)),
        _ => {}
    }
}

fn key_to_button(code: KeyCode, xbox_labels: bool) -> Option<Button> {
    Some(match code {
        KeyCode::BTN_SOUTH => Button::South,
        KeyCode::BTN_EAST => Button::East,
        KeyCode::BTN_NORTH if xbox_labels => Button::West,
        KeyCode::BTN_WEST if xbox_labels => Button::North,
        KeyCode::BTN_NORTH => Button::North,
        KeyCode::BTN_WEST => Button::West,
        KeyCode::BTN_TL => Button::LeftBumper,
        KeyCode::BTN_TR => Button::RightBumper,
        KeyCode::BTN_SELECT => Button::Select,
        KeyCode::BTN_START => Button::Start,
        KeyCode::BTN_MODE => Button::Guide,
        KeyCode::BTN_THUMBL => Button::LeftStick,
        KeyCode::BTN_THUMBR => Button::RightStick,
        KeyCode::BTN_DPAD_UP => Button::DpadUp,
        KeyCode::BTN_DPAD_DOWN => Button::DpadDown,
        KeyCode::BTN_DPAD_LEFT => Button::DpadLeft,
        KeyCode::BTN_DPAD_RIGHT => Button::DpadRight,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_stick_and_trigger_ranges() {
        let stick = AxisRange { axis: Axis::LeftX, min: -32768.0, max: 32767.0 };
        assert!((stick.normalize(-32768) + 1.0).abs() < 1e-4);
        assert!(stick.normalize(0).abs() < 1e-3);
        assert!((stick.normalize(32767) - 1.0).abs() < 1e-4);

        let trig = AxisRange { axis: Axis::RightTrigger, min: 0.0, max: 1023.0 };
        assert_eq!(trig.normalize(0), 0.0);
        assert_eq!(trig.normalize(1023), 1.0);
    }

    #[test]
    fn xbox_label_drivers_swap_north_and_west() {
        // xpad sends BTN_X (== BTN_NORTH) for the X button, which sits on the west.
        assert_eq!(key_to_button(KeyCode::BTN_NORTH, true), Some(Button::West));
        assert_eq!(key_to_button(KeyCode::BTN_WEST, true), Some(Button::North));
        assert_eq!(key_to_button(KeyCode::BTN_NORTH, false), Some(Button::North));
        assert_eq!(key_to_button(KeyCode::BTN_SOUTH, true), Some(Button::South));
    }

    #[test]
    fn hat_emits_release_then_press() {
        let mut out = Vec::new();
        hat_change(-1, 1, Button::DpadLeft, Button::DpadRight, &mut out);
        assert_eq!(
            out,
            vec![
                InputEvent::Button(Button::DpadLeft, false),
                InputEvent::Button(Button::DpadRight, true)
            ]
        );
    }
}

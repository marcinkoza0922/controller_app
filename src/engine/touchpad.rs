//! Touchpad input: the click runs the profile's touchpad action like a button press, and finger
//! movement drives the mouse pointer.

use crate::{
    config::{Profile, TouchpadMotion},
    input::TouchpadEvent,
    output::OutEvent,
};

use super::{Engine, Source, take_whole};

impl Engine {
    /// Handles one touchpad event.
    pub(super) fn touchpad(&mut self, profile: &Profile, ev: TouchpadEvent, out: &mut Vec<OutEvent>) {
        match ev {
            TouchpadEvent::Click(pressed) => self.digital(&Source::Touchpad, &profile.touchpad.click, pressed, out),
            TouchpadEvent::Move(dx, dy) => {
                if let TouchpadMotion::Mouse { speed } = profile.touchpad.motion {
                    let (x, y) = take_whole(&mut self.touchpad_acc, dx * speed, dy * speed);
                    if (x, y) != (0, 0) {
                        out.push(OutEvent::MouseMove(x, y));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{ButtonAction, MouseButton, TouchpadConfig};

    use super::*;

    fn profile(touchpad: TouchpadConfig) -> Profile {
        Profile { touchpad, ..Profile::passthrough("test") }
    }

    fn run(e: &mut Engine, p: &Profile, ev: TouchpadEvent) -> Vec<OutEvent> {
        let mut out = Vec::new();
        e.touchpad(p, ev, &mut out);
        out
    }

    #[test]
    fn movement_becomes_pointer_motion_scaled_by_speed() {
        let p = profile(TouchpadConfig { motion: TouchpadMotion::Mouse { speed: 1000.0 }, ..TouchpadConfig::default() });
        let mut e = Engine::default();
        assert_eq!(run(&mut e, &p, TouchpadEvent::Move(0.1, -0.05)), vec![OutEvent::MouseMove(100, -50)]);
        // Fractions carry over until they make a whole pixel.
        assert!(run(&mut e, &p, TouchpadEvent::Move(0.0004, 0.0)).is_empty());
        assert_eq!(run(&mut e, &p, TouchpadEvent::Move(0.0006, 0.0)), vec![OutEvent::MouseMove(1, 0)]);
    }

    #[test]
    fn motion_off_moves_nothing() {
        let p = profile(TouchpadConfig { motion: TouchpadMotion::Off, ..TouchpadConfig::default() });
        assert!(run(&mut Engine::default(), &p, TouchpadEvent::Move(0.5, 0.5)).is_empty());
    }

    #[test]
    fn click_runs_its_action_on_press_and_releases_it() {
        let p = profile(TouchpadConfig { click: ButtonAction::Mouse(MouseButton::Right), ..TouchpadConfig::default() });
        let mut e = Engine::default();
        assert_eq!(run(&mut e, &p, TouchpadEvent::Click(true)), vec![OutEvent::MouseButton(MouseButton::Right, true)]);
        assert_eq!(run(&mut e, &p, TouchpadEvent::Click(false)), vec![OutEvent::MouseButton(MouseButton::Right, false)]);
    }

    #[test]
    fn a_disabled_click_does_nothing() {
        let p = profile(TouchpadConfig { click: ButtonAction::Disabled, ..TouchpadConfig::default() });
        let mut e = Engine::default();
        assert!(run(&mut e, &p, TouchpadEvent::Click(true)).is_empty());
    }
}

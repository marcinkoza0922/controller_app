//! Touchpads on controllers (DualShock 4, DualSense): their own evdev node, with a click button
//! and multitouch positions. Only the primary finger (the lowest slot still touching) counts.

use evdev::{AbsoluteAxisCode as Abs, Device, EventSummary, KeyCode};

use super::InputEvent;

/// What a touchpad did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TouchpadEvent {
    /// The click, pressed or released.
    Click(bool),
    /// The primary finger moved this much since the last report, in pad widths. X and Y are
    /// both divided by the width, so a diagonal stroke keeps its angle.
    Move(f32, f32),
}

/// Pad width assumed when the device doesn't report its range (DualSense: 1920 units).
const FALLBACK_WIDTH: f32 = 1920.0;

/// True for a controller's touchpad node: a click button and multitouch positions, and no face
/// buttons (which would make it a gamepad). Only used to pair it with a managed controller.
pub fn is_touchpad(dev: &Device) -> bool {
    let click = dev.supported_keys().is_some_and(|k| k.contains(KeyCode::BTN_LEFT));
    let multitouch = dev
        .supported_absolute_axes()
        .is_some_and(|a| a.contains(Abs::ABS_MT_POSITION_X) && a.contains(Abs::ABS_MT_POSITION_Y));
    click && multitouch && !super::is_gamepad(dev)
}

/// The latest state of one contact slot. `tracking` is -1 when the slot has no finger.
#[derive(Clone, Copy)]
struct Contact {
    tracking: i32,
    x: i32,
    y: i32,
}

/// The finger being followed, and where it was at the last report.
#[derive(Clone, Copy)]
struct Primary {
    slot: usize,
    tracking: i32,
    x: i32,
    y: i32,
}

/// Turns a touchpad's raw events into [`InputEvent::Touchpad`]s, one movement per report.
pub struct TouchNormalizer {
    /// Raw units across the pad; movement is divided by this.
    width: f32,
    contacts: Vec<Contact>,
    /// Slot that the next position events apply to (ABS_MT_SLOT).
    slot: usize,
    primary: Option<Primary>,
}

impl TouchNormalizer {
    pub fn new(dev: &Device) -> Self {
        let width = dev
            .get_absinfo()
            .ok()
            .and_then(|mut it| it.find(|(code, _)| *code == Abs::ABS_MT_POSITION_X))
            .map(|(_, info)| (info.maximum() - info.minimum()) as f32)
            .filter(|w| *w > 0.0)
            .unwrap_or(FALLBACK_WIDTH);
        TouchNormalizer { width, contacts: Vec::new(), slot: 0, primary: None }
    }

    pub fn translate(&mut self, ev: evdev::InputEvent, out: &mut Vec<InputEvent>) {
        match ev.destructure() {
            EventSummary::Key(_, KeyCode::BTN_LEFT, value) if value <= 1 => {
                out.push(InputEvent::Touchpad(TouchpadEvent::Click(value == 1)));
            }
            EventSummary::AbsoluteAxis(_, Abs::ABS_MT_SLOT, value) => self.slot = value.max(0) as usize,
            EventSummary::AbsoluteAxis(_, Abs::ABS_MT_TRACKING_ID, value) => self.contact().tracking = value,
            EventSummary::AbsoluteAxis(_, Abs::ABS_MT_POSITION_X, value) => self.contact().x = value,
            EventSummary::AbsoluteAxis(_, Abs::ABS_MT_POSITION_Y, value) => self.contact().y = value,
            EventSummary::Synchronization(..) => self.finish_report(out),
            _ => {}
        }
    }

    /// The contact slot the next position or tracking event applies to.
    fn contact(&mut self) -> &mut Contact {
        if self.contacts.len() <= self.slot {
            self.contacts.resize(self.slot + 1, Contact { tracking: -1, x: 0, y: 0 });
        }
        &mut self.contacts[self.slot]
    }

    /// Reports the primary finger's movement since the last report, if it is the same finger.
    fn finish_report(&mut self, out: &mut Vec<InputEvent>) {
        let now = self.contacts.iter().enumerate().find(|(_, c)| c.tracking >= 0).map(|(slot, c)| Primary {
            slot,
            tracking: c.tracking,
            x: c.x,
            y: c.y,
        });
        if let (Some(prev), Some(cur)) = (self.primary, now)
            && prev.slot == cur.slot
            && prev.tracking == cur.tracking
            && (prev.x, prev.y) != (cur.x, cur.y)
        {
            let dx = (cur.x - prev.x) as f32 / self.width;
            let dy = (cur.y - prev.y) as f32 / self.width;
            out.push(InputEvent::Touchpad(TouchpadEvent::Move(dx, dy)));
        }
        self.primary = now;
    }
}

#[cfg(test)]
mod tests {
    use evdev::{EventType, InputEvent as Raw};

    use super::*;

    fn abs(code: Abs, value: i32) -> Raw {
        Raw::new(EventType::ABSOLUTE.0, code.0, value)
    }

    fn sync() -> Raw {
        Raw::new(EventType::SYNCHRONIZATION.0, 0, 0)
    }

    /// Feeds one report and returns what it produced.
    fn report(norm: &mut TouchNormalizer, events: &[Raw]) -> Vec<InputEvent> {
        let mut out = Vec::new();
        for ev in events.iter().copied().chain([sync()]) {
            norm.translate(ev, &mut out);
        }
        out
    }

    fn touch(norm: &mut TouchNormalizer, slot: i32, id: i32, x: i32, y: i32) -> Vec<InputEvent> {
        report(norm, &[abs(Abs::ABS_MT_SLOT, slot), abs(Abs::ABS_MT_TRACKING_ID, id), abs(Abs::ABS_MT_POSITION_X, x), abs(Abs::ABS_MT_POSITION_Y, y)])
    }

    fn norm() -> TouchNormalizer {
        TouchNormalizer { width: 1000.0, contacts: Vec::new(), slot: 0, primary: None }
    }

    #[test]
    fn a_finger_moves_in_pad_widths_and_its_first_report_is_silent() {
        let mut n = norm();
        assert!(touch(&mut n, 0, 7, 100, 200).is_empty(), "landing is not movement");
        assert_eq!(touch(&mut n, 0, 7, 300, 150), vec![InputEvent::Touchpad(TouchpadEvent::Move(0.2, -0.05))]);
        // A report with no change (or only other slots changing) moves nothing.
        assert!(report(&mut n, &[]).is_empty());
    }

    #[test]
    fn lifting_and_landing_again_does_not_jump() {
        let mut n = norm();
        touch(&mut n, 0, 7, 100, 200);
        assert!(report(&mut n, &[abs(Abs::ABS_MT_TRACKING_ID, -1)]).is_empty());
        assert!(touch(&mut n, 0, 8, 900, 900).is_empty(), "a new contact starts from rest");
    }

    #[test]
    fn only_the_lowest_touching_slot_drives_movement() {
        let mut n = norm();
        touch(&mut n, 0, 1, 100, 100);
        // A second finger lands in slot 1 and moves: no movement from it.
        assert!(touch(&mut n, 1, 2, 500, 500).is_empty());
        assert!(touch(&mut n, 1, 2, 600, 500).is_empty());
        // The first finger still counts.
        assert_eq!(touch(&mut n, 0, 1, 150, 100), vec![InputEvent::Touchpad(TouchpadEvent::Move(0.05, 0.0))]);
        // When it lifts, the second finger becomes primary without a jump.
        assert!(report(&mut n, &[abs(Abs::ABS_MT_SLOT, 0), abs(Abs::ABS_MT_TRACKING_ID, -1)]).is_empty());
        assert_eq!(touch(&mut n, 1, 2, 610, 500), vec![InputEvent::Touchpad(TouchpadEvent::Move(0.01, 0.0))]);
    }

    #[test]
    fn click_reports_press_and_release_and_ignores_repeats() {
        let mut n = norm();
        let key = |value| Raw::new(EventType::KEY.0, KeyCode::BTN_LEFT.0, value);
        assert_eq!(report(&mut n, &[key(1)]), vec![InputEvent::Touchpad(TouchpadEvent::Click(true))]);
        assert!(report(&mut n, &[key(2)]).is_empty());
        assert_eq!(report(&mut n, &[key(0)]), vec![InputEvent::Touchpad(TouchpadEvent::Click(false))]);
    }
}

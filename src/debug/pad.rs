//! The virtual controller: the input state of whichever model the debug session pretends to be.

use crate::{
    config::{Button, Stick, Trigger},
    info::{PadFamily, PadModel},
    input::Axis,
    ipc::{DebugEvent, InputSnapshot},
};

#[derive(Debug, Clone, PartialEq)]
pub struct VirtualPad {
    /// `None` is the generic controller.
    pub model: Option<PadModel>,
    /// Whose button glyphs are drawn; follows the model unless set by hand.
    pub family: PadFamily,
    buttons: Vec<Button>,
    left_stick: (f32, f32),
    right_stick: (f32, f32),
    left_trigger: f32,
    right_trigger: f32,
    gyro: Option<[f32; 3]>,
    /// Changes not yet sent to the daemon, while a controller is injected there.
    pending: Option<Vec<DebugEvent>>,
}

impl Default for VirtualPad {
    fn default() -> Self {
        Self::of(None)
    }
}

impl VirtualPad {
    /// A pad at rest, of `model`.
    pub fn of(model: Option<PadModel>) -> Self {
        Self {
            model,
            family: model.map_or(PadFamily::Xbox, PadModel::family),
            buttons: Vec::new(),
            left_stick: (0.0, 0.0),
            right_stick: (0.0, 0.0),
            left_trigger: 0.0,
            right_trigger: 0.0,
            gyro: None,
            pending: None,
        }
    }

    /// Becomes another model, keeping the input held so far.
    pub fn set_model(&mut self, model: Option<PadModel>) {
        self.model = model;
        self.family = model.map_or(PadFamily::Xbox, PadModel::family);
    }

    /// Lets go of everything and centers the sticks, keeping the model.
    pub fn reset(&mut self) {
        let held = self.buttons.clone();
        held.into_iter().for_each(|b| self.release(b));
        for stick in [Stick::Left, Stick::Right] {
            self.set_stick(stick, (0.0, 0.0));
        }
        for trigger in [Trigger::Left, Trigger::Right] {
            self.set_trigger(trigger, 0.0);
        }
        self.gyro = None;
    }

    /// Starts keeping the changes made from now on, for [`VirtualPad::take_events`].
    pub fn track_events(&mut self) {
        self.pending.get_or_insert_with(Vec::new);
    }

    /// The changes since the last call, in order.
    pub fn take_events(&mut self) -> Vec<DebugEvent> {
        self.pending.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// Events that bring a pad at rest to the current state.
    pub fn state_events(&self) -> Vec<DebugEvent> {
        let mut events: Vec<DebugEvent> = self.buttons.iter().map(|&b| DebugEvent::Button(b, true)).collect();
        for stick in [Stick::Left, Stick::Right] {
            let (x, y) = self.stick(stick);
            events.extend(stick_events(stick, (0.0, 0.0), (x, y)));
        }
        for trigger in [Trigger::Left, Trigger::Right] {
            if self.trigger(trigger) != 0.0 {
                events.push(DebugEvent::Axis(trigger_axis(trigger), self.trigger(trigger)));
            }
        }
        events
    }

    fn queue(&mut self, events: impl IntoIterator<Item = DebugEvent>) {
        if let Some(pending) = &mut self.pending {
            pending.extend(events);
        }
    }

    pub fn is_pressed(&self, b: Button) -> bool {
        self.buttons.contains(&b)
    }

    pub fn press(&mut self, b: Button) {
        if !self.is_pressed(b) {
            self.buttons.push(b);
            self.queue([DebugEvent::Button(b, true)]);
        }
    }

    pub fn release(&mut self, b: Button) {
        if self.is_pressed(b) {
            self.buttons.retain(|&held| held != b);
            self.queue([DebugEvent::Button(b, false)]);
        }
    }

    /// Presses `b` if it is up, releases it if it is down.
    pub fn toggle(&mut self, b: Button) {
        if self.is_pressed(b) {
            self.release(b);
        } else {
            self.press(b);
        }
    }

    /// Moves a stick; each axis is held to -1..1 (y positive is down).
    pub fn set_stick(&mut self, stick: Stick, (x, y): (f32, f32)) {
        let value = (x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0));
        self.queue(stick_events(stick, self.stick(stick), value));
        match stick {
            Stick::Left => self.left_stick = value,
            Stick::Right => self.right_stick = value,
        }
    }

    pub fn stick(&self, stick: Stick) -> (f32, f32) {
        match stick {
            Stick::Left => self.left_stick,
            Stick::Right => self.right_stick,
        }
    }

    /// Pulls a trigger by `value`, held to 0..1.
    pub fn set_trigger(&mut self, trigger: Trigger, value: f32) {
        let value = value.clamp(0.0, 1.0);
        if value != self.trigger(trigger) {
            self.queue([DebugEvent::Axis(trigger_axis(trigger), value)]);
        }
        match trigger {
            Trigger::Left => self.left_trigger = value,
            Trigger::Right => self.right_trigger = value,
        }
    }

    pub fn trigger(&self, trigger: Trigger) -> f32 {
        match trigger {
            Trigger::Left => self.left_trigger,
            Trigger::Right => self.right_trigger,
        }
    }

    /// Degrees per second of pitch, yaw and roll; `None` is a pad that isn't turning.
    pub fn set_gyro(&mut self, rates: Option<[f32; 3]>) {
        self.gyro = rates;
    }

    /// The input as the daemon reports a connected controller's.
    pub fn snapshot(&self) -> InputSnapshot {
        let name = self.model.map_or("generic", PadModel::slug);
        InputSnapshot {
            device: format!("Debug controller ({name})"),
            model: self.model,
            family: Some(self.family),
            buttons: self.buttons.clone(),
            left_stick: self.left_stick,
            right_stick: self.right_stick,
            left_trigger: self.left_trigger,
            right_trigger: self.right_trigger,
            gyro: self.gyro,
        }
    }
}

fn trigger_axis(trigger: Trigger) -> Axis {
    match trigger {
        Trigger::Left => Axis::LeftTrigger,
        Trigger::Right => Axis::RightTrigger,
    }
}

/// The axis events for a stick moving from `from` to `to`; an axis that stays put sends nothing.
fn stick_events(stick: Stick, from: (f32, f32), to: (f32, f32)) -> Vec<DebugEvent> {
    let (x, y) = match stick {
        Stick::Left => (Axis::LeftX, Axis::LeftY),
        Stick::Right => (Axis::RightX, Axis::RightY),
    };
    [(x, from.0, to.0), (y, from.1, to.1)]
        .into_iter()
        .filter(|(_, was, now)| was != now)
        .map(|(axis, _, now)| DebugEvent::Axis(axis, now))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_is_held_and_clamped() {
        let mut pad = VirtualPad::of(Some(PadModel::DualSense));
        assert_eq!(pad.family, PadFamily::PlayStation);
        pad.press(Button::South);
        pad.press(Button::South);
        pad.set_stick(Stick::Left, (3.0, -0.5));
        pad.set_trigger(Trigger::Right, 2.0);
        let s = pad.snapshot();
        assert_eq!(s.buttons, [Button::South]);
        assert_eq!(s.left_stick, (1.0, -0.5));
        assert_eq!(s.right_trigger, 1.0);
        pad.toggle(Button::South);
        assert!(!pad.is_pressed(Button::South));
    }

    #[test]
    fn reset_centers_everything_but_keeps_the_model() {
        let mut pad = VirtualPad::of(Some(PadModel::Xbox360));
        pad.press(Button::Start);
        pad.set_gyro(Some([1.0, 2.0, 3.0]));
        pad.set_model(Some(PadModel::JoyCons));
        pad.reset();
        assert_eq!(pad.snapshot().buttons, Vec::<Button>::new());
        assert_eq!(pad.snapshot().gyro, None);
        assert_eq!(pad.model, Some(PadModel::JoyCons));
        assert_eq!(pad.family, PadFamily::Nintendo);
    }

    #[test]
    fn changes_are_queued_in_order_once_tracked() {
        let mut pad = VirtualPad::default();
        pad.press(Button::South);
        pad.track_events();
        pad.press(Button::South);
        pad.press(Button::East);
        pad.set_stick(Stick::Left, (0.5, 0.0));
        pad.set_trigger(Trigger::Right, 1.0);
        pad.reset();
        assert_eq!(
            pad.take_events(),
            [
                DebugEvent::Button(Button::East, true),
                DebugEvent::Axis(Axis::LeftX, 0.5),
                DebugEvent::Axis(Axis::RightTrigger, 1.0),
                DebugEvent::Button(Button::South, false),
                DebugEvent::Button(Button::East, false),
                DebugEvent::Axis(Axis::LeftX, 0.0),
                DebugEvent::Axis(Axis::RightTrigger, 0.0),
            ]
        );
        assert!(pad.take_events().is_empty());
    }

    #[test]
    fn state_events_rebuild_the_current_state() {
        let mut pad = VirtualPad::default();
        pad.press(Button::Start);
        pad.set_stick(Stick::Right, (0.0, -1.0));
        assert_eq!(pad.state_events(), [DebugEvent::Button(Button::Start, true), DebugEvent::Axis(Axis::RightY, -1.0)]);
    }
}

//! Per-device mapping state: turns normalized input into output events for the active profile.

use std::{
    collections::HashMap,
    str::FromStr,
    time::{Duration, Instant},
};

use evdev::KeyCode;

use crate::{
    config::{Analog, Button, ButtonAction, Profile, Stick, StickAction, Trigger, TriggerAction},
    input::{Axis, InputEvent},
    output::OutEvent,
};

/// A held zone stays active this far past its edges, so it doesn't flicker on a boundary.
const ZONE_HYSTERESIS: f32 = 0.02;
/// A trigger mapped to a button releases this far below its press threshold.
const TRIGGER_HYSTERESIS: f32 = 0.05;
const WHEEL_UNITS_PER_NOTCH: f32 = 120.0;
/// A held wheel action scrolls one notch at once, then continuously after this delay...
const WHEEL_REPEAT_DELAY: f32 = 0.35;
/// ...at this many notches per second.
const WHEEL_REPEAT_RATE: f32 = 10.0;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Source {
    Button(Button),
    Trigger(Trigger),
    /// An active combo, identified by its sorted member buttons.
    Combo(Vec<Button>),
    /// A held double/triple-tap or long-press action.
    Gesture(Button),
    /// An analog zone (index into the stick's or trigger's zone list) that is active.
    Zone(Analog, usize),
}

/// Gesture detection for a button acting alone that has gestures configured.
#[derive(Debug, Clone, Copy, PartialEq)]
enum GestureState {
    /// Held, undecided. `taps` counts presses in this sequence including this one.
    Down { taps: u8, long_deadline: Option<Instant> },
    /// Released, waiting to see whether another tap follows.
    Up { taps: u8, deadline: Instant },
    /// A gesture action fired and is held until release.
    Holding,
}

/// State of a held button that belongs to at least one combo.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ComboMember {
    /// Waiting to see if the rest of a combo follows. `None` never times out: a member
    /// whose own action is Disabled works as a modifier.
    Pending { deadline: Option<Instant> },
    /// The window ran out; the button's own action is pressed.
    Fired,
    /// Used up by a combo; releasing it releases the combo, nothing else.
    Consumed,
}

#[derive(Default)]
pub struct Engine {
    axes: HashMap<Axis, f32>,
    /// Action each held input pressed, so release always matches press even across profile edits.
    held: HashMap<Source, ButtonAction>,
    members: HashMap<Button, ComboMember>,
    gestures: HashMap<Button, GestureState>,
    stick_keys: HashMap<Stick, Vec<KeyCode>>,
    mouse_acc: (f32, f32),
    scroll_acc: (f32, f32),
    /// Seconds each held action containing a wheel direction has been held.
    wheel_held: HashMap<Source, f32>,
    wheel_acc: (f32, f32),
}

impl Engine {
    /// Processes one input. Returns true if the input requested switching to the next profile.
    pub fn handle(
        &mut self,
        profile: &Profile,
        ev: InputEvent,
        now: Instant,
        out: &mut Vec<OutEvent>,
    ) -> bool {
        match ev {
            InputEvent::Button(b, true) if profile.in_combo(b) => {
                let deadline = match profile.button(b) {
                    ButtonAction::Disabled => None,
                    _ => Some(now + Duration::from_millis(profile.combo_window_ms)),
                };
                self.members.insert(b, ComboMember::Pending { deadline });
                self.try_combo(profile, out)
            }
            InputEvent::Button(b, false) if self.members.contains_key(&b) => {
                match self.members.remove(&b) {
                    // Released inside the window: a quick tap of the button on its own.
                    Some(ComboMember::Pending { .. }) => {
                        let switch = self.solo(profile, b, true, now, out);
                        self.solo(profile, b, false, now, out) | switch
                    }
                    Some(ComboMember::Fired) => self.solo(profile, b, false, now, out),
                    Some(ComboMember::Consumed) => {
                        let combos: Vec<Source> = self
                            .held
                            .keys()
                            .filter(|s| matches!(s, Source::Combo(m) if m.contains(&b)))
                            .cloned()
                            .collect();
                        for src in combos {
                            self.digital(src, &ButtonAction::Disabled, false, out);
                        }
                        false
                    }
                    None => false,
                }
            }
            InputEvent::Button(b, pressed) => self.solo(profile, b, pressed, now, out),
            InputEvent::Axis(axis, value) => {
                self.axes.insert(axis, value);
                match axis {
                    Axis::LeftTrigger => self.trigger(profile, Trigger::Left, value, out),
                    Axis::RightTrigger => self.trigger(profile, Trigger::Right, value, out),
                    Axis::LeftX | Axis::LeftY => self.stick(profile, Stick::Left, out),
                    Axis::RightX | Axis::RightY => self.stick(profile, Stick::Right, out),
                }
            }
        }
    }

    /// Fires the largest combo whose members are all pending.
    fn try_combo(&mut self, profile: &Profile, out: &mut Vec<OutEvent>) -> bool {
        let ready = profile
            .combos
            .iter()
            .filter(|c| c.buttons.len() >= 2)
            .filter(|c| {
                c.buttons
                    .iter()
                    .all(|b| matches!(self.members.get(b), Some(ComboMember::Pending { .. })))
            })
            .max_by_key(|c| c.buttons.len());
        let Some(combo) = ready else { return false };
        for b in &combo.buttons {
            self.members.insert(*b, ComboMember::Consumed);
        }
        let mut id = combo.buttons.clone();
        id.sort();
        id.dedup();
        self.digital(Source::Combo(id), &combo.action, true, out)
    }

    /// Fires combo members whose window has run out. Returns true on a profile switch request.
    pub fn timers(&mut self, profile: &Profile, now: Instant, out: &mut Vec<OutEvent>) -> bool {
        let due: Vec<Button> = self
            .members
            .iter()
            .filter(|(_, m)| matches!(m, ComboMember::Pending { deadline: Some(d) } if *d <= now))
            .map(|(b, _)| *b)
            .collect();
        let mut switch = false;
        for b in due {
            self.members.insert(b, ComboMember::Fired);
            switch |= self.solo(profile, b, true, now, out);
        }

        let due: Vec<(Button, GestureState)> = self
            .gestures
            .iter()
            .filter(|(_, g)| match g {
                GestureState::Down { long_deadline: Some(d), .. } => *d <= now,
                GestureState::Up { deadline, .. } => *deadline <= now,
                _ => false,
            })
            .map(|(b, g)| (*b, *g))
            .collect();
        for (b, state) in due {
            let Some(gestures) = profile.gestures(b) else {
                self.gestures.remove(&b);
                continue;
            };
            match state {
                // Held long enough: the long press fires and stays held until release.
                GestureState::Down { .. } => {
                    self.gestures.insert(b, GestureState::Holding);
                    if let Some(action) = &gestures.long_press {
                        switch |= self.digital(Source::Gesture(b), action, true, out);
                    }
                }
                // No further tap came: the sequence so far is final.
                GestureState::Up { taps, .. } => {
                    self.gestures.remove(&b);
                    switch |= self.tap_sequence(profile, b, taps, out);
                }
                GestureState::Holding => {}
            }
        }
        switch
    }

    /// Earliest moment `timers` has work to do.
    pub fn next_deadline(&self) -> Option<Instant> {
        let combos = self.members.values().filter_map(|m| match m {
            ComboMember::Pending { deadline } => *deadline,
            _ => None,
        });
        let gestures = self.gestures.values().filter_map(|g| match g {
            GestureState::Down { long_deadline, .. } => *long_deadline,
            GestureState::Up { deadline, .. } => Some(*deadline),
            GestureState::Holding => None,
        });
        combos.chain(gestures).min()
    }

    /// A button acting on its own (not as part of a combo). Runs gesture detection if the
    /// button has gestures, otherwise presses/releases its action directly.
    fn solo(&mut self, profile: &Profile, b: Button, pressed: bool, now: Instant, out: &mut Vec<OutEvent>) -> bool {
        let Some(gestures) = profile.gestures(b) else {
            // Gestures may have been removed mid-sequence; a release still has to land.
            if !pressed && self.gestures.remove(&b) == Some(GestureState::Holding) {
                return self.digital(Source::Gesture(b), &ButtonAction::Disabled, false, out);
            }
            return self.digital(Source::Button(b), profile.button(b), pressed, out);
        };
        let max_taps = gestures.max_taps();
        if pressed {
            let taps = match self.gestures.get(&b) {
                Some(GestureState::Up { taps, .. }) => taps + 1,
                _ => 1,
            };
            if taps > 1 && taps == max_taps {
                // Final tap of the longest sequence: fire now and hold until release.
                self.gestures.insert(b, GestureState::Holding);
                let action = gestures.for_taps(taps).unwrap_or(profile.button(b));
                return self.digital(Source::Gesture(b), action, true, out);
            }
            let long_deadline = (taps == 1 && gestures.long_press.is_some())
                .then(|| now + Duration::from_millis(profile.long_press_ms));
            self.gestures.insert(b, GestureState::Down { taps, long_deadline });
            false
        } else {
            match self.gestures.remove(&b) {
                Some(GestureState::Holding) => {
                    self.digital(Source::Gesture(b), &ButtonAction::Disabled, false, out)
                }
                Some(GestureState::Down { taps, .. }) if taps < max_taps => {
                    let deadline = now + Duration::from_millis(profile.tap_window_ms);
                    self.gestures.insert(b, GestureState::Up { taps, deadline });
                    false
                }
                // Nothing more can follow (e.g. only a long press is set): act as a tap now.
                Some(GestureState::Down { taps, .. }) => self.tap_sequence(profile, b, taps, out),
                _ => false,
            }
        }
    }

    /// Emits the result of a finished tap sequence as a quick press and release.
    fn tap_sequence(&mut self, profile: &Profile, b: Button, taps: u8, out: &mut Vec<OutEvent>) -> bool {
        let gestures = profile.gestures(b);
        let mut switch = false;
        match gestures.and_then(|g| g.for_taps(taps)) {
            Some(action) => switch |= self.tap(Source::Gesture(b), action, out),
            // e.g. a double tap when only a triple tap is set: that many normal taps.
            None => {
                for _ in 0..taps {
                    switch |= self.tap(Source::Button(b), profile.button(b), out);
                }
            }
        }
        switch
    }

    fn tap(&mut self, src: Source, action: &ButtonAction, out: &mut Vec<OutEvent>) -> bool {
        let switch = self.digital(src.clone(), action, true, out);
        self.digital(src, action, false, out);
        switch
    }

    fn digital(
        &mut self,
        src: Source,
        action: &ButtonAction,
        pressed: bool,
        out: &mut Vec<OutEvent>,
    ) -> bool {
        if pressed {
            if self.held.contains_key(&src) {
                return false;
            }
            self.held.insert(src, action.clone());
            emit_action(action, true, out)
        } else {
            if let Some(action) = self.held.remove(&src) {
                emit_action(&action, false, out);
            }
            false
        }
    }

    fn trigger(&mut self, profile: &Profile, t: Trigger, value: f32, out: &mut Vec<OutEvent>) -> bool {
        let switch = match profile.trigger(t) {
            TriggerAction::Disabled => false,
            TriggerAction::Gamepad(target) => {
                out.push(OutEvent::PadAxis(trigger_axis(*target), value));
                false
            }
            TriggerAction::Button { action, threshold } => {
                let src = Source::Trigger(t);
                let held = self.held.contains_key(&src);
                let threshold = *threshold;
                if !held && value >= threshold {
                    self.digital(src, action, true, out)
                } else if held && value < threshold - TRIGGER_HYSTERESIS {
                    self.digital(src, action, false, out)
                } else {
                    false
                }
            }
        };
        self.zones(profile, Analog::Trigger(t), value, out) | switch
    }

    /// Presses/releases zone actions for an analog input at `value` (0.0..1.0). Zones are
    /// inactive at rest (value 0), so a zone starting at 0 means "as soon as it moves".
    fn zones(&mut self, profile: &Profile, analog: Analog, value: f32, out: &mut Vec<OutEvent>) -> bool {
        let zones = profile.zones(analog);
        let mut changes = Vec::new();
        for (i, zone) in zones.iter().enumerate() {
            let held = self.held.contains_key(&Source::Zone(analog, i));
            let margin = if held { ZONE_HYSTERESIS } else { 0.0 };
            // The top zone includes full deflection itself.
            let below_max = zone.max >= 1.0 || value < zone.max + margin;
            let active = value > 0.0 && value >= zone.min - margin && below_max;
            if active != held {
                changes.push((i, active));
            }
        }
        // Release before press so moving between adjacent zones never holds both.
        changes.sort_by_key(|(_, active)| *active);
        let mut switch = false;
        for (i, active) in changes {
            switch |= self.digital(Source::Zone(analog, i), &zones[i].action, active, out);
        }
        switch
    }

    fn stick_pos(&self, s: Stick, deadzone: f32) -> (f32, f32) {
        let (ax, ay) = stick_axes(s);
        let x = self.axes.get(&ax).copied().unwrap_or(0.0);
        let y = self.axes.get(&ay).copied().unwrap_or(0.0);
        apply_deadzone(x, y, deadzone)
    }

    fn stick(&mut self, profile: &Profile, s: Stick, out: &mut Vec<OutEvent>) -> bool {
        let cfg = profile.stick(s);
        let (x, y) = self.stick_pos(s, cfg.deadzone);
        match &cfg.action {
            StickAction::Gamepad { stick, invert_y } => {
                let (ox, oy) = stick_axes(*stick);
                out.push(OutEvent::PadAxis(ox, x));
                out.push(OutEvent::PadAxis(oy, if *invert_y { -y } else { y }));
            }
            StickAction::Keys { up, down, left, right } => {
                let mut want = Vec::new();
                let t = cfg.key_threshold;
                for (active, name) in [(y < -t, up), (y > t, down), (x < -t, left), (x > t, right)] {
                    if active && let Some(k) = parse_key(name) {
                        want.push(k);
                    }
                }
                let held = self.stick_keys.entry(s).or_default();
                for k in held.iter().filter(|k| !want.contains(k)) {
                    out.push(OutEvent::Key(*k, false));
                }
                for k in want.iter().filter(|k| !held.contains(k)) {
                    out.push(OutEvent::Key(*k, true));
                }
                *held = want;
            }
            // Mouse and scroll are continuous and driven by `tick`.
            StickAction::Mouse { .. } | StickAction::Scroll { .. } | StickAction::Disabled => {}
        }
        self.zones(profile, Analog::Stick(s), x.hypot(y).min(1.0), out)
    }

    /// Whether `tick` currently has anything to do: a mouse/scroll stick is deflected or a
    /// wheel action is held.
    pub fn needs_tick(&self, profile: &Profile) -> bool {
        let sticks = [Stick::Left, Stick::Right].into_iter().any(|s| {
            let cfg = profile.stick(s);
            matches!(cfg.action, StickAction::Mouse { .. } | StickAction::Scroll { .. })
                && self.stick_pos(s, cfg.deadzone) != (0.0, 0.0)
        });
        sticks || self.held.values().any(|a| !a.wheel_directions().is_empty())
    }

    /// Advances continuous outputs (mouse motion, scrolling) by `dt` seconds.
    pub fn tick(&mut self, profile: &Profile, dt: f32, out: &mut Vec<OutEvent>) {
        for s in [Stick::Left, Stick::Right] {
            let cfg = profile.stick(s);
            let (x, y) = self.stick_pos(s, cfg.deadzone);
            let mag = x.hypot(y).min(1.0);
            if mag == 0.0 {
                continue;
            }
            let gain = mag.powf(cfg.curve.max(0.1)) / mag;
            let (x, y) = (x * gain, y * gain);
            match cfg.action {
                StickAction::Mouse { speed } => {
                    let (dx, dy) = take_whole(&mut self.mouse_acc, x * speed * dt, y * speed * dt);
                    if dx != 0 || dy != 0 {
                        out.push(OutEvent::MouseMove(dx, dy));
                    }
                }
                StickAction::Scroll { speed } => {
                    let units = speed * WHEEL_UNITS_PER_NOTCH * dt;
                    // Stick up scrolls up, which is a positive wheel value.
                    let (h, v) = take_whole(&mut self.scroll_acc, x * units, -y * units);
                    if h != 0 || v != 0 {
                        out.push(OutEvent::Wheel { vertical: v, horizontal: h });
                    }
                }
                _ => {}
            }
        }
        self.tick_wheel(dt, out);
    }

    /// Keeps scrolling for wheel actions held past the repeat delay.
    fn tick_wheel(&mut self, dt: f32, out: &mut Vec<OutEvent>) {
        self.wheel_held.retain(|src, _| self.held.contains_key(src));
        let (mut h, mut v) = (0.0, 0.0);
        for (src, action) in &self.held {
            let directions = action.wheel_directions();
            if directions.is_empty() {
                continue;
            }
            let held_for = self.wheel_held.entry(src.clone()).or_insert(0.0);
            *held_for += dt;
            if *held_for > WHEEL_REPEAT_DELAY {
                for d in directions {
                    let (dh, dv) = d.vector();
                    h += dh;
                    v += dv;
                }
            }
        }
        let units = WHEEL_REPEAT_RATE * WHEEL_UNITS_PER_NOTCH * dt;
        let (h, v) = take_whole(&mut self.wheel_acc, h * units, v * units);
        if h != 0 || v != 0 {
            out.push(OutEvent::Wheel { vertical: v, horizontal: h });
        }
    }

    /// Releases everything this engine holds down and centers the virtual pad. Used before a
    /// profile switch or when the device goes away.
    pub fn release_all(&mut self, out: &mut Vec<OutEvent>) {
        for (_, action) in self.held.drain() {
            emit_action(&action, false, out);
        }
        // Held combo members stay silent until released rather than firing under a new profile.
        for m in self.members.values_mut() {
            *m = ComboMember::Consumed;
        }
        // Unfinished tap sequences are dropped.
        self.gestures.clear();
        for (_, keys) in self.stick_keys.drain() {
            out.extend(keys.into_iter().map(|k| OutEvent::Key(k, false)));
        }
        for axis in [Axis::LeftX, Axis::LeftY, Axis::RightX, Axis::RightY, Axis::LeftTrigger, Axis::RightTrigger] {
            out.push(OutEvent::PadAxis(axis, 0.0));
        }
        self.mouse_acc = (0.0, 0.0);
        self.scroll_acc = (0.0, 0.0);
        self.wheel_held.clear();
        self.wheel_acc = (0.0, 0.0);
    }

    /// Re-applies current analog state under a (new) profile, e.g. after switching.
    pub fn resync(&mut self, profile: &Profile, out: &mut Vec<OutEvent>) {
        self.stick(profile, Stick::Left, out);
        self.stick(profile, Stick::Right, out);
        for (t, axis) in [(Trigger::Left, Axis::LeftTrigger), (Trigger::Right, Axis::RightTrigger)] {
            let value = self.axes.get(&axis).copied().unwrap_or(0.0);
            self.trigger(profile, t, value, out);
        }
    }
}

/// Emits press/release for an action. Returns true if it requests the next profile.
fn emit_action(action: &ButtonAction, pressed: bool, out: &mut Vec<OutEvent>) -> bool {
    match action {
        ButtonAction::Disabled => {}
        ButtonAction::Gamepad(b) => out.push(OutEvent::PadButton(*b, pressed)),
        ButtonAction::Mouse(m) => out.push(OutEvent::MouseButton(*m, pressed)),
        // One notch right away; `tick_wheel` continues while held.
        ButtonAction::Wheel(d) => {
            if pressed {
                let (h, v) = d.vector();
                out.push(OutEvent::Wheel {
                    vertical: (v * WHEEL_UNITS_PER_NOTCH) as i32,
                    horizontal: (h * WHEEL_UNITS_PER_NOTCH) as i32,
                });
            }
        }
        ButtonAction::Keys(keys) => {
            let codes = keys.iter().filter_map(|k| parse_key(k));
            // Press modifiers first, release them last.
            if pressed {
                out.extend(codes.map(|k| OutEvent::Key(k, true)));
            } else {
                let codes: Vec<_> = codes.collect();
                out.extend(codes.into_iter().rev().map(|k| OutEvent::Key(k, false)));
            }
        }
        ButtonAction::NextProfile => return pressed,
        ButtonAction::Multi(actions) => {
            let mut switch = false;
            if pressed {
                for a in actions {
                    switch |= emit_action(a, true, out);
                }
            } else {
                for a in actions.iter().rev() {
                    emit_action(a, false, out);
                }
            }
            return switch;
        }
    }
    false
}

pub fn parse_key(name: &str) -> Option<KeyCode> {
    match KeyCode::from_str(name) {
        Ok(k) => Some(k),
        Err(_) => {
            crate::monitor::log!("unknown key name in config: {name:?}");
            None
        }
    }
}

fn stick_axes(s: Stick) -> (Axis, Axis) {
    match s {
        Stick::Left => (Axis::LeftX, Axis::LeftY),
        Stick::Right => (Axis::RightX, Axis::RightY),
    }
}

fn trigger_axis(t: Trigger) -> Axis {
    match t {
        Trigger::Left => Axis::LeftTrigger,
        Trigger::Right => Axis::RightTrigger,
    }
}

/// Radial deadzone, rescaled so output still spans the full 0..1 range.
fn apply_deadzone(x: f32, y: f32, deadzone: f32) -> (f32, f32) {
    let mag = x.hypot(y);
    if mag <= deadzone || mag == 0.0 {
        return (0.0, 0.0);
    }
    let scaled = ((mag - deadzone) / (1.0 - deadzone).max(f32::EPSILON)).min(1.0);
    (x * scaled / mag, y * scaled / mag)
}

/// Adds a delta to a sub-unit accumulator and returns the whole units ready to emit.
fn take_whole(acc: &mut (f32, f32), dx: f32, dy: f32) -> (i32, i32) {
    acc.0 += dx;
    acc.1 += dy;
    let whole = (acc.0.trunc(), acc.1.trunc());
    acc.0 -= whole.0;
    acc.1 -= whole.1;
    (whole.0 as i32, whole.1 as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Combo, MouseButton, StickConfig};

    fn run(engine: &mut Engine, profile: &Profile, ev: InputEvent) -> Vec<OutEvent> {
        let mut out = Vec::new();
        engine.handle(profile, ev, Instant::now(), &mut out);
        out
    }

    #[test]
    fn passthrough_button_and_dpad() {
        let p = Profile::passthrough("p");
        let mut e = Engine::default();
        assert_eq!(
            run(&mut e, &p, InputEvent::Button(Button::South, true)),
            vec![OutEvent::PadButton(Button::South, true)]
        );
        assert_eq!(
            run(&mut e, &p, InputEvent::Button(Button::South, false)),
            vec![OutEvent::PadButton(Button::South, false)]
        );
    }

    #[test]
    fn guide_requests_next_profile_only_on_press() {
        let p = Profile::passthrough("p");
        let mut e = Engine::default();
        let mut out = Vec::new();
        assert!(e.handle(&p, InputEvent::Button(Button::Guide, true), Instant::now(), &mut out));
        assert!(!e.handle(&p, InputEvent::Button(Button::Guide, false), Instant::now(), &mut out));
        assert!(out.is_empty());
    }

    #[test]
    fn release_uses_action_from_press_time() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::South, ButtonAction::Mouse(MouseButton::Left));
        let mut e = Engine::default();
        run(&mut e, &p, InputEvent::Button(Button::South, true));
        // Profile changes while held: release must still go to the mouse button.
        p.set_button(Button::South, ButtonAction::Gamepad(Button::South));
        assert_eq!(
            run(&mut e, &p, InputEvent::Button(Button::South, false)),
            vec![OutEvent::MouseButton(MouseButton::Left, false)]
        );
    }

    #[test]
    fn chord_releases_in_reverse() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::West, ButtonAction::Keys(vec!["KEY_LEFTCTRL".into(), "KEY_C".into()]));
        let mut e = Engine::default();
        run(&mut e, &p, InputEvent::Button(Button::West, true));
        assert_eq!(
            run(&mut e, &p, InputEvent::Button(Button::West, false)),
            vec![OutEvent::Key(KeyCode::KEY_C, false), OutEvent::Key(KeyCode::KEY_LEFTCTRL, false)]
        );
    }

    #[test]
    fn trigger_threshold_with_hysteresis() {
        let p = Profile::desktop("d"); // right trigger = left click at 0.5
        let mut e = Engine::default();
        let click = |down| vec![OutEvent::MouseButton(MouseButton::Left, down)];
        assert!(run(&mut e, &p, InputEvent::Axis(Axis::RightTrigger, 0.4)).is_empty());
        assert_eq!(run(&mut e, &p, InputEvent::Axis(Axis::RightTrigger, 0.6)), click(true));
        assert!(run(&mut e, &p, InputEvent::Axis(Axis::RightTrigger, 0.48)).is_empty());
        assert_eq!(run(&mut e, &p, InputEvent::Axis(Axis::RightTrigger, 0.1)), click(false));
    }

    #[test]
    fn stick_to_keys() {
        let mut p = Profile::passthrough("p");
        p.left_stick = StickConfig::new(wasd(), 0.1, 1.0);
        let mut e = Engine::default();
        assert_eq!(
            run(&mut e, &p, InputEvent::Axis(Axis::LeftY, -1.0)),
            vec![OutEvent::Key(KeyCode::KEY_W, true)]
        );
        assert_eq!(
            run(&mut e, &p, InputEvent::Axis(Axis::LeftY, 1.0)),
            vec![OutEvent::Key(KeyCode::KEY_W, false), OutEvent::Key(KeyCode::KEY_S, true)]
        );
    }

    #[test]
    fn mouse_tick_moves_and_accumulates_subpixels() {
        let p = Profile::desktop("d"); // left stick = mouse at 1200 px/s
        let mut e = Engine::default();
        run(&mut e, &p, InputEvent::Axis(Axis::LeftX, 1.0));
        assert!(e.needs_tick(&p));
        let mut out = Vec::new();
        for _ in 0..250 {
            e.tick(&p, 0.004, &mut out);
        }
        let total: i32 = out
            .iter()
            .map(|o| match o {
                OutEvent::MouseMove(dx, _) => *dx,
                _ => 0,
            })
            .sum();
        assert!((1195..=1200).contains(&total), "moved {total}px in 1s");
    }

    #[test]
    fn deadzone_is_radial_and_rescaled() {
        assert_eq!(apply_deadzone(0.05, 0.05, 0.1), (0.0, 0.0));
        let (x, _) = apply_deadzone(1.0, 0.0, 0.2);
        assert!((x - 1.0).abs() < 1e-6);
        let (x, _) = apply_deadzone(0.6, 0.0, 0.2);
        assert!((x - 0.5).abs() < 1e-6);
    }

    fn combo_profile() -> Profile {
        let mut p = Profile::passthrough("p");
        p.combos.push(Combo {
            buttons: vec![Button::LeftBumper, Button::RightBumper],
            action: ButtonAction::Keys(vec!["KEY_LEFTALT".into(), "KEY_TAB".into()]),
        });
        p
    }

    fn step(e: &mut Engine, p: &Profile, ev: InputEvent, at: Instant) -> Vec<OutEvent> {
        let mut out = Vec::new();
        e.handle(p, ev, at, &mut out);
        out
    }

    #[test]
    fn combo_fires_instead_of_members() {
        let p = combo_profile();
        let mut e = Engine::default();
        let t0 = Instant::now();
        assert!(step(&mut e, &p, InputEvent::Button(Button::LeftBumper, true), t0).is_empty());
        assert_eq!(
            step(&mut e, &p, InputEvent::Button(Button::RightBumper, true), t0 + Duration::from_millis(20)),
            vec![OutEvent::Key(KeyCode::KEY_LEFTALT, true), OutEvent::Key(KeyCode::KEY_TAB, true)]
        );
        // Window passing must not fire the consumed members.
        let mut out = Vec::new();
        e.timers(&p, t0 + Duration::from_secs(1), &mut out);
        assert!(out.is_empty());
        // Releasing either member releases the combo; the other release is silent.
        assert_eq!(
            step(&mut e, &p, InputEvent::Button(Button::RightBumper, false), t0),
            vec![OutEvent::Key(KeyCode::KEY_TAB, false), OutEvent::Key(KeyCode::KEY_LEFTALT, false)]
        );
        assert!(step(&mut e, &p, InputEvent::Button(Button::LeftBumper, false), t0).is_empty());
    }

    #[test]
    fn combo_member_acts_alone_after_window() {
        let p = combo_profile();
        let mut e = Engine::default();
        let t0 = Instant::now();
        step(&mut e, &p, InputEvent::Button(Button::LeftBumper, true), t0);
        assert_eq!(e.next_deadline(), Some(t0 + Duration::from_millis(60)));
        let mut out = Vec::new();
        e.timers(&p, t0 + Duration::from_millis(59), &mut out);
        assert!(out.is_empty());
        e.timers(&p, t0 + Duration::from_millis(60), &mut out);
        assert_eq!(out, vec![OutEvent::PadButton(Button::LeftBumper, true)]);
        // Too late for the combo: RB acts on its own too (after its own window).
        assert!(step(&mut e, &p, InputEvent::Button(Button::RightBumper, true), t0 + Duration::from_millis(100)).is_empty());
        assert_eq!(
            step(&mut e, &p, InputEvent::Button(Button::LeftBumper, false), t0),
            vec![OutEvent::PadButton(Button::LeftBumper, false)]
        );
    }

    #[test]
    fn quick_tap_of_combo_member_still_registers() {
        let p = combo_profile();
        let mut e = Engine::default();
        let t0 = Instant::now();
        step(&mut e, &p, InputEvent::Button(Button::LeftBumper, true), t0);
        assert_eq!(
            step(&mut e, &p, InputEvent::Button(Button::LeftBumper, false), t0 + Duration::from_millis(10)),
            vec![
                OutEvent::PadButton(Button::LeftBumper, true),
                OutEvent::PadButton(Button::LeftBumper, false)
            ]
        );
        assert_eq!(e.next_deadline(), None);
    }

    #[test]
    fn disabled_member_works_as_modifier() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::Guide, ButtonAction::Disabled);
        p.combos.push(Combo {
            buttons: vec![Button::Guide, Button::South],
            action: ButtonAction::NextProfile,
        });
        let mut e = Engine::default();
        let t0 = Instant::now();
        step(&mut e, &p, InputEvent::Button(Button::Guide, true), t0);
        assert_eq!(e.next_deadline(), None);
        let mut out = Vec::new();
        assert!(e.handle(&p, InputEvent::Button(Button::South, true), t0 + Duration::from_secs(5), &mut out));
    }

    #[test]
    fn multi_presses_in_order_and_releases_in_reverse() {
        let mut p = Profile::passthrough("p");
        p.set_button(
            Button::South,
            ButtonAction::Multi(vec![
                ButtonAction::Gamepad(Button::South),
                ButtonAction::Keys(vec!["KEY_X".into()]),
                ButtonAction::Mouse(MouseButton::Left),
            ]),
        );
        let mut e = Engine::default();
        assert_eq!(
            run(&mut e, &p, InputEvent::Button(Button::South, true)),
            vec![
                OutEvent::PadButton(Button::South, true),
                OutEvent::Key(KeyCode::KEY_X, true),
                OutEvent::MouseButton(MouseButton::Left, true)
            ]
        );
        assert_eq!(
            run(&mut e, &p, InputEvent::Button(Button::South, false)),
            vec![
                OutEvent::MouseButton(MouseButton::Left, false),
                OutEvent::Key(KeyCode::KEY_X, false),
                OutEvent::PadButton(Button::South, false)
            ]
        );
    }

    fn gesture_profile() -> Profile {
        let mut p = Profile::passthrough("p");
        p.gestures.insert(
            Button::North,
            crate::config::Gestures {
                double_tap: Some(ButtonAction::Keys(vec!["KEY_2".into()])),
                triple_tap: Some(ButtonAction::Keys(vec!["KEY_3".into()])),
                long_press: Some(ButtonAction::Keys(vec!["KEY_L".into()])),
            },
        );
        p
    }

    fn ms(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_millis(n)
    }

    fn press(e: &mut Engine, p: &Profile, b: Button, down: bool, at: Instant) -> Vec<OutEvent> {
        step(e, p, InputEvent::Button(b, down), at)
    }

    fn fire_timers(e: &mut Engine, p: &Profile, at: Instant) -> Vec<OutEvent> {
        let mut out = Vec::new();
        e.timers(p, at, &mut out);
        out
    }

    fn key_tap(k: KeyCode) -> Vec<OutEvent> {
        vec![OutEvent::Key(k, true), OutEvent::Key(k, false)]
    }

    #[test]
    fn single_tap_fires_normal_action_after_window() {
        let p = gesture_profile();
        let mut e = Engine::default();
        let t0 = Instant::now();
        assert!(press(&mut e, &p, Button::North, true, t0).is_empty());
        assert!(press(&mut e, &p, Button::North, false, ms(t0, 80)).is_empty());
        assert_eq!(e.next_deadline(), Some(ms(t0, 330)));
        assert!(fire_timers(&mut e, &p, ms(t0, 329)).is_empty());
        assert_eq!(
            fire_timers(&mut e, &p, ms(t0, 330)),
            vec![OutEvent::PadButton(Button::North, true), OutEvent::PadButton(Button::North, false)]
        );
        assert_eq!(e.next_deadline(), None);
    }

    #[test]
    fn double_tap_resolves_after_window_when_triple_is_possible() {
        let p = gesture_profile();
        let mut e = Engine::default();
        let t0 = Instant::now();
        press(&mut e, &p, Button::North, true, t0);
        press(&mut e, &p, Button::North, false, ms(t0, 60));
        assert!(press(&mut e, &p, Button::North, true, ms(t0, 150)).is_empty());
        assert!(press(&mut e, &p, Button::North, false, ms(t0, 200)).is_empty());
        assert_eq!(fire_timers(&mut e, &p, ms(t0, 450)), key_tap(KeyCode::KEY_2));
    }

    #[test]
    fn triple_tap_fires_on_third_press_and_holds() {
        let p = gesture_profile();
        let mut e = Engine::default();
        let t0 = Instant::now();
        for (down, at) in [(true, 0), (false, 50), (true, 120), (false, 170)] {
            assert!(press(&mut e, &p, Button::North, down, ms(t0, at)).is_empty());
        }
        assert_eq!(
            press(&mut e, &p, Button::North, true, ms(t0, 240)),
            vec![OutEvent::Key(KeyCode::KEY_3, true)]
        );
        assert!(fire_timers(&mut e, &p, ms(t0, 5000)).is_empty());
        assert_eq!(
            press(&mut e, &p, Button::North, false, ms(t0, 5000)),
            vec![OutEvent::Key(KeyCode::KEY_3, false)]
        );
    }

    #[test]
    fn long_press_fires_while_held_and_releases_on_release() {
        let p = gesture_profile();
        let mut e = Engine::default();
        let t0 = Instant::now();
        press(&mut e, &p, Button::North, true, t0);
        assert!(fire_timers(&mut e, &p, ms(t0, 499)).is_empty());
        assert_eq!(fire_timers(&mut e, &p, ms(t0, 500)), vec![OutEvent::Key(KeyCode::KEY_L, true)]);
        assert_eq!(
            press(&mut e, &p, Button::North, false, ms(t0, 900)),
            vec![OutEvent::Key(KeyCode::KEY_L, false)]
        );
        assert_eq!(e.next_deadline(), None);
    }

    #[test]
    fn long_press_only_button_taps_on_release() {
        let mut p = Profile::passthrough("p");
        p.gestures.insert(
            Button::South,
            crate::config::Gestures { long_press: Some(ButtonAction::NextProfile), ..Default::default() },
        );
        let mut e = Engine::default();
        let t0 = Instant::now();
        press(&mut e, &p, Button::South, true, t0);
        assert_eq!(
            press(&mut e, &p, Button::South, false, ms(t0, 100)),
            vec![OutEvent::PadButton(Button::South, true), OutEvent::PadButton(Button::South, false)]
        );
        // Held past the threshold: switches profile instead.
        press(&mut e, &p, Button::South, true, ms(t0, 1000));
        let mut out = Vec::new();
        assert!(e.timers(&p, ms(t0, 1500), &mut out));
    }

    #[test]
    fn combo_member_with_gestures_double_taps_after_combo_window() {
        let mut p = combo_profile(); // LB+RB combo, 60 ms window
        p.gestures.insert(
            Button::LeftBumper,
            crate::config::Gestures { double_tap: Some(ButtonAction::Keys(vec!["KEY_D".into()])), ..Default::default() },
        );
        let mut e = Engine::default();
        let t0 = Instant::now();
        // Two quick taps, each released inside the combo window.
        press(&mut e, &p, Button::LeftBumper, true, t0);
        assert!(press(&mut e, &p, Button::LeftBumper, false, ms(t0, 30)).is_empty());
        press(&mut e, &p, Button::LeftBumper, true, ms(t0, 100));
        assert_eq!(press(&mut e, &p, Button::LeftBumper, false, ms(t0, 130)), key_tap(KeyCode::KEY_D));
    }

    fn wasd() -> StickAction {
        StickAction::Keys {
            up: "KEY_W".into(),
            down: "KEY_S".into(),
            left: "KEY_A".into(),
            right: "KEY_D".into(),
        }
    }

    fn axis(e: &mut Engine, p: &Profile, axis: Axis, v: f32) -> Vec<OutEvent> {
        run(e, p, InputEvent::Axis(axis, v))
    }

    #[test]
    fn partial_push_walks_full_push_runs() {
        let mut p = Profile::passthrough("p");
        p.left_stick = StickConfig::new(wasd(), 0.0, 1.0);
        p.left_stick.key_threshold = 0.2;
        p.left_stick.zones.push(crate::config::Zone {
            min: 0.0,
            max: 0.75,
            action: ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into()]),
        });
        let mut e = Engine::default();
        let shift = KeyCode::KEY_LEFTSHIFT;
        assert_eq!(
            axis(&mut e, &p, Axis::LeftY, -0.4),
            vec![OutEvent::Key(KeyCode::KEY_W, true), OutEvent::Key(shift, true)]
        );
        // Just past the edge: hysteresis keeps walking.
        assert!(axis(&mut e, &p, Axis::LeftY, -0.76).is_empty());
        assert_eq!(axis(&mut e, &p, Axis::LeftY, -1.0), vec![OutEvent::Key(shift, false)]);
        assert_eq!(axis(&mut e, &p, Axis::LeftY, -0.5), vec![OutEvent::Key(shift, true)]);
        assert_eq!(
            axis(&mut e, &p, Axis::LeftY, 0.0),
            vec![OutEvent::Key(KeyCode::KEY_W, false), OutEvent::Key(shift, false)]
        );
    }

    #[test]
    fn trigger_zones_layer_on_top_of_main_action() {
        let mut p = Profile::passthrough("p"); // RT = gamepad trigger
        p.right_trigger.zones = vec![
            crate::config::Zone { min: 0.3, max: 0.9, action: ButtonAction::Mouse(MouseButton::Right) },
            crate::config::Zone { min: 0.9, max: 1.0, action: ButtonAction::Mouse(MouseButton::Left) },
        ];
        let mut e = Engine::default();
        assert_eq!(
            axis(&mut e, &p, Axis::RightTrigger, 0.5),
            vec![OutEvent::PadAxis(Axis::RightTrigger, 0.5), OutEvent::MouseButton(MouseButton::Right, true)]
        );
        // Moving into the next zone releases the first before pressing the second.
        assert_eq!(
            axis(&mut e, &p, Axis::RightTrigger, 1.0),
            vec![
                OutEvent::PadAxis(Axis::RightTrigger, 1.0),
                OutEvent::MouseButton(MouseButton::Right, false),
                OutEvent::MouseButton(MouseButton::Left, true)
            ]
        );
        assert_eq!(
            axis(&mut e, &p, Axis::RightTrigger, 0.0),
            vec![OutEvent::PadAxis(Axis::RightTrigger, 0.0), OutEvent::MouseButton(MouseButton::Left, false)]
        );
    }

    #[test]
    fn zone_from_zero_is_inactive_at_rest() {
        let mut p = Profile::passthrough("p");
        p.left_stick.deadzone = 0.2;
        p.left_stick.zones.push(crate::config::Zone { min: 0.0, max: 1.0, action: ButtonAction::Keys(vec!["KEY_Q".into()]) });
        let mut e = Engine::default();
        // Inside the deadzone counts as rest.
        assert!(!axis(&mut e, &p, Axis::LeftX, 0.1).contains(&OutEvent::Key(KeyCode::KEY_Q, true)));
        assert!(axis(&mut e, &p, Axis::LeftX, 0.5).contains(&OutEvent::Key(KeyCode::KEY_Q, true)));
    }

    #[test]
    fn pc_action_template_drives_keyboard_and_mouse() {
        let p = Profile::pc_action("a");
        let mut e = Engine::default();
        let has = |out: &[OutEvent], ev: OutEvent| out.contains(&ev);

        assert!(has(&axis(&mut e, &p, Axis::RightTrigger, 1.0), OutEvent::MouseButton(MouseButton::Left, true)));
        assert!(has(&axis(&mut e, &p, Axis::LeftTrigger, 1.0), OutEvent::MouseButton(MouseButton::Right, true)));
        assert_eq!(run(&mut e, &p, InputEvent::Button(Button::South, true)), vec![OutEvent::Key(KeyCode::KEY_E, true)]);

        // Partial push walks forward; full push adds sprint.
        assert_eq!(axis(&mut e, &p, Axis::LeftY, -0.5), vec![OutEvent::Key(KeyCode::KEY_W, true)]);
        assert_eq!(axis(&mut e, &p, Axis::LeftY, -1.0), vec![OutEvent::Key(KeyCode::KEY_LEFTSHIFT, true)]);

        // Right stick is mouse look.
        axis(&mut e, &p, Axis::RightX, 1.0);
        assert!(e.needs_tick(&p));
        let mut out = Vec::new();
        e.tick(&p, 0.1, &mut out);
        assert!(matches!(out.as_slice(), [OutEvent::MouseMove(dx, 0)] if *dx > 0), "{out:?}");
    }

    fn wheel_total(out: &[OutEvent]) -> (i32, i32) {
        out.iter().fold((0, 0), |(v, h), ev| match ev {
            OutEvent::Wheel { vertical, horizontal } => (v + vertical, h + horizontal),
            _ => (v, h),
        })
    }

    fn hold_for(e: &mut Engine, p: &Profile, seconds: f32) -> Vec<OutEvent> {
        let mut out = Vec::new();
        let steps = (seconds / 0.004).round() as usize;
        for _ in 0..steps {
            e.tick(p, 0.004, &mut out);
        }
        out
    }

    #[test]
    fn wheel_button_scrolls_a_notch_then_repeats_while_held() {
        use crate::config::WheelDirection;
        let mut p = Profile::passthrough("p");
        p.set_button(Button::RightBumper, ButtonAction::Wheel(WheelDirection::Down));
        let mut e = Engine::default();
        assert_eq!(
            run(&mut e, &p, InputEvent::Button(Button::RightBumper, true)),
            vec![OutEvent::Wheel { vertical: -120, horizontal: 0 }]
        );
        assert!(e.needs_tick(&p));
        // Nothing more during the repeat delay...
        assert_eq!(wheel_total(&hold_for(&mut e, &p, 0.3)), (0, 0));
        // ...then ~10 notches/s: 0.65 s more is ~0.6 s past the delay, about 6 notches.
        let (v, h) = wheel_total(&hold_for(&mut e, &p, 0.65));
        assert!((-720..=-600).contains(&v) && h == 0, "scrolled {v}");
        // Releasing stops it.
        assert!(run(&mut e, &p, InputEvent::Button(Button::RightBumper, false)).is_empty());
        assert!(!e.needs_tick(&p));
        assert_eq!(wheel_total(&hold_for(&mut e, &p, 1.0)), (0, 0));
    }

    #[test]
    fn wheel_inside_multi_and_horizontal() {
        use crate::config::WheelDirection;
        let mut p = Profile::passthrough("p");
        p.set_button(
            Button::West,
            ButtonAction::Multi(vec![
                ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into()]),
                ButtonAction::Wheel(WheelDirection::Right),
            ]),
        );
        let mut e = Engine::default();
        assert_eq!(
            run(&mut e, &p, InputEvent::Button(Button::West, true)),
            vec![OutEvent::Key(KeyCode::KEY_LEFTSHIFT, true), OutEvent::Wheel { vertical: 0, horizontal: 120 }]
        );
        let (v, h) = wheel_total(&hold_for(&mut e, &p, 1.0));
        assert!(v == 0 && h > 0, "{v} {h}");
    }
}

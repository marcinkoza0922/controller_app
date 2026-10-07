//! Remaps the physical keyboard and mouse through the active keyboard profile.
//!
//! The daemon grabs the devices and feeds their events here as [`RawEvent`]s; what comes out
//! goes to the virtual keyboard, mouse and (if the profile uses one) gamepad. Nothing here
//! touches a device, so it is all testable. The actions themselves run in an [`Engine`], the
//! same one gamepads use.

use std::{collections::HashSet, time::Instant};

use evdev::KeyCode;

use crate::{
    config::{
        ButtonAction, KeyboardMap, Macro, Menu, MotionDirection, MotionTarget, MouseButton, OtherKeys, Profile, Stick, WheelDirection,
        default_panic_chord, is_reserved_key,
    },
    engine::{Engine, RawInput, input_name, parse_key},
    input::{Axis, InputEvent},
    output::OutEvent,
};

mod device;

pub use device::{classify, read_device};

/// One thing the keyboard or mouse did. Key repeats are left out; the desktop repeats keys
/// itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawEvent {
    Key(KeyCode, bool),
    Button(MouseButton, bool),
    Move(i32, i32),
    /// High-resolution wheel units (120 = one notch).
    Wheel { vertical: i32, horizontal: i32 },
}

/// Wheel units in one notch.
const NOTCH: i32 = 120;
/// How far the mouse has to have pushed (0..1) for a movement direction to press, and how far
/// it falls back before the direction lets go.
const DIRECTION_PRESS: f32 = 0.5;
const DIRECTION_RELEASE: f32 = 0.4;
/// A push this small counts as none.
const REST: f32 = 0.005;

pub struct Remapper {
    engine: Engine,
    /// When the event being handled happened.
    now: Instant,
    /// Keys physically down, for the panic chord and the console-switch exception.
    down: HashSet<KeyCode>,
    chord: Vec<KeyCode>,
    /// Inputs sent on unchanged, which let go the same way.
    passing: HashSet<RawInput>,
    /// Inputs with gestures or combos, which the engine decides about.
    routed: HashSet<RawInput>,
    /// An action asked for the next profile.
    switch: bool,
    /// How far the mouse has pushed, -1..1 each way (y down), fading back to the middle.
    push: (f32, f32),
    /// The pointer's fractional movement, when it is scaled.
    pointer_acc: (f32, f32),
    wheel_acc: (i32, i32),
    /// Movement directions pressed now.
    directions: HashSet<MotionDirection>,
}

impl Default for Remapper {
    fn default() -> Self {
        Remapper {
            engine: Engine::default(),
            now: Instant::now(),
            down: HashSet::new(),
            chord: Vec::new(),
            passing: HashSet::new(),
            routed: HashSet::new(),
            switch: false,
            push: (0.0, 0.0),
            pointer_acc: (0.0, 0.0),
            wheel_acc: (0, 0),
            directions: HashSet::new(),
        }
    }
}

/// The other half of a modifier, so either Ctrl, Alt or Shift satisfies a chord.
fn sibling(key: KeyCode) -> Option<KeyCode> {
    Some(match key {
        KeyCode::KEY_LEFTCTRL => KeyCode::KEY_RIGHTCTRL,
        KeyCode::KEY_RIGHTCTRL => KeyCode::KEY_LEFTCTRL,
        KeyCode::KEY_LEFTALT => KeyCode::KEY_RIGHTALT,
        KeyCode::KEY_RIGHTALT => KeyCode::KEY_LEFTALT,
        KeyCode::KEY_LEFTSHIFT => KeyCode::KEY_RIGHTSHIFT,
        KeyCode::KEY_RIGHTSHIFT => KeyCode::KEY_LEFTSHIFT,
        _ => return None,
    })
}

fn is_function_key(key: KeyCode) -> bool {
    (KeyCode::KEY_F1.0..=KeyCode::KEY_F10.0).contains(&key.0) || matches!(key, KeyCode::KEY_F11 | KeyCode::KEY_F12)
}

/// What an input does: its own mapping, else itself or nothing, as the profile says.
enum Plan {
    /// Sent on as it is, whatever the profile says: Super, and the console switches.
    Reserved,
    Pass,
    Run(ButtonAction),
}

fn plan(mapped: Option<&ButtonAction>, other: OtherKeys) -> Plan {
    match (mapped, other) {
        (Some(action), _) => Plan::Run(action.clone()),
        (None, OtherKeys::Pass) => Plan::Pass,
        (None, OtherKeys::Block) => Plan::Run(ButtonAction::Disabled),
    }
}

impl Remapper {
    pub fn set_chord(&mut self, chord: &[String]) {
        self.chord = chord.iter().filter_map(|n| parse_key(n)).collect();
        // Never leave the escape hatch empty.
        if self.chord.is_empty() {
            self.chord = default_panic_chord().iter().filter_map(|n| parse_key(n)).collect();
        }
    }

    fn is_down(&self, key: KeyCode) -> bool {
        self.down.contains(&key) || sibling(key).is_some_and(|s| self.down.contains(&s))
    }

    /// Handles one event. True when it completed the panic chord: everything is already
    /// released, and the caller should turn remapping off.
    pub fn event(&mut self, profile: &Profile, ev: RawEvent, out: &mut Vec<OutEvent>) -> bool {
        self.event_at(profile, ev, Instant::now(), out)
    }

    pub fn event_at(&mut self, profile: &Profile, ev: RawEvent, now: Instant, out: &mut Vec<OutEvent>) -> bool {
        self.now = now;
        let map = &profile.keyboard;
        match ev {
            RawEvent::Move(dx, dy) => self.moved(profile, dx, dy, out),
            RawEvent::Wheel { vertical, horizontal } => self.wheel(map, vertical, horizontal, out),
            RawEvent::Key(key, pressed) => {
                if pressed {
                    self.down.insert(key);
                } else {
                    self.down.remove(&key);
                }
                if pressed && self.chord.contains(&key) && self.chord.iter().all(|k| self.is_down(*k)) {
                    self.release_all(out);
                    return true;
                }
                let plan = pressed.then(|| self.key_plan(map, key));
                self.input(profile, RawInput::Key(key), plan, out);
            }
            RawEvent::Button(button, pressed) => {
                let plan = pressed.then(|| plan(map.mouse.get(&button), map.other_keys));
                self.input(profile, RawInput::Button(button), plan, out);
            }
        }
        false
    }

    fn is_console_switch(&self, key: KeyCode) -> bool {
        is_function_key(key) && self.is_down(KeyCode::KEY_LEFTCTRL) && self.is_down(KeyCode::KEY_LEFTALT)
    }

    /// An input went down (with its plan) or up (`None`).
    fn input(&mut self, profile: &Profile, input: RawInput, plan: Option<Plan>, out: &mut Vec<OutEvent>) {
        let itself = match input {
            RawInput::Key(k) => OutEvent::Key(k, true),
            RawInput::Button(b) => OutEvent::MouseButton(b, true),
            RawInput::Wheel(_) | RawInput::Motion(_) => return,
        };
        let switch = match plan {
            None if self.passing.remove(&input) => {
                out.push(release_of(&itself));
                false
            }
            None if self.routed.remove(&input) => self.engine.handle_raw(profile, (input, false), self.now, out),
            None => self.engine.run_raw(input, &ButtonAction::Disabled, false, out),
            Some(Plan::Pass | Plan::Run(_)) if self.has_gestures_or_combos(profile, input) => {
                self.routed.insert(input);
                self.engine.handle_raw(profile, (input, true), self.now, out)
            }
            Some(Plan::Reserved | Plan::Pass) => {
                if self.passing.insert(input) {
                    out.push(itself);
                }
                false
            }
            Some(Plan::Run(action)) => self.engine.run_raw(input, &action, true, out),
        };
        self.switch |= switch;
    }

    /// Whether the profile gives `input` gestures or puts it in a combo.
    fn has_gestures_or_combos(&self, profile: &Profile, input: RawInput) -> bool {
        let Some(name) = input_name(input) else { return false };
        let map = &profile.keyboard;
        map.gestures.get(&name).is_some_and(|g| !g.is_empty()) || map.combos.iter().any(|c| c.inputs.len() >= 2 && c.inputs.contains(&name))
    }

    /// Whether an action asked for the next profile since last asked.
    pub fn take_switch(&mut self) -> bool {
        std::mem::take(&mut self.switch)
    }

    /// Lets gesture and combo timers that have run out act.
    pub fn timers(&mut self, profile: &Profile, now: Instant, out: &mut Vec<OutEvent>) {
        self.switch |= self.engine.timers(profile, now, out);
    }

    /// When `timers` next has something to do.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.engine.next_deadline()
    }

    /// What a key press turns into: itself when reserved, else what the profile says.
    fn key_plan(&self, map: &KeyboardMap, key: KeyCode) -> Plan {
        let name = format!("{key:?}");
        if is_reserved_key(&name) || self.is_console_switch(key) {
            return Plan::Reserved;
        }
        plan(map.keys.get(&name), map.other_keys)
    }

    /// Wheel movement: a direction with a mapping presses it once per notch; the rest goes on.
    fn wheel(&mut self, map: &KeyboardMap, vertical: i32, horizontal: i32, out: &mut Vec<OutEvent>) {
        let (mut pass_v, mut pass_h) = (vertical, horizontal);
        let axes = [(vertical, WheelDirection::Up, WheelDirection::Down, 0), (horizontal, WheelDirection::Right, WheelDirection::Left, 1)];
        for (amount, positive, negative, axis) in axes {
            let dir = if amount >= 0 { positive } else { negative };
            let Some(action) = map.wheel.get(&dir) else { continue };
            if axis == 0 {
                pass_v = 0;
            } else {
                pass_h = 0;
            }
            let acc = if axis == 0 { &mut self.wheel_acc.0 } else { &mut self.wheel_acc.1 };
            // Turning the other way starts counting afresh.
            if (*acc > 0) != (amount > 0) && *acc != 0 {
                *acc = 0;
            }
            *acc += amount;
            let notches = *acc / NOTCH;
            *acc -= notches * NOTCH;
            for _ in 0..notches.abs() {
                self.engine.tap_raw(RawInput::Wheel(dir), action, out);
            }
        }
        if pass_v != 0 || pass_h != 0 {
            out.push(OutEvent::Wheel { vertical: pass_v, horizontal: pass_h });
        }
    }

    /// Mouse movement: the pointer (scaled), or a push on a stick, and the direction buttons.
    fn moved(&mut self, profile: &Profile, dx: i32, dy: i32, out: &mut Vec<OutEvent>) {
        let map = &profile.keyboard;
        let m = map.motion;
        let sign = |inverted: bool| if inverted { -1.0 } else { 1.0 };
        let (sx, sy) = (sign(m.invert_x), sign(m.invert_y));
        let pointer = m.target == MotionTarget::Pointer;
        if pointer {
            if m.pointer_scale == 1.0 && !m.invert_x && !m.invert_y {
                out.push(OutEvent::MouseMove(dx, dy));
            } else {
                let (x, y) = (self.pointer_acc.0 + dx as f32 * m.pointer_scale * sx, self.pointer_acc.1 + dy as f32 * m.pointer_scale * sy);
                let (wx, wy) = (x.trunc(), y.trunc());
                self.pointer_acc = (x - wx, y - wy);
                if wx != 0.0 || wy != 0.0 {
                    out.push(OutEvent::MouseMove(wx as i32, wy as i32));
                }
            }
        }
        if pointer && map.motion_buttons.is_empty() {
            return;
        }
        let counts = m.counts.max(1.0);
        let (x, y) = (self.push.0 + dx as f32 * sx / counts, self.push.1 + dy as f32 * sy / counts);
        let length = x.hypot(y);
        self.push = if length > 1.0 { (x / length, y / length) } else { (x, y) };
        self.push_changed(profile, out);
    }

    /// Sends the push to its stick and presses or lets go the movement directions.
    fn push_changed(&mut self, profile: &Profile, out: &mut Vec<OutEvent>) {
        let map = &profile.keyboard;
        if let MotionTarget::Stick(stick) = map.motion.target {
            let (ax, ay) = match stick {
                Stick::Left => (Axis::LeftX, Axis::LeftY),
                Stick::Right => (Axis::RightX, Axis::RightY),
            };
            let now = Instant::now();
            self.engine.handle(profile, InputEvent::Axis(ax, self.push.0), now, out);
            self.engine.handle(profile, InputEvent::Axis(ay, self.push.1), now, out);
        }
        let (x, y) = self.push;
        for dir in MotionDirection::ALL {
            let Some(action) = map.motion_buttons.get(&dir) else { continue };
            let amount = match dir {
                MotionDirection::Up => -y,
                MotionDirection::Down => y,
                MotionDirection::Left => -x,
                MotionDirection::Right => x,
            };
            let input = RawInput::Motion(dir);
            if amount >= DIRECTION_PRESS && self.directions.insert(dir) {
                self.engine.run_raw(input, action, true, out);
            } else if amount < DIRECTION_RELEASE && self.directions.remove(&dir) {
                self.engine.run_raw(input, action, false, out);
            }
        }
    }

    pub fn set_macros(&mut self, macros: &[Macro]) {
        self.engine.set_macros(macros);
    }

    /// Layers held or toggled on, oldest first.
    pub fn layers(&self) -> Vec<String> {
        self.engine.layers()
    }

    pub fn take_layers_changed(&mut self) -> bool {
        self.engine.take_layers_changed()
    }

    pub fn take_info_changed(&mut self) -> bool {
        self.engine.take_info_changed()
    }

    pub fn shown_info(&self) -> impl Iterator<Item = &String> {
        self.engine.shown_info()
    }

    pub fn shown_logs(&self) -> impl Iterator<Item = &String> {
        self.engine.shown_logs()
    }

    /// Switches on the toggles set to start on, when the game starts.
    pub fn start_toggles(&mut self, profile: &Profile, menus: &[Menu], out: &mut Vec<OutEvent>) {
        self.switch |= self.engine.start_toggles(profile, menus, out);
    }

    /// Re-applies the mouse's push under a (new) profile, after layers changed.
    pub fn resync(&mut self, profile: &Profile, out: &mut Vec<OutEvent>) {
        self.push_changed(profile, out);
    }

    /// Whether `tick` has anything to do: a push to fade, or something the engine repeats.
    pub fn needs_tick(&self, profile: &Profile) -> bool {
        self.push != (0.0, 0.0) || self.engine.needs_tick(profile)
    }

    /// Fades the mouse's push and advances what the engine repeats, by `dt` seconds.
    pub fn tick(&mut self, profile: &Profile, dt: f32, out: &mut Vec<OutEvent>) {
        if self.push != (0.0, 0.0) {
            let decay_s = profile.keyboard.motion.decay_ms as f32 / 1000.0;
            let keep = if decay_s > 0.0 { (-dt / decay_s).exp() } else { 0.0 };
            let (x, y) = (self.push.0 * keep, self.push.1 * keep);
            self.push = if x.abs() < REST && y.abs() < REST { (0.0, 0.0) } else { (x, y) };
            self.push_changed(profile, out);
        }
        self.engine.tick(profile, dt, out);
    }

    /// Lets go of everything being held, and forgets which keys are down (on a profile
    /// change, or when the devices are released).
    pub fn release_all(&mut self, out: &mut Vec<OutEvent>) {
        for input in self.passing.drain() {
            out.push(match input {
                RawInput::Key(k) => OutEvent::Key(k, false),
                RawInput::Button(b) => OutEvent::MouseButton(b, false),
                RawInput::Wheel(_) | RawInput::Motion(_) => continue,
            });
        }
        self.engine.release_all(false, out);
        self.routed.clear();
        self.down.clear();
        self.push = (0.0, 0.0);
        self.pointer_acc = (0.0, 0.0);
        self.wheel_acc = (0, 0);
        self.directions.clear();
    }
}

fn release_of(ev: &OutEvent) -> OutEvent {
    match ev {
        OutEvent::Key(k, _) => OutEvent::Key(*k, false),
        OutEvent::MouseButton(b, _) => OutEvent::MouseButton(*b, false),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests;

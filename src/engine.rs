//! Per-device mapping state: turns normalized input into output events for the active profile.

use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use evdev::KeyCode;

use crate::{
    config::{
        Analog, Button, ButtonAction, GestureKind, GyroActivation, GyroConfig, GyroHorizontal, GyroInput, GyroMode,
        Macro, MacroStep, Menu, Profile, Stick, StickAction, Toggled, Trigger, TriggerAction,
    },
    input::{Axis, InputEvent, MotionSample},
    output::OutEvent,
};

mod stick;

/// A stick-direction button releases this far below its press threshold.
const STICK_DIRECTION_HYSTERESIS: f32 = 0.05;
/// A held zone stays active this far past its edges, so it doesn't flicker on a boundary.
const ZONE_HYSTERESIS: f32 = 0.02;
/// A trigger mapped to a button releases this far below its press threshold.
const TRIGGER_HYSTERESIS: f32 = 0.05;
const WHEEL_UNITS_PER_NOTCH: f32 = 120.0;
/// A held wheel action scrolls one notch at once, then continuously after this delay...
const WHEEL_REPEAT_DELAY: f32 = 0.35;
/// ...at this many notches per second.
const WHEEL_REPEAT_RATE: f32 = 10.0;
/// Time constant (seconds) for smoothing the accelerometer tilt used by gyro steering.
const TILT_SMOOTHING: f32 = 0.05;

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
    /// A menu item chosen on screen (menu name, item index).
    MenuItem(String, usize),
}

/// The physical input that opened a menu, which holds the menu up until it is let go.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Opener {
    /// Released when any of these is (stick directions included).
    pub buttons: Vec<Button>,
    /// Released when this trigger drops below the menu's release point.
    pub trigger: Option<Trigger>,
    /// Opened through a Toggle: the menu stays until the opener is pressed again.
    pub toggled: bool,
}

impl Source {
    /// The input this came from, as the input log knows it; `None` for a menu item.
    fn fired_from(&self) -> Option<crate::inputlog::FiredFrom> {
        use crate::inputlog::FiredFrom;
        Some(match self {
            Source::Button(b) | Source::Gesture(b) => FiredFrom::Button(*b),
            Source::Combo(members) => FiredFrom::Combo(members.clone()),
            Source::Trigger(t) | Source::Zone(Analog::Trigger(t), _) => FiredFrom::Trigger(*t),
            Source::Zone(Analog::Stick(s), _) => FiredFrom::Stick(*s),
            Source::MenuItem(..) => return None,
        })
    }

    fn opener(&self) -> Opener {
        match self {
            Source::Button(b) | Source::Gesture(b) => Opener { buttons: vec![*b], ..Opener::default() },
            Source::Combo(members) => Opener { buttons: members.clone(), ..Opener::default() },
            Source::Trigger(t) => Opener { trigger: Some(*t), ..Opener::default() },
            _ => Opener::default(),
        }
    }
}

/// Gesture detection for a button acting alone that has gestures configured.
#[derive(Debug, Clone, Copy, PartialEq)]
enum GestureState {
    /// Held, undecided. `taps` counts presses in this sequence including this one.
    /// `hold_deadline`: held this long, a first press is a plain hold, not a tap.
    Down { taps: u8, long_deadline: Option<Instant>, hold_deadline: Option<Instant> },
    /// Released, waiting to see whether another tap follows.
    Up { taps: u8, deadline: Instant },
    /// A gesture action fired and is held until release.
    Holding,
    /// Held past the tap window: the button's own action is pressed until release.
    Pressed,
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
    /// Keys held by sticks in direction-keys mode: (direction 0 up, 1 down, 2 left, 3 right;
    /// key). A direction keeps its key until let go, even if a layer changed the stick.
    stick_keys: HashMap<Stick, Vec<(usize, KeyCode)>>,
    /// Virtual-pad stick each physical stick feeds in gamepad mode, so it can be recentered
    /// when a layer gives the stick another mode.
    pad_feeds: HashMap<Stick, Stick>,
    /// Virtual-pad trigger each physical trigger feeds, likewise.
    trigger_feeds: HashMap<Trigger, Trigger>,
    /// Press threshold of each trigger whose button action is held: it releases below that,
    /// whatever the trigger is set to by then.
    trigger_release: HashMap<Trigger, f32>,
    /// Bounds of each active zone, which it releases by (the zone list may have changed).
    zone_bounds: HashMap<(Analog, usize), (f32, f32)>,
    /// Layers held or toggled on, oldest first, with how many inputs hold each.
    layers: Vec<(String, u32)>,
    /// Set when `layers` changes; the daemon takes it.
    layers_changed: bool,
    mouse_acc: (f32, f32),
    scroll_acc: (f32, f32),
    /// Smoothed position of each mouse stick whose response smooths it, until it settles at rest.
    stick_smooth: HashMap<Stick, (f32, f32)>,
    /// Seconds each mouse stick has been held at full deflection, for acceleration.
    stick_ramp: HashMap<Stick, f32>,
    /// Seconds each held action containing a wheel direction has been held.
    wheel_held: HashMap<Source, f32>,
    wheel_acc: (f32, f32),
    /// Toggles that are on, keyed by input and position in the action tree.
    toggled: HashMap<StateId, ButtonAction>,
    turbo: HashMap<StateId, TurboState>,
    /// Physically held buttons, for gyro activation.
    raw_buttons: HashSet<Button>,
    gyro: GyroState,
    /// What physical sticks currently send to each virtual-pad stick, so gyro can add to it.
    pad_sticks: HashMap<Stick, (f32, f32)>,
    /// Macro definitions by name, compiled to operations (see [`Engine::set_macros`]).
    macro_defs: HashMap<String, Arc<Vec<MacroOp>>>,
    macros_running: HashMap<StateId, MacroRun>,
    /// Virtual-pad stick positions set by macro steps, added to physical and gyro input.
    macro_sticks: HashMap<Stick, (f32, f32)>,
    /// Stick-direction virtual buttons currently pressed by stick movement.
    stick_buttons: HashSet<Button>,
    /// Stick-direction gamepad outputs held by actions (reference counted), pushing the
    /// virtual stick that way.
    pushed_directions: HashMap<Button, u32>,
    /// Set when a ToggleOverlay/ToggleNumpad action fires; the daemon takes it.
    overlay_toggled: Option<crate::keyboard::Layout>,
    /// Set when an OpenMenu action fires; the daemon takes it.
    menu_request: Option<(String, Opener)>,
    /// Set while a Toggle turns its inner action on.
    toggling: bool,
    /// Info overlays shown by held (or toggled) ShowInfo actions, with how many hold each.
    info_holds: HashMap<String, u32>,
    /// Log overlays shown by held (or toggled) ShowLog actions, likewise.
    log_holds: HashMap<String, u32>,
    /// Set when `info_holds` or `log_holds` changes; the daemon takes it.
    info_changed: bool,
    /// Actions pressed since the daemon last took them, with the input that pressed each,
    /// for the input log.
    fired: Vec<(crate::inputlog::FiredFrom, ButtonAction)>,
}

#[derive(Debug, Clone, PartialEq)]
enum MacroOp {
    Press(ButtonAction),
    Release(ButtonAction),
    /// Seconds.
    Wait(f32),
    Stick(Stick, (f32, f32)),
}

struct MacroRun {
    ops: Arc<Vec<MacroOp>>,
    next: usize,
    /// Seconds left in the current wait; may go negative to carry over into the next step.
    wait: f32,
    repeat: bool,
    /// Actions the macro has pressed and not yet released.
    held: Vec<ButtonAction>,
    /// Sticks the macro has moved, recentered when a pass ends.
    sticks: Vec<Stick>,
}

fn compile_macro(m: &Macro) -> Vec<MacroOp> {
    let mut ops = Vec::new();
    for step in &m.steps {
        match step {
            MacroStep::Tap { action, hold_ms } => ops.extend([
                MacroOp::Press(action.clone()),
                MacroOp::Wait(*hold_ms as f32 / 1000.0),
                MacroOp::Release(action.clone()),
            ]),
            MacroStep::Press(action) => ops.push(MacroOp::Press(action.clone())),
            MacroStep::Release(action) => ops.push(MacroOp::Release(action.clone())),
            MacroStep::Wait(ms) => ops.push(MacroOp::Wait(*ms as f32 / 1000.0)),
            MacroStep::Stick { stick, x, y } => {
                ops.push(MacroOp::Stick(*stick, (x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0))))
            }
        }
    }
    ops
}

#[derive(Default)]
struct GyroState {
    /// For `GyroActivation::Toggle`.
    toggled_on: bool,
    /// Virtual-pad stick the gyro is deflecting, and by how much.
    stick: Option<(Stick, (f32, f32))>,
    mouse_acc: (f32, f32),
    /// Low-passed tilt from the accelerometer (degrees), and the tilt counted as straight.
    roll: Option<f32>,
    roll_center: f32,
}

/// Identifies a Toggle/Turbo node: the input it belongs to and its pre-order position among
/// the stateful nodes of that input's action, so press and release find the same state.
type StateId = (Source, usize);

struct TurboState {
    action: ButtonAction,
    /// Seconds between press and release (half a turbo cycle).
    half_period: f32,
    elapsed: f32,
    down: bool,
}

impl Engine {
    /// Forgets buttons let go while something else (an on-screen menu) had the controller.
    pub fn forget_released(&mut self, down: &HashSet<Button>) {
        self.raw_buttons.retain(|b| down.contains(b));
    }

    /// A menu an OpenMenu action asked for since the last call, and what opened it.
    /// Actions pressed since last taken, and the input behind each.
    pub fn take_fired(&mut self) -> Vec<(crate::inputlog::FiredFrom, ButtonAction)> {
        std::mem::take(&mut self.fired)
    }

    pub fn take_menu_request(&mut self) -> Option<(String, Opener)> {
        self.menu_request.take()
    }

    /// Runs a chosen menu item's action as a quick press and release. Toggles and the like
    /// keep their state per item. Returns true if it asks for the next profile.
    pub fn tap_menu_item(&mut self, menu: &str, item: usize, action: &ButtonAction, out: &mut Vec<OutEvent>) -> bool {
        let src = Source::MenuItem(menu.to_string(), item);
        let switch = self.digital(&src, action, true, out);
        self.digital(&src, action, false, out);
        switch
    }

    /// Active layers, oldest first.
    pub fn layers(&self) -> Vec<String> {
        self.layers.iter().map(|(name, _)| name.clone()).collect()
    }

    /// Whether the active layers changed since the last call.
    pub fn take_layers_changed(&mut self) -> bool {
        std::mem::take(&mut self.layers_changed)
    }

    /// Whether the shown info overlays changed since the last call.
    pub fn take_info_changed(&mut self) -> bool {
        std::mem::take(&mut self.info_changed)
    }

    /// Info overlays held up by ShowInfo actions right now.
    pub fn shown_info(&self) -> impl Iterator<Item = &String> {
        self.info_holds.keys()
    }

    /// Log overlays held up by ShowLog actions right now.
    pub fn shown_logs(&self) -> impl Iterator<Item = &String> {
        self.log_holds.keys()
    }

    /// Which on-screen keyboard (or numpad) a toggle action asked for since the last call.
    pub fn take_overlay_toggle(&mut self) -> Option<crate::keyboard::Layout> {
        self.overlay_toggled.take()
    }

    /// Replaces the macro definitions `ButtonAction::Macro` refers to (from the config).
    pub fn set_macros(&mut self, macros: &[Macro]) {
        self.macro_defs = macros.iter().map(|m| (m.name.clone(), Arc::new(compile_macro(m)))).collect();
    }

    /// Runs a macro's operations until it has to wait or it ends. Each pass ends by releasing
    /// whatever it still holds; a repeating macro then starts over seamlessly, except that a
    /// macro with no waits at all runs one pass per tick instead of spinning in place.
    fn advance_macro(&mut self, id: &StateId, dt: f32, out: &mut Vec<OutEvent>) {
        let Some(mut run) = self.macros_running.remove(id) else { return };
        run.wait -= dt;
        let has_wait = run.ops.iter().any(|op| matches!(op, MacroOp::Wait(s) if *s > 0.0));
        while run.wait <= 0.0 {
            if run.next >= run.ops.len() {
                self.end_pass(id, &mut run, out);
                if !run.repeat {
                    return;
                }
                run.next = 0;
                if !has_wait {
                    run.wait = 0.0;
                    break;
                }
                continue;
            }
            match run.ops[run.next].clone() {
                MacroOp::Press(action) => {
                    self.emit(&id.0, &action, true, id.1 + 1, out);
                    run.held.push(action);
                }
                MacroOp::Release(action) => {
                    if let Some(i) = run.held.iter().position(|a| *a == action) {
                        run.held.remove(i);
                        self.emit(&id.0, &action, false, id.1 + 1, out);
                    }
                }
                MacroOp::Wait(seconds) => run.wait += seconds,
                MacroOp::Stick(stick, position) => {
                    self.macro_sticks.insert(stick, position);
                    if !run.sticks.contains(&stick) {
                        run.sticks.push(stick);
                    }
                    self.emit_pad_stick(stick, out);
                }
            }
            run.next += 1;
        }
        self.macros_running.insert(id.clone(), run);
    }

    /// Ends a pass: releases what the macro still holds (most recent first) and recenters the
    /// sticks it moved.
    fn end_pass(&mut self, id: &StateId, run: &mut MacroRun, out: &mut Vec<OutEvent>) {
        for action in std::mem::take(&mut run.held).into_iter().rev() {
            self.emit(&id.0, &action, false, id.1 + 1, out);
        }
        for stick in std::mem::take(&mut run.sticks) {
            self.macro_sticks.remove(&stick);
            self.emit_pad_stick(stick, out);
        }
    }

    fn stop_macro(&mut self, id: &StateId, out: &mut Vec<OutEvent>) {
        if let Some(mut run) = self.macros_running.remove(id) {
            self.end_pass(id, &mut run, out);
        }
    }

    fn tick_macros(&mut self, dt: f32, out: &mut Vec<OutEvent>) {
        let ids: Vec<StateId> = self.macros_running.keys().cloned().collect();
        for id in ids {
            self.advance_macro(&id, dt, out);
        }
    }

    /// Processes one input. Returns true if the input requested switching to the next profile.
    pub fn handle(
        &mut self,
        profile: &Profile,
        ev: InputEvent,
        now: Instant,
        out: &mut Vec<OutEvent>,
    ) -> bool {
        let gyro = &profile.gyro;
        let before = self.gyro_controls_held(gyro);
        if let InputEvent::Button(b, pressed) = ev {
            if pressed {
                self.raw_buttons.insert(b);
            } else {
                self.raw_buttons.remove(&b);
            }
        }
        let switch = self.handle_mapped(profile, ev, now, out);
        let after = self.gyro_controls_held(gyro);
        if matches!(gyro.activation, GyroActivation::Toggle(_)) && after.0 && !before.0 {
            self.gyro.toggled_on = !self.gyro.toggled_on;
        }
        if after.1 && !before.1 {
            self.recenter_gyro();
        }
        if !self.gyro_active(gyro) {
            self.set_gyro_stick(None, out);
        }
        // After the gyro edge checks above: the direction buttons run their own.
        if let InputEvent::Axis(axis, _) = ev
            && let Some(s) = axis_stick(axis)
        {
            return self.update_stick_buttons(profile, s, now, out) | switch;
        }
        switch
    }

    /// Presses or releases a stick's direction buttons (e.g. `Button::LeftStickUp`) as it
    /// moves past its press threshold, feeding them through the normal button path so they
    /// get actions, gestures and combos.
    fn update_stick_buttons(&mut self, profile: &Profile, s: Stick, now: Instant, out: &mut Vec<OutEvent>) -> bool {
        let cfg = profile.stick(s);
        let (x, y) = self.stick_pos(s, cfg.deadzone);
        let t = cfg.key_threshold;
        let [up, down, left, right] = Button::stick_directions(s);
        let mut switch = false;
        for (b, v) in [(up, -y), (down, y), (left, -x), (right, x)] {
            let pressed = self.stick_buttons.contains(&b);
            if !pressed && v >= t {
                self.stick_buttons.insert(b);
                switch |= self.handle(profile, InputEvent::Button(b, true), now, out);
            } else if pressed && v < t - STICK_DIRECTION_HYSTERESIS {
                self.stick_buttons.remove(&b);
                switch |= self.handle(profile, InputEvent::Button(b, false), now, out);
            }
        }
        switch
    }

    fn handle_mapped(
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
                            self.digital(&src, &ButtonAction::Disabled, false, out);
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
        self.digital(&Source::Combo(id), &combo.action, true, out)
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
                GestureState::Down { long_deadline, hold_deadline, .. } => {
                    long_deadline.is_some_and(|d| d <= now) || hold_deadline.is_some_and(|d| d <= now)
                }
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
                GestureState::Down { long_deadline: Some(_), .. } => {
                    self.gestures.insert(b, GestureState::Holding);
                    if let Some(action) = &gestures.long_press {
                        switch |= self.digital(&Source::Gesture(b), action, true, out);
                    }
                }
                // Held past the tap window with no long press set: a plain press, held (so
                // e.g. a menu on it stays up while held).
                GestureState::Down { .. } => {
                    self.gestures.insert(b, GestureState::Pressed);
                    switch |= self.digital(&Source::Button(b), profile.button(b), true, out);
                }
                // No further tap came: the sequence so far is final.
                GestureState::Up { taps, .. } => {
                    self.gestures.remove(&b);
                    switch |= self.tap_sequence(profile, b, taps, out);
                }
                GestureState::Holding | GestureState::Pressed => {}
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
            GestureState::Down { long_deadline, hold_deadline, .. } => long_deadline.or(*hold_deadline),
            GestureState::Up { deadline, .. } => Some(*deadline),
            GestureState::Holding | GestureState::Pressed => None,
        });
        combos.chain(gestures).min()
    }

    /// A button acting on its own (not as part of a combo). Runs gesture detection if the
    /// button has gestures, otherwise presses/releases its action directly.
    #[expect(clippy::too_many_arguments, reason = "predates the size lints")]
    fn solo(&mut self, profile: &Profile, b: Button, pressed: bool, now: Instant, out: &mut Vec<OutEvent>) -> bool {
        let Some(gestures) = profile.gestures(b) else {
            // Gestures may have been removed mid-sequence; a release still has to land.
            if !pressed {
                match self.gestures.remove(&b) {
                    Some(GestureState::Holding) => {
                        return self.digital(&Source::Gesture(b), &ButtonAction::Disabled, false, out);
                    }
                    Some(GestureState::Pressed) => {
                        return self.digital(&Source::Button(b), &ButtonAction::Disabled, false, out);
                    }
                    _ => {}
                }
            }
            return self.digital(&Source::Button(b), profile.button(b), pressed, out);
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
                return self.digital(&Source::Gesture(b), action, true, out);
            }
            let long_deadline = (taps == 1 && gestures.long_press.is_some())
                .then(|| now + Duration::from_millis(profile.long_press_ms));
            let hold_deadline = (taps == 1 && gestures.long_press.is_none())
                .then(|| now + Duration::from_millis(profile.tap_window_ms));
            self.gestures.insert(b, GestureState::Down { taps, long_deadline, hold_deadline });
            false
        } else {
            match self.gestures.remove(&b) {
                Some(GestureState::Holding) => {
                    self.digital(&Source::Gesture(b), &ButtonAction::Disabled, false, out)
                }
                Some(GestureState::Pressed) => self.digital(&Source::Button(b), &ButtonAction::Disabled, false, out),
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
            Some(action) => switch |= self.tap(&Source::Gesture(b), action, out),
            // e.g. a double tap when only a triple tap is set: that many normal taps.
            None => {
                for _ in 0..taps {
                    switch |= self.tap(&Source::Button(b), profile.button(b), out);
                }
            }
        }
        switch
    }

    fn tap(&mut self, src: &Source, action: &ButtonAction, out: &mut Vec<OutEvent>) -> bool {
        let switch = self.digital(src, action, true, out);
        self.digital(src, action, false, out);
        switch
    }

    fn digital(
        &mut self,
        src: &Source,
        action: &ButtonAction,
        pressed: bool,
        out: &mut Vec<OutEvent>,
    ) -> bool {
        if pressed {
            if self.held.contains_key(src) {
                return false;
            }
            self.held.insert(src.clone(), action.clone());
            if let Some(from) = src.fired_from() {
                self.fired.push((from, action.clone()));
            }
            self.emit(src, action, true, 0, out)
        } else {
            if let Some(action) = self.held.remove(src) {
                self.emit(src, &action, false, 0, out);
            }
            false
        }
    }

    /// Emits press/release for an action. `slot` is the position of this node among the
    /// Toggle/Turbo nodes of the input's action (see [`StateId`]). Returns true if it
    /// requests the next profile.
    #[expect(clippy::too_many_lines, clippy::too_many_arguments, clippy::cognitive_complexity, reason = "predates the size lints")]
    fn emit(&mut self, src: &Source, action: &ButtonAction, pressed: bool, slot: usize, out: &mut Vec<OutEvent>) -> bool {
        match action {
            ButtonAction::Disabled => {}
            ButtonAction::Gamepad(b) => match b.stick_direction() {
                // A stick direction as output pushes the virtual stick while held.
                Some((stick, _)) => {
                    let count = self.pushed_directions.entry(*b).or_insert(0);
                    if pressed {
                        *count += 1;
                    } else {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            self.pushed_directions.remove(b);
                        }
                    }
                    self.emit_pad_stick(stick, out);
                }
                None => out.push(OutEvent::PadButton(*b, pressed)),
            },
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
        ButtonAction::ToggleOverlay => {
            if pressed {
                self.overlay_toggled = Some(crate::keyboard::Layout::Keyboard);
            }
        }
        ButtonAction::ToggleNumpad => {
            if pressed {
                self.overlay_toggled = Some(crate::keyboard::Layout::Numpad);
            }
        }
        ButtonAction::ShowInfo(name) => {
            count_hold(&mut self.info_holds, name, pressed);
            self.info_changed = true;
        }
        ButtonAction::ShowLog(name) => {
            count_hold(&mut self.log_holds, name, pressed);
            self.info_changed = true;
        }
        ButtonAction::OpenMenu(name) => {
            if pressed {
                self.menu_request = Some((name.clone(), Opener { toggled: self.toggling, ..src.opener() }));
            }
        }
        ButtonAction::Layer(name) => {
            let at = self.layers.iter().position(|(n, _)| n == name);
            match (pressed, at) {
                (true, Some(i)) => self.layers[i].1 += 1,
                (true, None) => self.layers.push((name.clone(), 1)),
                (false, Some(i)) => {
                    self.layers[i].1 -= 1;
                    if self.layers[i].1 == 0 {
                        self.layers.remove(i);
                    }
                }
                (false, None) => {}
            }
            self.layers_changed = true;
        }
            ButtonAction::Multi(actions) => {
                // Child slots depend only on position, so reverse-order release matches.
                let mut slots = Vec::with_capacity(actions.len());
                let mut next = slot;
                for a in actions {
                    slots.push(next);
                    next += stateful_nodes(a);
                }
                let mut switch = false;
                if pressed {
                    for (a, s) in actions.iter().zip(&slots) {
                        switch |= self.emit(src, a, true, *s, out);
                    }
                } else {
                    for (a, s) in actions.iter().zip(&slots).rev() {
                        self.emit(src, a, false, *s, out);
                    }
                }
                return switch;
            }
            // Only presses matter: each one flips the inner action on or off.
            ButtonAction::Toggle(Toggled { action: inner, .. }) => {
                if !pressed {
                    return false;
                }
                let id = (src.clone(), slot);
                if let Some(inner) = self.toggled.remove(&id) {
                    self.emit(src, &inner, false, slot + 1, out);
                    return false;
                }
                return self.toggle_on(src, inner, slot, out);
            }
            ButtonAction::Turbo { action: inner, rate } => {
                let id = (src.clone(), slot);
                if pressed {
                    if self.turbo.contains_key(&id) {
                        return false;
                    }
                    let half_period = 0.5 / rate.clamp(0.5, 60.0);
                    self.turbo.insert(id, TurboState { action: (**inner).clone(), half_period, elapsed: 0.0, down: true });
                    return self.emit(src, inner, true, slot + 1, out);
                }
                if let Some(state) = self.turbo.remove(&id)
                    && state.down
                {
                    self.emit(src, &state.action, false, slot + 1, out);
                }
            }
            // Play once per press (release doesn't stop it), or loop while held.
            ButtonAction::Macro { name, repeat } => {
                let id = (src.clone(), slot);
                if !pressed {
                    if *repeat {
                        self.stop_macro(&id, out);
                    }
                    return false;
                }
                if self.macros_running.contains_key(&id) {
                    return false;
                }
                let Some(ops) = self.macro_defs.get(name).cloned() else {
                    crate::monitor::log!("no macro named {name:?}");
                    return false;
                };
                let run = MacroRun { ops, next: 0, wait: 0.0, repeat: *repeat, held: Vec::new(), sticks: Vec::new() };
                self.macros_running.insert(id.clone(), run);
                self.advance_macro(&id, 0.0, out);
            }
        }
        false
    }

    /// Turns a Toggle (at `slot` of `src`'s action) on, pressing its inner action.
    fn toggle_on(&mut self, src: &Source, inner: &ButtonAction, slot: usize, out: &mut Vec<OutEvent>) -> bool {
        self.toggled.insert((src.clone(), slot), inner.clone());
        // A menu opened from here stays up until toggled off.
        let was = std::mem::replace(&mut self.toggling, true);
        let switch = self.emit(src, inner, true, slot + 1, out);
        self.toggling = was;
        switch
    }

    /// Switches on every Toggle set to start on, in the profile's mappings and the game's menu
    /// items, as if pressed (one that's on already stays on). For when the game starts.
    pub fn start_toggles(&mut self, profile: &Profile, menus: &[Menu], out: &mut Vec<OutEvent>) -> bool {
        let mut inputs: Vec<(Source, &ButtonAction)> = Vec::new();
        inputs.extend(profile.buttons.iter().map(|(b, a)| (Source::Button(*b), a)));
        for (b, g) in &profile.gestures {
            inputs.extend(GestureKind::ALL.into_iter().filter_map(|k| g.get(k)).map(|a| (Source::Gesture(*b), a)));
        }
        for c in &profile.combos {
            let mut id = c.buttons.clone();
            id.sort();
            id.dedup();
            inputs.push((Source::Combo(id), &c.action));
        }
        for t in [Trigger::Left, Trigger::Right] {
            if let TriggerAction::Button { action, .. } = profile.trigger(t) {
                inputs.push((Source::Trigger(t), action));
            }
        }
        for a in [Analog::Stick(Stick::Left), Analog::Stick(Stick::Right), Analog::Trigger(Trigger::Left), Analog::Trigger(Trigger::Right)] {
            inputs.extend(profile.zones(a).iter().enumerate().map(|(i, z)| (Source::Zone(a, i), &z.action)));
        }
        for m in menus {
            inputs.extend(m.items.iter().enumerate().map(|(i, item)| (Source::MenuItem(m.name.clone(), i), &item.action)));
        }
        let mut switch = false;
        for (src, action) in inputs {
            let mut starting = Vec::new();
            start_on_toggles(action, 0, &mut starting);
            for (slot, inner) in starting {
                if !self.toggled.contains_key(&(src.clone(), slot)) {
                    switch |= self.toggle_on(&src, inner, slot, out);
                }
            }
        }
        switch
    }

    /// Advances turbo actions: each flips between pressed and released every half period.
    fn tick_turbo(&mut self, dt: f32, out: &mut Vec<OutEvent>) {
        let ids: Vec<StateId> = self.turbo.keys().cloned().collect();
        for id in ids {
            let Some(state) = self.turbo.get_mut(&id) else { continue };
            state.elapsed += dt;
            let mut flips = Vec::new();
            while state.elapsed >= state.half_period {
                state.elapsed -= state.half_period;
                state.down = !state.down;
                flips.push(state.down);
            }
            let action = state.action.clone();
            for down in flips {
                self.emit(&id.0, &action, down, id.1 + 1, out);
            }
        }
    }

    fn trigger(&mut self, profile: &Profile, t: Trigger, value: f32, out: &mut Vec<OutEvent>) -> bool {
        let action = profile.trigger(t);
        // A virtual trigger no longer fed (a layer changed this one's mode) goes back to rest.
        let target = match action {
            TriggerAction::Gamepad(target) => Some(*target),
            _ => None,
        };
        if let Some(old) = self.trigger_feeds.get(&t).copied()
            && Some(old) != target
        {
            self.trigger_feeds.remove(&t);
            out.push(OutEvent::PadAxis(trigger_axis(old), 0.0));
        }
        let src = Source::Trigger(t);
        let mut switch = false;
        // A pull keeps its action until let go past where it pressed.
        if let Some(threshold) = self.trigger_release.get(&t).copied()
            && value < threshold - TRIGGER_HYSTERESIS
        {
            self.trigger_release.remove(&t);
            switch |= self.digital(&src, &ButtonAction::Disabled, false, out);
        }
        match action {
            TriggerAction::Disabled => {}
            TriggerAction::Gamepad(target) => {
                self.trigger_feeds.insert(t, *target);
                out.push(OutEvent::PadAxis(trigger_axis(*target), value));
            }
            TriggerAction::Button { action, threshold } => {
                if !self.trigger_release.contains_key(&t) && value >= *threshold {
                    self.trigger_release.insert(t, *threshold);
                    switch |= self.digital(&src, action, true, out);
                }
            }
        }
        self.zones(profile, Analog::Trigger(t), value, out) | switch
    }

    /// Presses/releases zone actions for an analog input at `value` (0.0..1.0). Zones are
    /// inactive at rest (value 0), so a zone starting at 0 means "as soon as it moves".
    fn zones(&mut self, profile: &Profile, analog: Analog, value: f32, out: &mut Vec<OutEvent>) -> bool {
        let inside = |(min, max): (f32, f32), margin: f32| {
            // The top zone includes full deflection itself.
            let below_max = max >= 1.0 || value < max + margin;
            value > 0.0 && value >= min - margin && below_max
        };
        // Release first, by the bounds each active zone pressed with (a layer may have
        // changed the zones since), so moving between adjacent zones never holds both.
        let mut switch = false;
        let leaving: Vec<usize> = self
            .zone_bounds
            .iter()
            .filter(|((a, _), bounds)| *a == analog && !inside(**bounds, ZONE_HYSTERESIS))
            .map(|((_, i), _)| *i)
            .collect();
        for i in leaving {
            self.zone_bounds.remove(&(analog, i));
            switch |= self.digital(&Source::Zone(analog, i), &ButtonAction::Disabled, false, out);
        }
        for (i, zone) in profile.zones(analog).iter().enumerate() {
            let bounds = (zone.min, zone.max);
            if !self.zone_bounds.contains_key(&(analog, i)) && inside(bounds, 0.0) {
                self.zone_bounds.insert((analog, i), bounds);
                switch |= self.digital(&Source::Zone(analog, i), &zone.action, true, out);
            }
        }
        switch
    }

    /// Changes the gyro's stick deflection, re-sending affected sticks.
    fn set_gyro_stick(&mut self, value: Option<(Stick, (f32, f32))>, out: &mut Vec<OutEvent>) {
        let old = std::mem::replace(&mut self.gyro.stick, value);
        if old == value {
            return;
        }
        if let Some((s, _)) = old
            && value.is_none_or(|(n, _)| n != s)
        {
            self.emit_pad_stick(s, out);
        }
        if let Some((s, _)) = value {
            self.emit_pad_stick(s, out);
        }
    }

    fn gyro_input_held(&self, input: GyroInput) -> bool {
        let trigger = |axis| self.axes.get(&axis).copied().unwrap_or(0.0) > 0.5;
        match input {
            GyroInput::Button(b) => self.raw_buttons.contains(&b),
            GyroInput::LeftTrigger => trigger(Axis::LeftTrigger),
            GyroInput::RightTrigger => trigger(Axis::RightTrigger),
        }
    }

    /// (toggle input held, recenter input held), for edge detection around each event.
    fn gyro_controls_held(&self, cfg: &GyroConfig) -> (bool, bool) {
        let toggle = match cfg.activation {
            GyroActivation::Toggle(i) => self.gyro_input_held(i),
            _ => false,
        };
        (toggle, cfg.recenter.is_some_and(|i| self.gyro_input_held(i)))
    }

    fn gyro_active(&self, cfg: &GyroConfig) -> bool {
        match cfg.activation {
            GyroActivation::Always => true,
            GyroActivation::WhileHeld(i) => self.gyro_input_held(i),
            GyroActivation::UnlessHeld(i) => !self.gyro_input_held(i),
            GyroActivation::Toggle(_) => self.gyro.toggled_on,
        }
    }

    fn recenter_gyro(&mut self) {
        self.gyro.roll_center = self.gyro.roll.unwrap_or(0.0);
        self.gyro.mouse_acc = (0.0, 0.0);
    }

    /// Applies one motion-sensor sample (with calibration bias already removed).
    pub fn motion(&mut self, profile: &Profile, sample: MotionSample, out: &mut Vec<OutEvent>) {
        let cfg = &profile.gyro;
        // Track tilt from gravity all the time, so steering is centered when switched on.
        let [ax, ay, _] = sample.accel;
        if ax != 0.0 || ay != 0.0 {
            let tilt = ax.atan2(ay).to_degrees();
            let blend = 1.0 - (-sample.dt / TILT_SMOOTHING).exp();
            self.gyro.roll = Some(match self.gyro.roll {
                Some(r) => r + angle_diff(tilt, r) * blend,
                None => tilt,
            });
        }
        if matches!(cfg.mode, GyroMode::Off) || !self.gyro_active(cfg) {
            self.set_gyro_stick(None, out);
            return;
        }

        let [pitch, yaw, roll] = sample.gyro;
        let h = match cfg.horizontal {
            GyroHorizontal::Yaw => yaw,
            GyroHorizontal::Roll => roll,
            GyroHorizontal::YawAndRoll => yaw + roll,
        };
        // Below the threshold, scale down smoothly so jitter and drift don't move the aim.
        let speed = h.hypot(pitch);
        let tighten = if cfg.noise_threshold > 0.0 { (speed / cfg.noise_threshold).min(1.0) } else { 1.0 };
        // Turning left or tilting the front up should aim left/up (negative x/y).
        let mut x = -h * tighten;
        let mut y = -pitch * tighten;
        if cfg.invert_x {
            x = -x;
        }
        if cfg.invert_y {
            y = -y;
        }

        match cfg.mode {
            GyroMode::Off => {}
            GyroMode::Mouse { sensitivity } => {
                // A layer may have switched from stick mode: stop deflecting the stick.
                self.set_gyro_stick(None, out);
                let (dx, dy) = take_whole(&mut self.gyro.mouse_acc, x * sensitivity * sample.dt, y * sensitivity * sample.dt);
                if dx != 0 || dy != 0 {
                    out.push(OutEvent::MouseMove(dx, dy));
                }
            }
            GyroMode::Stick { stick, full_rate, anti_deadzone } => {
                let (mut sx, mut sy) = (x / full_rate.max(1.0), y / full_rate.max(1.0));
                let mag = sx.hypot(sy);
                if mag > 0.0 {
                    let scaled = (anti_deadzone + (1.0 - anti_deadzone) * mag.min(1.0)) / mag;
                    sx *= scaled;
                    sy *= scaled;
                }
                self.set_gyro_stick(Some((stick, (sx, sy))), out);
            }
            GyroMode::Steering { stick, max_angle } => {
                let angle = angle_diff(self.gyro.roll.unwrap_or(0.0), self.gyro.roll_center);
                let mut sx = (angle / max_angle.max(1.0)).clamp(-1.0, 1.0);
                if cfg.invert_x {
                    sx = -sx;
                }
                self.set_gyro_stick(Some((stick, (sx, 0.0))), out);
            }
        }
    }

    /// Whether `tick` currently has anything to do: a mouse/scroll stick is deflected or a
    /// wheel action is held.
    pub fn needs_tick(&self, profile: &Profile) -> bool {
        self.sticks_need_tick(profile)
            || !self.turbo.is_empty()
            || !self.macros_running.is_empty()
            || self.held.values().any(|a| !a.wheel_directions().is_empty())
    }

    /// Advances continuous outputs (mouse motion, scrolling) by `dt` seconds.
    pub fn tick(&mut self, profile: &Profile, dt: f32, out: &mut Vec<OutEvent>) {
        self.tick_sticks(profile, dt, out);
        self.tick_wheel(dt, out);
        self.tick_turbo(dt, out);
        self.tick_macros(dt, out);
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
    /// profile switch, when an on-screen overlay takes the controller, or when the device goes
    /// away. With `keep_toggled_layers`, layers a Toggle switched on stay on (held ones end).
    pub fn release_all(&mut self, keep_toggled_layers: bool, out: &mut Vec<OutEvent>) {
        let held: Vec<_> = self.held.drain().collect();
        for (src, action) in held {
            self.emit(&src, &action, false, 0, out);
        }
        // Toggled-on actions are released too (this also stops toggled turbos).
        let toggled: Vec<_> = self.toggled.drain().collect();
        for ((src, slot), inner) in toggled {
            if keep_toggled_layers && matches!(inner, ButtonAction::Layer(_)) {
                self.toggled.insert((src, slot), inner);
                continue;
            }
            self.emit(&src, &inner, false, slot + 1, out);
        }
        for id in self.macros_running.keys().cloned().collect::<Vec<_>>() {
            self.stop_macro(&id, out);
        }
        // Anything still repeating (normally nothing by now) is stopped as well.
        for ((src, slot), state) in self.turbo.drain().collect::<Vec<_>>() {
            if state.down {
                self.emit(&src, &state.action, false, slot + 1, out);
            }
        }
        // Held combo members stay silent until released rather than firing under a new profile.
        for m in self.members.values_mut() {
            *m = ComboMember::Consumed;
        }
        // Unfinished tap sequences are dropped.
        self.gestures.clear();
        for (_, keys) in self.stick_keys.drain() {
            out.extend(keys.into_iter().map(|(_, k)| OutEvent::Key(k, false)));
        }
        self.pad_feeds.clear();
        self.trigger_feeds.clear();
        self.trigger_release.clear();
        self.zone_bounds.clear();
        for axis in [Axis::LeftX, Axis::LeftY, Axis::RightX, Axis::RightY, Axis::LeftTrigger, Axis::RightTrigger] {
            out.push(OutEvent::PadAxis(axis, 0.0));
        }
        self.mouse_acc = (0.0, 0.0);
        self.scroll_acc = (0.0, 0.0);
        self.stick_smooth.clear();
        self.stick_ramp.clear();
        self.wheel_held.clear();
        self.wheel_acc = (0.0, 0.0);
        // Pad axes were centered above; forget what fed them. Tilt tracking carries on.
        self.pad_sticks.clear();
        self.gyro.stick = None;
        self.gyro.mouse_acc = (0.0, 0.0);
        self.gyro.toggled_on = false;
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

/// Difference between two angles in degrees, wrapped to -180..180.
/// Counts one more (or one fewer) hold on a named overlay, forgetting it at none.
fn count_hold(holds: &mut HashMap<String, u32>, name: &str, pressed: bool) {
    if pressed {
        *holds.entry(name.to_string()).or_insert(0) += 1;
    } else if let Some(n) = holds.get_mut(name) {
        *n -= 1;
        if *n == 0 {
            holds.remove(name);
        }
    }
}

fn angle_diff(a: f32, b: f32) -> f32 {
    (a - b + 540.0).rem_euclid(360.0) - 180.0
}

/// The Toggles set to start on in `action` (at `slot`), with their slots, numbered as
/// [`Engine::emit`] numbers them.
fn start_on_toggles<'a>(action: &'a ButtonAction, slot: usize, found: &mut Vec<(usize, &'a ButtonAction)>) {
    match action {
        ButtonAction::Multi(list) => {
            let mut next = slot;
            for a in list {
                start_on_toggles(a, next, found);
                next += stateful_nodes(a);
            }
        }
        ButtonAction::Toggle(t) => {
            if t.start_on {
                found.push((slot, &t.action));
            }
            start_on_toggles(&t.action, slot + 1, found);
        }
        ButtonAction::Turbo { action, .. } => start_on_toggles(action, slot + 1, found),
        _ => {}
    }
}

/// Number of Toggle/Turbo nodes in an action, each of which owns a [`StateId`] slot.
fn stateful_nodes(action: &ButtonAction) -> usize {
    match action {
        ButtonAction::Toggle(Toggled { action: inner, .. }) | ButtonAction::Turbo { action: inner, .. } => 1 + stateful_nodes(inner),
        ButtonAction::Macro { .. } => 1,
        ButtonAction::Multi(actions) => actions.iter().map(stateful_nodes).sum(),
        _ => 0,
    }
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

/// The stick an axis belongs to, if it is a stick axis.
fn axis_stick(axis: Axis) -> Option<Stick> {
    match axis {
        Axis::LeftX | Axis::LeftY => Some(Stick::Left),
        Axis::RightX | Axis::RightY => Some(Stick::Right),
        Axis::LeftTrigger | Axis::RightTrigger => None,
    }
}

fn trigger_axis(t: Trigger) -> Axis {
    match t {
        Trigger::Left => Axis::LeftTrigger,
        Trigger::Right => Axis::RightTrigger,
    }
}

/// Radial deadzone, rescaled so output still spans the full 0..1 range.
pub(crate) fn apply_deadzone(x: f32, y: f32, deadzone: f32) -> (f32, f32) {
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
    use crate::config::{Combo, MouseButton, StickConfig, Zone};

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
    fn fired_actions_are_reported_once_per_press() {
        use crate::inputlog::FiredFrom;
        let mut p = Profile::passthrough("p");
        p.set_button(Button::South, ButtonAction::Mouse(MouseButton::Left));
        let mut e = Engine::default();
        run(&mut e, &p, InputEvent::Button(Button::South, true));
        assert_eq!(e.take_fired(), [(FiredFrom::Button(Button::South), ButtonAction::Mouse(MouseButton::Left))]);
        run(&mut e, &p, InputEvent::Button(Button::South, false));
        assert!(e.take_fired().is_empty());
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
    fn holding_past_the_tap_window_presses_and_holds_the_buttons_own_action() {
        // e.g. RB: hold for a weapon wheel, double tap to holster.
        let mut p = Profile::passthrough("p");
        p.set_button(Button::RightBumper, ButtonAction::OpenMenu("Weapons".into()));
        p.gestures.insert(
            Button::RightBumper,
            crate::config::Gestures { double_tap: Some(ButtonAction::Keys(vec!["KEY_H".into()])), ..Default::default() },
        );
        let mut e = Engine::default();
        let t0 = Instant::now();
        press(&mut e, &p, Button::RightBumper, true, t0);
        assert_eq!(e.take_menu_request(), None, "undecided inside the tap window");
        fire_timers(&mut e, &p, ms(t0, p.tap_window_ms + 1));
        let (name, opener) = e.take_menu_request().expect("held past the window: the menu opens");
        assert_eq!((name.as_str(), opener.buttons), ("Weapons", vec![Button::RightBumper]));
        // The double tap still works.
        press(&mut e, &p, Button::RightBumper, false, ms(t0, 400));
        press(&mut e, &p, Button::RightBumper, true, ms(t0, 1000));
        press(&mut e, &p, Button::RightBumper, false, ms(t0, 1050));
        let out = press(&mut e, &p, Button::RightBumper, true, ms(t0, 1100));
        assert_eq!(out, vec![OutEvent::Key(KeyCode::KEY_H, true)]);
        assert_eq!(e.take_menu_request(), None);
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

    fn toggle(inner: ButtonAction) -> ButtonAction {
        ButtonAction::toggle(inner)
    }

    fn turbo(inner: ButtonAction, rate: f32) -> ButtonAction {
        ButtonAction::Turbo { action: Box::new(inner), rate }
    }

    fn c_key() -> ButtonAction {
        ButtonAction::Keys(vec!["KEY_C".into()])
    }

    fn clicks(out: &[OutEvent]) -> Vec<bool> {
        out.iter()
            .filter_map(|ev| match ev {
                OutEvent::MouseButton(MouseButton::Left, down) => Some(*down),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn toggle_holds_until_pressed_again() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::RightStick, toggle(c_key()));
        let mut e = Engine::default();
        let press = |e: &mut Engine, down| run(e, &p, InputEvent::Button(Button::RightStick, down));
        assert_eq!(press(&mut e, true), vec![OutEvent::Key(KeyCode::KEY_C, true)]);
        assert!(press(&mut e, false).is_empty(), "letting go keeps crouching");
        assert_eq!(press(&mut e, true), vec![OutEvent::Key(KeyCode::KEY_C, false)]);
        assert!(press(&mut e, false).is_empty());
        assert_eq!(press(&mut e, true), vec![OutEvent::Key(KeyCode::KEY_C, true)]);
    }

    #[test]
    fn turbo_repeats_at_rate_while_held() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::West, turbo(ButtonAction::Mouse(MouseButton::Left), 10.0));
        let mut e = Engine::default();
        assert_eq!(clicks(&run(&mut e, &p, InputEvent::Button(Button::West, true))), [true]);
        assert!(e.needs_tick(&p));
        // 10/s for one second: alternating up/down every 50 ms, 20 flips.
        let flips = clicks(&hold_for(&mut e, &p, 1.0));
        assert!((19..=21).contains(&flips.len()), "{flips:?}");
        assert!(flips.windows(2).all(|w| w[0] != w[1]), "must alternate: {flips:?}");
        // Releasing mid-press releases the button and stops repeating.
        let mut all = flips;
        all.extend(clicks(&run(&mut e, &p, InputEvent::Button(Button::West, false))));
        assert_eq!(all.last(), Some(&false));
        assert!(!e.needs_tick(&p));
        assert!(clicks(&hold_for(&mut e, &p, 0.5)).is_empty());
    }

    #[test]
    fn toggled_turbo_keeps_firing_after_release_until_toggled_off() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::West, toggle(turbo(ButtonAction::Mouse(MouseButton::Left), 10.0)));
        let mut e = Engine::default();
        run(&mut e, &p, InputEvent::Button(Button::West, true));
        run(&mut e, &p, InputEvent::Button(Button::West, false));
        assert!(clicks(&hold_for(&mut e, &p, 0.5)).len() >= 9, "auto-fire continues hands-off");
        let mut out = run(&mut e, &p, InputEvent::Button(Button::West, true));
        out.extend(hold_for(&mut e, &p, 0.5));
        assert!(clicks(&out).iter().all(|down| !down), "only a final release: {out:?}");
        assert!(!e.needs_tick(&p));
    }

    #[test]
    fn toggle_inside_multi_keeps_its_state_across_reverse_release() {
        let mut p = Profile::passthrough("p");
        p.set_button(
            Button::South,
            ButtonAction::Multi(vec![ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into()]), toggle(c_key())]),
        );
        let mut e = Engine::default();
        let shift = KeyCode::KEY_LEFTSHIFT;
        assert_eq!(
            run(&mut e, &p, InputEvent::Button(Button::South, true)),
            vec![OutEvent::Key(shift, true), OutEvent::Key(KeyCode::KEY_C, true)]
        );
        assert_eq!(run(&mut e, &p, InputEvent::Button(Button::South, false)), vec![OutEvent::Key(shift, false)]);
        assert_eq!(
            run(&mut e, &p, InputEvent::Button(Button::South, true)),
            vec![OutEvent::Key(shift, true), OutEvent::Key(KeyCode::KEY_C, false)]
        );
    }

    #[test]
    fn profile_switch_releases_toggles_and_stops_turbo() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::North, toggle(c_key()));
        p.set_button(Button::West, turbo(ButtonAction::Mouse(MouseButton::Left), 10.0));
        let mut e = Engine::default();
        run(&mut e, &p, InputEvent::Button(Button::North, true));
        run(&mut e, &p, InputEvent::Button(Button::North, false));
        run(&mut e, &p, InputEvent::Button(Button::West, true));
        let mut out = Vec::new();
        e.release_all(false, &mut out);
        assert!(out.contains(&OutEvent::Key(KeyCode::KEY_C, false)), "{out:?}");
        assert!(out.contains(&OutEvent::MouseButton(MouseButton::Left, false)), "{out:?}");
        assert!(!e.needs_tick(&p));
    }

    #[test]
    fn double_tap_can_toggle() {
        let mut p = Profile::passthrough("p");
        p.gestures.insert(
            Button::East,
            crate::config::Gestures { double_tap: Some(toggle(c_key())), ..Default::default() },
        );
        let mut e = Engine::default();
        let t0 = Instant::now();
        let mut out = Vec::new();
        for (down, at) in [(true, 0), (false, 50), (true, 120)] {
            out.extend(press(&mut e, &p, Button::East, down, ms(t0, at)));
        }
        assert_eq!(out, vec![OutEvent::Key(KeyCode::KEY_C, true)]);
        // Releasing the double tap leaves the toggle on.
        assert!(press(&mut e, &p, Button::East, false, ms(t0, 200)).is_empty());
    }

    use crate::config::{GyroActivation, GyroConfig, GyroHorizontal, GyroInput, GyroMode};

    fn gyro_profile(gyro: GyroConfig) -> Profile {
        Profile { gyro, ..Profile::passthrough("p") }
    }

    /// `seconds` of turning at constant rates (degrees/second), held flat, at 250 Hz.
    #[expect(clippy::too_many_arguments, reason = "predates the size lints")]
    fn turn(e: &mut Engine, p: &Profile, pitch: f32, yaw: f32, roll: f32, seconds: f32) -> Vec<OutEvent> {
        let mut out = Vec::new();
        for _ in 0..(seconds * 250.0).round() as usize {
            let sample = MotionSample { gyro: [pitch, yaw, roll], accel: [0.0, 1.0, 0.0], dt: 0.004 };
            e.motion(p, sample, &mut out);
        }
        out
    }

    fn mouse_total(out: &[OutEvent]) -> (i32, i32) {
        out.iter().fold((0, 0), |(x, y), ev| match ev {
            OutEvent::MouseMove(dx, dy) => (x + dx, y + dy),
            _ => (x, y),
        })
    }

    fn mouse_gyro(activation: GyroActivation) -> GyroConfig {
        GyroConfig { mode: GyroMode::Mouse { sensitivity: 10.0 }, activation, ..GyroConfig::default() }
    }

    #[test]
    fn gyro_mouse_turns_degrees_into_pixels() {
        let p = gyro_profile(mouse_gyro(GyroActivation::Always));
        let mut e = Engine::default();
        // Turning left (positive yaw) 30 degrees, then tilting the front up 10 degrees.
        let (dx, dy) = mouse_total(&turn(&mut e, &p, 0.0, 60.0, 0.0, 0.5));
        assert!((-301..=-299).contains(&dx) && dy == 0, "{dx} {dy}");
        let (dx, dy) = mouse_total(&turn(&mut e, &p, 20.0, 0.0, 0.0, 0.5));
        assert!(dx == 0 && (-101..=-99).contains(&dy), "{dx} {dy}");
    }

    #[test]
    fn gyro_ignores_jitter_below_threshold() {
        let p = gyro_profile(GyroConfig { noise_threshold: 2.0, ..mouse_gyro(GyroActivation::Always) });
        let mut e = Engine::default();
        // 0.2 deg/s of drift for 10 s would be 20 px untreated; tightening shrinks it 10x.
        let (dx, _) = mouse_total(&turn(&mut e, &p, 0.0, 0.2, 0.0, 10.0));
        assert!(dx.abs() <= 2, "{dx}");
    }

    #[test]
    fn gyro_only_while_left_trigger_held() {
        let p = gyro_profile(mouse_gyro(GyroActivation::WhileHeld(GyroInput::LeftTrigger)));
        let mut e = Engine::default();
        assert_eq!(mouse_total(&turn(&mut e, &p, 0.0, 60.0, 0.0, 0.5)), (0, 0));
        axis(&mut e, &p, Axis::LeftTrigger, 0.8);
        assert_ne!(mouse_total(&turn(&mut e, &p, 0.0, 60.0, 0.0, 0.5)), (0, 0));
        axis(&mut e, &p, Axis::LeftTrigger, 0.1);
        assert_eq!(mouse_total(&turn(&mut e, &p, 0.0, 60.0, 0.0, 0.5)), (0, 0));
    }

    #[test]
    fn gyro_clutch_and_toggle() {
        let clutch = gyro_profile(mouse_gyro(GyroActivation::UnlessHeld(GyroInput::Button(Button::RightStick))));
        let mut e = Engine::default();
        run(&mut e, &clutch, InputEvent::Button(Button::RightStick, true));
        assert_eq!(mouse_total(&turn(&mut e, &clutch, 0.0, 60.0, 0.0, 0.5)), (0, 0));
        run(&mut e, &clutch, InputEvent::Button(Button::RightStick, false));
        assert_ne!(mouse_total(&turn(&mut e, &clutch, 0.0, 60.0, 0.0, 0.5)), (0, 0));

        let toggle = gyro_profile(mouse_gyro(GyroActivation::Toggle(GyroInput::Button(Button::Select))));
        let mut e = Engine::default();
        let tap = |e: &mut Engine| {
            run(e, &toggle, InputEvent::Button(Button::Select, true));
            run(e, &toggle, InputEvent::Button(Button::Select, false));
        };
        assert_eq!(mouse_total(&turn(&mut e, &toggle, 0.0, 60.0, 0.0, 0.2)), (0, 0));
        tap(&mut e);
        assert_ne!(mouse_total(&turn(&mut e, &toggle, 0.0, 60.0, 0.0, 0.2)), (0, 0));
        tap(&mut e);
        assert_eq!(mouse_total(&turn(&mut e, &toggle, 0.0, 60.0, 0.0, 0.2)), (0, 0));
    }

    #[test]
    fn gyro_stick_adds_to_physical_stick_and_clears_when_off() {
        let mut p = gyro_profile(GyroConfig {
            mode: GyroMode::Stick { stick: Stick::Right, full_rate: 100.0, anti_deadzone: 0.0 },
            activation: GyroActivation::WhileHeld(GyroInput::LeftTrigger),
            ..GyroConfig::default()
        });
        p.right_stick.deadzone = 0.0; // so the physical 0.3 passes through unchanged
        let mut e = Engine::default();
        axis(&mut e, &p, Axis::RightX, 0.3);
        axis(&mut e, &p, Axis::LeftTrigger, 1.0);
        // Turning right at 50 deg/s = half deflection, plus the physical 0.3.
        let out = turn(&mut e, &p, 0.0, -50.0, 0.0, 0.02);
        assert!(out.iter().any(|ev| matches!(ev, OutEvent::PadAxis(Axis::RightX, v) if (v - 0.8).abs() < 1e-3)), "{out:?}");
        // Letting go of LT drops the gyro part immediately.
        let out = axis(&mut e, &p, Axis::LeftTrigger, 0.0);
        assert!(out.contains(&OutEvent::PadAxis(Axis::RightX, 0.3)), "{out:?}");
    }

    #[test]
    fn gyro_steering_follows_tilt_and_recenters() {
        let p = gyro_profile(GyroConfig {
            mode: GyroMode::Steering { stick: Stick::Left, max_angle: 45.0 },
            recenter: Some(GyroInput::Button(Button::Select)),
            ..GyroConfig::default()
        });
        let mut e = Engine::default();
        let tilt = |e: &mut Engine, degrees: f32| {
            let r = degrees.to_radians();
            let mut out = Vec::new();
            for _ in 0..100 {
                let sample = MotionSample { gyro: [0.0; 3], accel: [r.sin(), r.cos(), 0.0], dt: 0.004 };
                e.motion(&p, sample, &mut out);
            }
            out.iter().rev().find_map(|ev| match ev {
                OutEvent::PadAxis(Axis::LeftX, v) => Some(*v),
                _ => None,
            })
        };
        let x = tilt(&mut e, 22.5).unwrap();
        assert!((x - 0.5).abs() < 0.02, "half of 45 degrees: {x}");
        // Recenter at the current tilt: it now counts as straight.
        run(&mut e, &p, InputEvent::Button(Button::Select, true));
        let x = tilt(&mut e, 22.5).unwrap_or(0.5);
        assert!(x.abs() < 0.02, "{x}");
        let x = tilt(&mut e, 67.5).unwrap();
        assert!((x - 1.0).abs() < 0.02, "{x}");
    }

    #[test]
    fn gyro_roll_and_inversion_options() {
        let p = gyro_profile(GyroConfig {
            horizontal: GyroHorizontal::Roll,
            invert_x: true,
            ..mouse_gyro(GyroActivation::Always)
        });
        let mut e = Engine::default();
        // Yaw is ignored in roll mode; inverted roll moves right.
        assert_eq!(mouse_total(&turn(&mut e, &p, 0.0, 60.0, 0.0, 0.5)), (0, 0));
        let (dx, _) = mouse_total(&turn(&mut e, &p, 0.0, 0.0, 60.0, 0.5));
        assert!(dx > 290, "{dx}");
    }

    #[test]
    fn angle_difference_wraps() {
        assert!((angle_diff(170.0, -170.0) + 20.0).abs() < 1e-4);
        assert!((angle_diff(-170.0, 170.0) - 20.0).abs() < 1e-4);
    }

    use crate::config::{Macro, MacroStep};

    fn key(k: &str) -> ButtonAction {
        ButtonAction::Keys(vec![k.into()])
    }

    fn macro_engine(steps: Vec<MacroStep>) -> Engine {
        let mut e = Engine::default();
        e.set_macros(&[Macro { name: "m".into(), steps }]);
        e
    }

    fn mapped(repeat: bool) -> Profile {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::West, ButtonAction::Macro { name: "m".into(), repeat });
        p
    }

    /// Key events with the time (ms since the press) they were sent.
    fn timeline(e: &mut Engine, p: &Profile, seconds: f32, release_at: Option<f32>) -> Vec<(u32, OutEvent)> {
        let mut events: Vec<(u32, OutEvent)> =
            run(e, p, InputEvent::Button(Button::West, true)).into_iter().map(|ev| (0, ev)).collect();
        let steps = (seconds / 0.004).round() as u32;
        for i in 1..=steps {
            let mut out = Vec::new();
            if release_at.is_some_and(|r| (i as f32 * 0.004 - r).abs() < 0.002) {
                out.extend(run(e, p, InputEvent::Button(Button::West, false)));
            }
            e.tick(p, 0.004, &mut out);
            events.extend(out.into_iter().map(|ev| (i * 4, ev)));
        }
        events
    }

    #[test]
    fn macro_plays_its_sequence_with_timing() {
        let mut e = macro_engine(vec![
            MacroStep::Tap { action: key("KEY_A"), hold_ms: 40 },
            MacroStep::Wait(100),
            MacroStep::Press(key("KEY_LEFTSHIFT")),
            MacroStep::Tap { action: ButtonAction::Mouse(MouseButton::Left), hold_ms: 20 },
            MacroStep::Release(key("KEY_LEFTSHIFT")),
        ]);
        let p = mapped(false);
        let events = timeline(&mut e, &p, 0.5, Some(0.01));
        let shift = KeyCode::KEY_LEFTSHIFT;
        assert_eq!(
            events,
            vec![
                (0, OutEvent::Key(KeyCode::KEY_A, true)),
                (40, OutEvent::Key(KeyCode::KEY_A, false)),
                (140, OutEvent::Key(shift, true)),
                (140, OutEvent::MouseButton(MouseButton::Left, true)),
                (160, OutEvent::MouseButton(MouseButton::Left, false)),
                (160, OutEvent::Key(shift, false)),
            ],
            "releasing the button early must not cut a play-once macro short"
        );
        assert!(!e.needs_tick(&p), "finished macros stop ticking");
    }

    #[test]
    fn repeating_macro_loops_while_held_and_cleans_up_on_release() {
        let mut e = macro_engine(vec![
            MacroStep::Press(key("KEY_LEFTSHIFT")),
            MacroStep::Tap { action: key("KEY_A"), hold_ms: 50 },
            MacroStep::Wait(50),
        ]);
        let p = mapped(true);
        let events = timeline(&mut e, &p, 1.0, Some(0.32));
        let a_presses: Vec<u32> = events
            .iter()
            .filter(|(_, ev)| *ev == OutEvent::Key(KeyCode::KEY_A, true))
            .map(|(t, _)| *t)
            .collect();
        assert_eq!(a_presses, [0, 100, 200, 300], "loops every 100 ms until released at 320 ms");
        // Shift is never released by a step, so each pass lets go of it at its end.
        let shift_downs = events.iter().filter(|(_, ev)| *ev == OutEvent::Key(KeyCode::KEY_LEFTSHIFT, true)).count();
        let shift_ups = events.iter().filter(|(_, ev)| *ev == OutEvent::Key(KeyCode::KEY_LEFTSHIFT, false)).count();
        assert_eq!((shift_downs, shift_ups), (4, 4), "{events:?}");
        // Releasing at 320 ms, mid-tap, lets go of A and Shift together, and nothing follows.
        let after: Vec<&OutEvent> = events.iter().filter(|(t, _)| *t >= 320).map(|(_, ev)| ev).collect();
        assert_eq!(after, [&OutEvent::Key(KeyCode::KEY_A, false), &OutEvent::Key(KeyCode::KEY_LEFTSHIFT, false)]);
        assert!(!e.needs_tick(&p));
    }

    #[test]
    fn toggled_repeating_macro_and_profile_switch_stop() {
        let mut e = macro_engine(vec![MacroStep::Tap { action: key("KEY_A"), hold_ms: 10 }, MacroStep::Wait(40)]);
        let mut p = Profile::passthrough("p");
        p.set_button(Button::West, toggle(ButtonAction::Macro { name: "m".into(), repeat: true }));
        run(&mut e, &p, InputEvent::Button(Button::West, true));
        run(&mut e, &p, InputEvent::Button(Button::West, false));
        let presses = |out: &[OutEvent]| out.iter().filter(|ev| **ev == OutEvent::Key(KeyCode::KEY_A, true)).count();
        assert_eq!(presses(&hold_for(&mut e, &p, 0.5)), 10, "keeps looping hands-off");
        let mut out = Vec::new();
        e.release_all(false, &mut out);
        assert!(!e.needs_tick(&p));
        assert_eq!(presses(&hold_for(&mut e, &p, 0.5)), 0);
    }

    #[test]
    fn repeating_macro_without_waits_cannot_hang() {
        let mut e = macro_engine(vec![MacroStep::Press(key("KEY_A")), MacroStep::Release(key("KEY_A"))]);
        let p = mapped(true);
        run(&mut e, &p, InputEvent::Button(Button::West, true));
        // One pass per tick rather than an infinite loop.
        let out = hold_for(&mut e, &p, 0.04);
        assert_eq!(out.iter().filter(|ev| **ev == OutEvent::Key(KeyCode::KEY_A, true)).count(), 10);
    }

    #[test]
    fn unknown_macro_does_nothing() {
        let mut e = Engine::default();
        assert!(run(&mut e, &mapped(false), InputEvent::Button(Button::West, true)).is_empty());
    }

    #[test]
    fn stick_directions_are_buttons_with_any_action_alongside_the_stick() {
        // The right stick stays a gamepad stick; its directions also act as buttons.
        let mut p = Profile::passthrough("p");
        p.right_stick.deadzone = 0.0;
        p.right_stick.key_threshold = 0.5;
        p.set_button(Button::RightStickUp, toggle(key("KEY_C")));
        p.set_button(Button::RightStickDown, key("KEY_S"));
        p.set_button(Button::RightStickLeft, ButtonAction::Mouse(MouseButton::Left));
        let mut e = Engine::default();
        let c_down = OutEvent::Key(KeyCode::KEY_C, true);
        let stick_y = |v: f32| OutEvent::PadAxis(Axis::RightY, v);

        // Up is a toggle: flicking up and back turns it on, the next flick turns it off.
        assert_eq!(axis(&mut e, &p, Axis::RightY, -0.9), vec![OutEvent::PadAxis(Axis::RightX, 0.0), stick_y(-0.9), c_down]);
        assert!(!axis(&mut e, &p, Axis::RightY, -0.47).iter().any(|ev| matches!(ev, OutEvent::Key(..))), "inside the release margin");
        axis(&mut e, &p, Axis::RightY, 0.0);
        assert!(axis(&mut e, &p, Axis::RightY, -0.9).contains(&OutEvent::Key(KeyCode::KEY_C, false)));
        axis(&mut e, &p, Axis::RightY, 0.0);

        // A diagonal presses both directions; returning to center releases both.
        axis(&mut e, &p, Axis::RightX, -0.7);
        assert!(axis(&mut e, &p, Axis::RightY, 0.7).contains(&OutEvent::Key(KeyCode::KEY_S, true)));
        let mut out = axis(&mut e, &p, Axis::RightX, 0.0);
        out.extend(axis(&mut e, &p, Axis::RightY, 0.0));
        assert!(out.contains(&OutEvent::MouseButton(MouseButton::Left, false)), "{out:?}");
        assert!(out.contains(&OutEvent::Key(KeyCode::KEY_S, false)), "{out:?}");
    }

    #[test]
    fn bumper_plus_stick_direction_combo() {
        let mut p = Profile::passthrough("p");
        p.right_stick.deadzone = 0.0;
        p.combos.push(Combo {
            buttons: vec![Button::LeftBumper, Button::RightStickRight],
            action: key("KEY_F"),
        });
        let mut e = Engine::default();
        let t0 = Instant::now();
        // Holding right on the stick (its own action is Disabled, so it waits as a modifier)...
        assert!(!axis(&mut e, &p, Axis::RightX, 1.0).iter().any(|ev| matches!(ev, OutEvent::Key(..))));
        // ...then pressing LB well afterwards fires the combo instead of LB's own action.
        assert_eq!(press(&mut e, &p, Button::LeftBumper, true, ms(t0, 2000)), vec![OutEvent::Key(KeyCode::KEY_F, true)]);
        assert_eq!(press(&mut e, &p, Button::LeftBumper, false, ms(t0, 2100)), vec![OutEvent::Key(KeyCode::KEY_F, false)]);
        // LB alone still works as a bumper.
        axis(&mut e, &p, Axis::RightX, 0.0);
        press(&mut e, &p, Button::LeftBumper, true, ms(t0, 3000));
        assert_eq!(fire_timers(&mut e, &p, ms(t0, 3100)), vec![OutEvent::PadButton(Button::LeftBumper, true)]);
    }

    #[test]
    fn buttons_can_push_a_stick_direction() {
        let mut p = Profile::passthrough("p");
        p.left_stick.deadzone = 0.0;
        p.set_button(Button::DpadRight, ButtonAction::Gamepad(Button::LeftStickRight));
        p.set_button(Button::DpadUp, ButtonAction::Gamepad(Button::LeftStickUp));
        let mut e = Engine::default();
        let last_stick = |out: &[OutEvent]| -> (f32, f32) {
            let x = out.iter().rev().find_map(|ev| match ev { OutEvent::PadAxis(Axis::LeftX, v) => Some(*v), _ => None });
            let y = out.iter().rev().find_map(|ev| match ev { OutEvent::PadAxis(Axis::LeftY, v) => Some(*v), _ => None });
            (x.unwrap(), y.unwrap())
        };
        assert_eq!(last_stick(&run(&mut e, &p, InputEvent::Button(Button::DpadRight, true))), (1.0, 0.0));
        // Two directions make a diagonal on the stick's circle, not a square corner.
        let (x, y) = last_stick(&run(&mut e, &p, InputEvent::Button(Button::DpadUp, true)));
        let d = std::f32::consts::FRAC_1_SQRT_2;
        assert!((x - d).abs() < 1e-4 && (y + d).abs() < 1e-4, "{x} {y}");
        assert_eq!(last_stick(&run(&mut e, &p, InputEvent::Button(Button::DpadRight, false))), (0.0, -1.0));
        assert_eq!(last_stick(&run(&mut e, &p, InputEvent::Button(Button::DpadUp, false))), (0.0, 0.0));
        // Pushing against the physical stick adds, limited to full deflection.
        axis(&mut e, &p, Axis::LeftX, 0.5);
        assert_eq!(last_stick(&run(&mut e, &p, InputEvent::Button(Button::DpadRight, true))), (1.0, 0.0));
    }

    #[test]
    fn macro_stick_steps_trace_a_motion_and_recenter() {
        let d = std::f32::consts::FRAC_1_SQRT_2;
        let mut e = macro_engine(vec![
            MacroStep::Stick { stick: Stick::Left, x: 0.0, y: 1.0 },
            MacroStep::Wait(16),
            MacroStep::Stick { stick: Stick::Left, x: d, y: d },
            MacroStep::Wait(16),
            MacroStep::Stick { stick: Stick::Left, x: 1.0, y: 0.0 },
            MacroStep::Tap { action: ButtonAction::Gamepad(Button::West), hold_ms: 16 },
        ]);
        let mut p = mapped(false);
        p.left_stick.deadzone = 0.0;
        let events = timeline(&mut e, &p, 0.2, None);
        let sticks: Vec<(u32, (Axis, f32))> = events
            .iter()
            .filter_map(|(t, ev)| match ev {
                OutEvent::PadAxis(a @ (Axis::LeftX | Axis::LeftY), v) => Some((*t, (*a, (v * 100.0).round() / 100.0))),
                _ => None,
            })
            .collect();
        assert_eq!(
            sticks,
            vec![
                (0, (Axis::LeftX, 0.0)),
                (0, (Axis::LeftY, 1.0)),
                (16, (Axis::LeftX, 0.71)),
                (16, (Axis::LeftY, 0.71)),
                (32, (Axis::LeftX, 1.0)),
                (32, (Axis::LeftY, 0.0)),
                // The pass ends 16 ms later and the stick recenters.
                (48, (Axis::LeftX, 0.0)),
                (48, (Axis::LeftY, 0.0)),
            ]
        );
        let punch: Vec<u32> = events
            .iter()
            .filter(|(_, ev)| matches!(ev, OutEvent::PadButton(Button::West, _)))
            .map(|(t, _)| *t)
            .collect();
        assert_eq!(punch, [32, 48], "pressed with the forward input, released 16 ms later");
    }

    #[test]
    fn macro_stick_adds_to_the_physical_stick() {
        let mut e = macro_engine(vec![MacroStep::Stick { stick: Stick::Left, x: 0.5, y: 0.0 }, MacroStep::Wait(100)]);
        let mut p = mapped(false);
        p.left_stick.deadzone = 0.0;
        axis(&mut e, &p, Axis::LeftX, 0.25);
        let out = run(&mut e, &p, InputEvent::Button(Button::West, true));
        assert!(out.contains(&OutEvent::PadAxis(Axis::LeftX, 0.75)), "{out:?}");
    }

    #[test]
    fn show_info_holds_while_pressed_and_toggles() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::Select, ButtonAction::ShowInfo("Keys".into()));
        p.set_button(Button::North, toggle(ButtonAction::ShowInfo("Stats".into())));
        let mut e = Engine::default();
        run(&mut e, &p, InputEvent::Button(Button::Select, true));
        assert!(e.take_info_changed());
        assert_eq!(e.shown_info().collect::<Vec<_>>(), ["Keys"]);
        run(&mut e, &p, InputEvent::Button(Button::Select, false));
        assert_eq!(e.shown_info().count(), 0);
        run(&mut e, &p, InputEvent::Button(Button::North, true));
        run(&mut e, &p, InputEvent::Button(Button::North, false));
        assert_eq!(e.shown_info().collect::<Vec<_>>(), ["Stats"], "toggled on stays up");
        run(&mut e, &p, InputEvent::Button(Button::North, true));
        assert_eq!(e.shown_info().count(), 0);
    }

    #[test]
    fn open_menu_reports_its_opener_and_menu_items_tap() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::Select, ButtonAction::OpenMenu("Pause".into()));
        p.left_trigger = TriggerAction::Button { action: ButtonAction::OpenMenu("Wheel".into()), threshold: 0.5 }.into();
        let mut e = Engine::default();
        run(&mut e, &p, InputEvent::Button(Button::Select, true));
        assert_eq!(e.take_menu_request(), Some(("Pause".into(), Opener { buttons: vec![Button::Select], ..Opener::default() })));
        assert_eq!(e.take_menu_request(), None);
        axis(&mut e, &p, Axis::LeftTrigger, 1.0);
        assert_eq!(e.take_menu_request().map(|(n, o)| (n, o.trigger)), Some(("Wheel".into(), Some(Trigger::Left))));
        // Through a Toggle, the menu is marked to stay up until pressed again.
        p.set_button(Button::North, toggle(ButtonAction::OpenMenu("Pause".into())));
        run(&mut e, &p, InputEvent::Button(Button::North, true));
        assert_eq!(e.take_menu_request().map(|(_, o)| o.toggled), Some(true));

        let mut out = Vec::new();
        let toggle_c = toggle(key("KEY_C"));
        e.tap_menu_item("Pause", 0, &key("KEY_M"), &mut out);
        assert_eq!(out, vec![OutEvent::Key(KeyCode::KEY_M, true), OutEvent::Key(KeyCode::KEY_M, false)]);
        out.clear();
        e.tap_menu_item("Pause", 1, &toggle_c, &mut out);
        e.tap_menu_item("Pause", 1, &toggle_c, &mut out);
        assert_eq!(out, vec![OutEvent::Key(KeyCode::KEY_C, true), OutEvent::Key(KeyCode::KEY_C, false)], "a toggle item flips each time");
    }

    #[test]
    fn toggles_set_to_start_on_switch_on_when_the_game_starts() {
        let starting = |inner| ButtonAction::Toggle(Toggled { action: Box::new(inner), start_on: true });
        let mut p = Profile::passthrough("p");
        p.set_button(Button::North, starting(ButtonAction::ShowInfo("Controls".into())));
        // Inside a Multi, after a Turbo (which takes a slot of its own).
        p.set_button(
            Button::West,
            ButtonAction::Multi(vec![
                ButtonAction::Turbo { action: Box::new(ButtonAction::Mouse(MouseButton::Left)), rate: 5.0 },
                starting(ButtonAction::Keys(vec!["KEY_C".into()])),
            ]),
        );
        p.combos.push(Combo { buttons: vec![Button::RightBumper, Button::LeftBumper], action: starting(ButtonAction::Layer("L".into())) });
        let menu = Menu {
            name: "M".into(),
            kind: crate::config::MenuKind::List,
            items: vec![crate::config::MenuItem { label: "x".into(), action: starting(ButtonAction::Keys(vec!["KEY_M".into()])), button: None }],
            cancel: None,
            style: Default::default(),
        };
        let mut e = Engine::default();
        let mut out = Vec::new();
        e.start_toggles(&p, std::slice::from_ref(&menu), &mut out);
        assert_eq!(e.shown_info().collect::<Vec<_>>(), ["Controls"]);
        assert_eq!(e.layers(), ["L"]);
        assert!(out.contains(&OutEvent::Key(KeyCode::KEY_C, true)) && out.contains(&OutEvent::Key(KeyCode::KEY_M, true)));
        // Again does nothing: they're on already.
        let mut again = Vec::new();
        e.start_toggles(&p, std::slice::from_ref(&menu), &mut again);
        assert!(again.is_empty(), "{again:?}");
        // Pressing the button turns it off, like any toggle, and the Multi's toggle too.
        run(&mut e, &p, InputEvent::Button(Button::North, true));
        assert_eq!(e.shown_info().count(), 0);
        let out = run(&mut e, &p, InputEvent::Button(Button::West, true));
        assert!(out.contains(&OutEvent::Key(KeyCode::KEY_C, false)), "{out:?}");
        // The menu item turns off when chosen.
        let mut out = Vec::new();
        e.tap_menu_item("M", 0, &menu.items[0].action, &mut out);
        assert_eq!(out, [OutEvent::Key(KeyCode::KEY_M, false)]);
    }

    // Layers: the engine keeps the stack; the daemon hands it the profile with them on top.

    fn layered(base: &Profile, layer: &crate::config::Layer) -> Profile {
        base.with_layers([layer])
    }

    #[test]
    fn layers_stack_and_count_their_holders() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::LeftBumper, ButtonAction::Layer("A".into()));
        p.set_button(Button::RightBumper, ButtonAction::Layer("B".into()));
        p.set_button(Button::West, ButtonAction::Layer("A".into()));
        p.set_button(Button::North, ButtonAction::toggle(ButtonAction::Layer("A".into())));
        let mut e = Engine::default();
        run(&mut e, &p, InputEvent::Button(Button::LeftBumper, true));
        run(&mut e, &p, InputEvent::Button(Button::RightBumper, true));
        assert_eq!(e.layers(), ["A", "B"], "oldest first");
        assert!(e.take_layers_changed() && !e.take_layers_changed());
        run(&mut e, &p, InputEvent::Button(Button::West, true));
        run(&mut e, &p, InputEvent::Button(Button::LeftBumper, false));
        assert_eq!(e.layers(), ["A", "B"], "West still holds A");
        run(&mut e, &p, InputEvent::Button(Button::West, false));
        assert_eq!(e.layers(), ["B"]);
        // A toggle keeps one on after release.
        run(&mut e, &p, InputEvent::Button(Button::North, true));
        run(&mut e, &p, InputEvent::Button(Button::North, false));
        assert_eq!(e.layers(), ["B", "A"]);
        run(&mut e, &p, InputEvent::Button(Button::North, true));
        assert_eq!(e.layers(), ["B"]);
    }

    #[test]
    fn toggled_layers_survive_menus_but_not_game_changes() {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::North, ButtonAction::toggle(ButtonAction::Layer("A".into())));
        p.set_button(Button::LeftBumper, ButtonAction::Layer("B".into()));
        let mut e = Engine::default();
        run(&mut e, &p, InputEvent::Button(Button::North, true));
        run(&mut e, &p, InputEvent::Button(Button::North, false));
        run(&mut e, &p, InputEvent::Button(Button::LeftBumper, true));
        let mut out = Vec::new();
        e.release_all(true, &mut out);
        assert_eq!(e.layers(), ["A"], "held ones end, toggled ones stay");
        // Still toggled: the next press turns it off.
        run(&mut e, &p, InputEvent::Button(Button::North, true));
        assert!(e.layers().is_empty());
        run(&mut e, &p, InputEvent::Button(Button::North, false));
        run(&mut e, &p, InputEvent::Button(Button::North, true));
        e.release_all(false, &mut out);
        assert!(e.layers().is_empty());
    }

    #[test]
    fn a_held_button_keeps_its_action_across_a_layer_change() {
        let base = Profile::passthrough("p");
        let mut layer = crate::config::Layer::new("L");
        layer.buttons.insert(Button::South, ButtonAction::Keys(vec!["KEY_F1".into()]));
        let on = layered(&base, &layer);
        let mut e = Engine::default();
        assert_eq!(run(&mut e, &base, InputEvent::Button(Button::South, true)), [OutEvent::PadButton(Button::South, true)]);
        // The layer comes on while A is down: A still releases the pad button.
        assert_eq!(run(&mut e, &on, InputEvent::Button(Button::South, false)), [OutEvent::PadButton(Button::South, false)]);
        assert_eq!(run(&mut e, &on, InputEvent::Button(Button::South, true)), [OutEvent::Key(KeyCode::KEY_F1, true)]);
    }

    #[test]
    fn analog_inputs_switch_modes_without_getting_stuck() {
        let base = Profile::passthrough("p");
        let mut layer = crate::config::Layer::new("L");
        layer.left_stick = Some(StickConfig::new(wasd(), 0.1, 1.0));
        layer.right_stick = Some(StickConfig::new(StickAction::mouse(1000.0), 0.1, 1.0));
        layer.right_trigger = Some(TriggerAction::Gamepad(Trigger::Right).into());
        let on = layered(&base, &layer);
        let mut e = Engine::default();

        // Right stick: pad stick, then mouse; the virtual stick recenters.
        axis(&mut e, &base, Axis::RightX, 0.8);
        let mut out = Vec::new();
        e.resync(&on, &mut out);
        assert!(out.contains(&OutEvent::PadAxis(Axis::RightX, 0.0)), "{out:?}");

        // Left stick: keys in the layer. W pressed under the layer stays down after it ends,
        // until the stick lets go of that direction.
        let held = axis(&mut e, &on, Axis::LeftY, -0.9);
        assert!(held.contains(&OutEvent::Key(KeyCode::KEY_W, true)));
        let mut out = Vec::new();
        e.resync(&base, &mut out);
        assert!(!out.contains(&OutEvent::Key(KeyCode::KEY_W, false)), "still pushed up: {out:?}");
        assert!(axis(&mut e, &base, Axis::LeftY, 0.0).contains(&OutEvent::Key(KeyCode::KEY_W, false)));

        // A trigger pull pressed as a button releases by its own threshold after the layer
        // made the trigger analog, and the analog trigger doesn't stay pulled afterwards.
        let mut pulled = Profile::passthrough("p");
        pulled.right_trigger = TriggerAction::Button { action: ButtonAction::Mouse(MouseButton::Left), threshold: 0.5 }.into();
        let pulled_on = layered(&pulled, &layer);
        // (The virtual trigger the resync above fed under the layer goes back to rest first.)
        let out = axis(&mut e, &pulled, Axis::RightTrigger, 0.9);
        assert_eq!(out, [OutEvent::PadAxis(Axis::RightTrigger, 0.0), OutEvent::MouseButton(MouseButton::Left, true)]);
        let out = axis(&mut e, &pulled_on, Axis::RightTrigger, 0.2);
        assert!(out.contains(&OutEvent::MouseButton(MouseButton::Left, false)), "{out:?}");
        let out = axis(&mut e, &pulled, Axis::RightTrigger, 0.1);
        assert!(out.contains(&OutEvent::PadAxis(Axis::RightTrigger, 0.0)), "{out:?}");
    }

    #[test]
    fn active_zones_release_by_their_own_bounds() {
        let mut base = Profile::passthrough("p");
        base.left_stick.zones.push(Zone { min: 0.0, max: 0.5, action: ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into()]) });
        let mut layer = crate::config::Layer::new("L");
        let mut stick = base.left_stick.clone();
        stick.zones = vec![Zone { min: 0.6, max: 1.0, action: ButtonAction::Keys(vec!["KEY_X".into()]) }];
        layer.left_stick = Some(stick);
        let on = layered(&base, &layer);
        let mut e = Engine::default();
        assert!(axis(&mut e, &base, Axis::LeftX, 0.3).contains(&OutEvent::Key(KeyCode::KEY_LEFTSHIFT, true)));
        // Under the layer, zone 0 means something else; Shift stays until 0.5 is passed.
        let out = axis(&mut e, &on, Axis::LeftX, 0.4);
        assert!(!out.iter().any(|o| matches!(o, OutEvent::Key(..))), "{out:?}");
        // Past it, Shift releases and the layer's zone presses, in that order.
        let out = axis(&mut e, &on, Axis::LeftX, 0.8);
        let keys: Vec<&OutEvent> = out.iter().filter(|o| matches!(o, OutEvent::Key(..))).collect();
        assert_eq!(keys, [&OutEvent::Key(KeyCode::KEY_LEFTSHIFT, false), &OutEvent::Key(KeyCode::KEY_X, true)]);
    }
}

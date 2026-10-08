//! Random input sequences: whatever the player does, releasing everything must leave no key,
//! button or mouse button held. A stuck key keeps typing after a controller is unplugged or the
//! profile changes, so this is the failure to rule out first.

use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

use super::*;
use crate::{
    config::{Macro, MacroStep, MouseButton},
    input::{Axis, InputEvent},
    output::OutEvent,
};

/// A small deterministic generator, so a failing seed can be replayed.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[(self.next() % items.len() as u64) as usize]
    }

    /// A value in -1.0..=1.0.
    fn unit(&mut self) -> f32 {
        (self.next() % 2001) as f32 / 1000.0 - 1.0
    }
}

fn key(k: &str) -> ButtonAction {
    ButtonAction::Keys(vec![k.into()])
}

/// The identity of an output that is a press or release, and whether it is a press.
fn held_identity(ev: &OutEvent) -> Option<(String, bool)> {
    match ev {
        OutEvent::Key(k, down) => Some((format!("key {k:?}"), *down)),
        OutEvent::PadButton(b, down) => Some((format!("pad {b:?}"), *down)),
        OutEvent::MouseButton(b, down) => Some((format!("mouse {b:?}"), *down)),
        _ => None,
    }
}

/// Keeps `held` equal to the outputs that are down after `out` was sent. A later press or
/// release of the same output replaces the earlier one, as it does on the kernel side.
fn track(held: &mut BTreeSet<String>, out: &[OutEvent]) {
    for ev in out {
        if let Some((id, down)) = held_identity(ev) {
            if down {
                held.insert(id);
            } else {
                held.remove(&id);
            }
        }
    }
}

pub(crate) fn random_event(rng: &mut Rng) -> InputEvent {
    match rng.next() % 4 {
        0 | 1 => InputEvent::Button(rng.pick(&Button::ALL), rng.next().is_multiple_of(2)),
        2 => InputEvent::Axis(rng.pick(&[Axis::LeftX, Axis::LeftY, Axis::RightX, Axis::RightY]), rng.unit()),
        _ => InputEvent::Axis(rng.pick(&[Axis::LeftTrigger, Axis::RightTrigger]), (rng.unit() + 1.0) / 2.0),
    }
}

/// A profile that uses the kinds of action most likely to leave something held: toggles,
/// turbo, multi-key chords, a repeating macro and a button ring.
pub(crate) fn busy_profile() -> Profile {
    let mut p = Profile::pc_action("Busy");
    p.set_button(Button::South, ButtonAction::toggle(key("KEY_LEFTCTRL")));
    p.set_button(Button::East, ButtonAction::Turbo { action: Box::new(ButtonAction::Mouse(MouseButton::Left)), rate: 12.0, every_ms: 0 });
    p.set_button(Button::West, ButtonAction::toggle(ButtonAction::Turbo { action: Box::new(key("KEY_R")), rate: 8.0, every_ms: 0 }));
    p.set_button(Button::North, ButtonAction::Multi(vec![key("KEY_A"), ButtonAction::Mouse(MouseButton::Right)]));
    p.set_button(Button::LeftBumper, ButtonAction::Macro { name: "loop".into(), repeat: true });
    p.set_button(Button::RightBumper, ButtonAction::Macro { name: "once".into(), repeat: false });
    p
}

pub(crate) fn engine() -> Engine {
    let mut e = Engine::default();
    e.set_macros(&[
        Macro {
            name: "loop".into(),
            steps: vec![
                MacroStep::Press(key("KEY_LEFTSHIFT")),
                MacroStep::Tap { action: key("KEY_W"), hold_ms: 30 },
                MacroStep::Wait(20),
            ],
        },
        Macro {
            name: "once".into(),
            steps: vec![MacroStep::Press(key("KEY_LEFTALT")), MacroStep::Wait(200), MacroStep::Release(key("KEY_LEFTALT"))],
        },
    ]);
    e
}

#[test]
fn releasing_everything_leaves_nothing_held_and_axes_stay_in_range() {
    let profiles = [Profile::passthrough("Pass"), Profile::desktop("Desk"), Profile::pc_action("Play"), busy_profile()];
    let step = Duration::from_millis(16);
    for profile in &profiles {
        for seed in 1..=60u64 {
            let mut e = engine();
            let mut rng = Rng::new(seed);
            let mut held = BTreeSet::new();
            let mut now = Instant::now();
            for _ in 0..400 {
                let mut out = Vec::new();
                e.handle(profile, random_event(&mut rng), now, &mut out);
                e.timers(profile, now, &mut out);
                if e.needs_tick(profile) {
                    e.tick(profile, step.as_secs_f32(), &mut out);
                }
                track(&mut held, &out);
                for ev in &out {
                    if let OutEvent::PadAxis(axis, v) = ev {
                        assert!(v.is_finite() && (-1.0..=1.0).contains(v), "profile {}, seed {seed}: {axis:?} at {v}", profile.name);
                    }
                }
                now += step;
            }

            let mut out = Vec::new();
            e.release_all(false, &mut out);
            track(&mut held, &out);
            assert!(held.is_empty(), "profile {}, seed {seed}: still held after release_all: {held:?}", profile.name);
        }
    }
}

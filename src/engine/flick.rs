//! Flick stick: pushing the stick out turns the camera to that direction, then rotating the
//! stick turns it by the same angle.

use super::*;
use crate::config::{FlickVertical, StickConfig};

/// Turning slower than this (degrees per tick) is stick noise and ignored.
const ROTATE_NOISE_DEGREES: f32 = 0.05;

/// A flick stick's settings, as `tick` uses them.
#[derive(Debug, Clone, Copy)]
pub(super) struct FlickParams {
    pub full_turn_px: f32,
    pub threshold: f32,
    pub flick_secs: f32,
    pub rotate_smoothing_secs: f32,
    pub forward_deadzone: f32,
    pub vertical: FlickVertical,
    pub vertical_speed: f32,
    pub curve: f32,
}

/// One stick's flick progress.
#[derive(Debug, Default)]
pub(super) struct FlickState {
    /// Pushed out past the threshold, so rotation counts.
    active: bool,
    /// Angle from up (degrees, clockwise) at the last tick.
    last_angle: f32,
    /// Flicks still being spread out: (total pixels, seconds elapsed, seconds long).
    flicks: Vec<(f32, f32, f32)>,
    /// Low-pass filtered rotation, in pixels per tick.
    smoothed: f32,
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

impl FlickState {
    /// Whether it still has movement to send even with the stick at rest.
    pub(super) fn busy(&self) -> bool {
        self.active || !self.flicks.is_empty()
    }

    /// Advances by `dt` seconds with the stick at `(x, y)` (up is negative y) and returns the
    /// mouse movement in pixels.
    pub(super) fn step(&mut self, p: &FlickParams, (x, y): (f32, f32), dt: f32) -> (f32, f32) {
        let mag = x.hypot(y);
        let angle = x.atan2(-y).to_degrees();
        let engaged = if self.active { p.threshold - STICK_DIRECTION_HYSTERESIS } else { p.threshold };
        let mut dx = 0.0;
        if mag > 0.0 && mag >= engaged {
            if self.active {
                let turned = angle_diff(angle, self.last_angle);
                let raw = if turned.abs() >= ROTATE_NOISE_DEGREES { turned / 360.0 * p.full_turn_px } else { 0.0 };
                dx += if p.rotate_smoothing_secs > 0.0 {
                    self.smoothed += (raw - self.smoothed) * (1.0 - (-dt / p.rotate_smoothing_secs).exp());
                    self.smoothed
                } else {
                    raw
                };
            } else {
                self.active = true;
                self.smoothed = 0.0;
                if angle.abs() > p.forward_deadzone {
                    self.flicks.push((angle / 360.0 * p.full_turn_px, 0.0, p.flick_secs.max(0.001)));
                }
            }
            self.last_angle = angle;
        } else {
            self.active = false;
            self.smoothed = 0.0;
        }
        for (total, elapsed, length) in &mut self.flicks {
            let from = (*elapsed / *length).min(1.0);
            *elapsed += dt;
            let to = (*elapsed / *length).min(1.0);
            dx += *total * (ease(to) - ease(from));
        }
        self.flicks.retain(|(_, elapsed, length)| elapsed < length);
        let dy = if p.vertical == FlickVertical::Look && mag > 0.0 {
            y * (mag.min(1.0).powf(p.curve.max(0.1)) / mag) * p.vertical_speed * dt
        } else {
            0.0
        };
        (dx, dy)
    }
}

impl Engine {
    /// Whether a flick stick has anything to do: it's deflected or still turning.
    pub(super) fn flick_needs_tick(&self, s: Stick, deadzone: f32) -> bool {
        self.stick_pos(s, deadzone) != (0.0, 0.0) || self.flick.get(&s).is_some_and(FlickState::busy)
    }

    /// Advances one flick stick and emits its mouse movement.
    pub(super) fn tick_flick(&mut self, s: Stick, cfg: &StickConfig, dt: f32, out: &mut Vec<OutEvent>) {
        let StickAction::Flick { full_turn_px, flick_threshold, flick_time_ms, rotate_smoothing_ms, forward_deadzone, vertical, vertical_speed } =
            &cfg.action
        else {
            return;
        };
        let params = FlickParams {
            full_turn_px: *full_turn_px,
            threshold: *flick_threshold,
            flick_secs: *flick_time_ms as f32 / 1000.0,
            rotate_smoothing_secs: *rotate_smoothing_ms as f32 / 1000.0,
            forward_deadzone: *forward_deadzone,
            vertical: *vertical,
            vertical_speed: *vertical_speed,
            curve: cfg.curve,
        };
        let pos = self.stick_pos(s, cfg.deadzone);
        let (dx, dy) = self.flick.entry(s).or_default().step(&params, pos, dt);
        let (dx, dy) = take_whole(&mut self.mouse_acc, dx, dy);
        if dx != 0 || dy != 0 {
            out.push(OutEvent::MouseMove(dx, dy));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> FlickParams {
        FlickParams {
            full_turn_px: 3600.0,
            threshold: 0.9,
            flick_secs: 0.1,
            rotate_smoothing_secs: 0.0,
            forward_deadzone: 0.0,
            vertical: FlickVertical::Off,
            vertical_speed: 1000.0,
            curve: 1.0,
        }
    }

    /// Steps for `secs` of 4 ms ticks with the stick held at `pos`, returning the total movement.
    fn hold(st: &mut FlickState, p: &FlickParams, pos: (f32, f32), secs: f32) -> (f32, f32) {
        let mut total = (0.0, 0.0);
        for _ in 0..(secs / 0.004).round() as usize {
            let (dx, dy) = st.step(p, pos, 0.004);
            total = (total.0 + dx, total.1 + dy);
        }
        total
    }

    fn at(degrees: f32, mag: f32) -> (f32, f32) {
        (degrees.to_radians().sin() * mag, -degrees.to_radians().cos() * mag)
    }

    #[test]
    fn a_flick_turns_to_the_stick_direction_over_the_flick_time() {
        let (p, mut st) = (params(), FlickState::default());
        // 90 degrees right is a quarter of 3600 px, all sent within the flick time.
        let (dx, dy) = hold(&mut st, &p, at(90.0, 1.0), 0.05);
        assert!(dx > 0.0 && dx < 900.0, "half-way through: {dx}");
        assert_eq!(dy, 0.0);
        let (more, _) = hold(&mut st, &p, at(90.0, 1.0), 0.2);
        assert!((dx + more - 900.0).abs() < 1.0, "total {}", dx + more);
        // Holding still adds nothing.
        assert_eq!(hold(&mut st, &p, at(90.0, 1.0), 0.5), (0.0, 0.0));
    }

    #[test]
    fn a_left_flick_turns_left() {
        let (p, mut st) = (params(), FlickState::default());
        let (dx, _) = hold(&mut st, &p, at(-90.0, 1.0), 0.3);
        assert!((dx + 900.0).abs() < 1.0, "{dx}");
    }

    #[test]
    fn rotating_the_stick_turns_by_the_same_angle() {
        let (p, mut st) = (params(), FlickState::default());
        hold(&mut st, &p, at(90.0, 1.0), 0.3);
        let mut total = 0.0;
        for i in 1..=45 {
            total += st.step(&p, at(90.0 + i as f32, 1.0), 0.004).0;
        }
        assert!((total - 450.0).abs() < 1.0, "45 degrees of 360 is an eighth: {total}");
        // And back the other way.
        for i in 1..=45 {
            total += st.step(&p, at(135.0 - i as f32, 1.0), 0.004).0;
        }
        assert!(total.abs() < 1.0, "{total}");
    }

    #[test]
    fn rotation_wraps_through_straight_down() {
        let (p, mut st) = (params(), FlickState::default());
        hold(&mut st, &p, at(170.0, 1.0), 0.3);
        let mut total = 0.0;
        for i in 1..=20 {
            total += st.step(&p, at(170.0 + i as f32, 1.0), 0.004).0;
        }
        assert!((total - 200.0).abs() < 1.0, "20 degrees across the wrap: {total}");
    }

    #[test]
    fn letting_go_ends_the_flick_and_a_new_push_flicks_again() {
        let (p, mut st) = (params(), FlickState::default());
        hold(&mut st, &p, at(90.0, 1.0), 0.3);
        hold(&mut st, &p, (0.0, 0.0), 0.05);
        assert!(!st.busy());
        let (dx, _) = hold(&mut st, &p, at(170.0, 1.0), 0.3);
        assert!((dx - 1700.0).abs() < 1.0, "{dx}");
    }

    #[test]
    fn a_push_below_the_threshold_does_nothing() {
        let (p, mut st) = (params(), FlickState::default());
        assert_eq!(hold(&mut st, &p, at(90.0, 0.8), 0.3), (0.0, 0.0));
    }

    #[test]
    fn the_forward_dead_angle_suppresses_a_straight_ahead_flick() {
        let p = FlickParams { forward_deadzone: 10.0, ..params() };
        let mut st = FlickState::default();
        assert_eq!(hold(&mut st, &p, at(5.0, 1.0), 0.3), (0.0, 0.0));
        let mut st = FlickState::default();
        let (dx, _) = hold(&mut st, &p, at(20.0, 1.0), 0.3);
        assert!(dx > 100.0, "{dx}");
    }

    #[test]
    fn vertical_look_follows_the_stick_up_and_down() {
        let p = FlickParams { vertical: FlickVertical::Look, ..params() };
        let mut st = FlickState::default();
        // Nearly straight down: 985 px/s down (cos 10 degrees), and the flick itself still turns.
        let (dx, dy) = hold(&mut st, &p, at(170.0, 1.0), 1.0);
        assert!((dy - 985.0).abs() < 3.0, "{dy}");
        assert!((dx - 1700.0).abs() < 1.0, "{dx}");
        let mut st = FlickState::default();
        let (_, off) = hold(&mut st, &params(), at(170.0, 1.0), 1.0);
        assert_eq!(off, 0.0);
    }

    #[test]
    fn rotation_smoothing_lags_but_still_follows() {
        let p = FlickParams { rotate_smoothing_secs: 0.05, ..params() };
        let mut st = FlickState::default();
        hold(&mut st, &p, at(90.0, 1.0), 0.3);
        let mut total = 0.0;
        for i in 1..=45 {
            total += st.step(&p, at(90.0 + i as f32, 1.0), 0.004).0;
        }
        assert!(total > 0.0 && total < 450.0, "{total}");
    }
}

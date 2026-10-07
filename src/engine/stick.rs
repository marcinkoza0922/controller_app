//! Stick processing: virtual-pad feeds, direction keys and zones, and the continuous mouse and
//! scroll output driven by `tick`.

use super::*;
use crate::config::{MouseResponse, StickConfig};

/// Deflection above which a mouse stick counts as fully pushed, for the outer boost and acceleration.
const OUTER_EDGE: f32 = 0.9;
/// A smoothed stick this close to rest snaps to it.
const SMOOTH_REST: f32 = 0.002;

impl Engine {
    pub(super) fn stick_pos(&self, s: Stick, deadzone: f32) -> (f32, f32) {
        let (ax, ay) = stick_axes(s);
        let x = self.axes.get(&ax).copied().unwrap_or(0.0);
        let y = self.axes.get(&ay).copied().unwrap_or(0.0);
        apply_deadzone(x, y, deadzone)
    }

    pub(super) fn stick(&mut self, profile: &Profile, s: Stick, out: &mut Vec<OutEvent>) -> bool {
        let cfg = profile.stick(s);
        let (x, y) = self.stick_pos(s, cfg.deadzone);
        // A virtual stick no longer fed (a layer changed this stick's mode) recenters.
        let target = match &cfg.action {
            StickAction::Gamepad { stick, .. } => Some(*stick),
            _ => None,
        };
        if let Some(old) = self.pad_feeds.get(&s).copied()
            && Some(old) != target
        {
            self.pad_feeds.remove(&s);
            self.pad_sticks.remove(&old);
            self.emit_pad_stick(old, out);
        }
        if let StickAction::Gamepad { stick, invert_y } = &cfg.action {
            self.pad_feeds.insert(s, *stick);
            self.pad_sticks.insert(*stick, (x, if *invert_y { -y } else { y }));
            self.emit_pad_stick(*stick, out);
        }
        // Direction keys, also released after a layer changed the mode: each direction keeps
        // the key it pressed until it's let go. (Mouse and scroll are driven by `tick`.)
        let names: [Option<&String>; 4] = match &cfg.action {
            StickAction::Keys { up, down, left, right } => [Some(up), Some(down), Some(left), Some(right)],
            _ => [None; 4],
        };
        let t = cfg.key_threshold;
        let active = [y < -t, y > t, x < -t, x > t];
        let held = self.stick_keys.entry(s).or_default();
        let mut now_held = Vec::new();
        for (dir, (on, name)) in active.into_iter().zip(names).enumerate() {
            match (on, held.iter().find(|(d, _)| *d == dir).map(|(_, k)| *k)) {
                (true, Some(k)) => now_held.push((dir, k)),
                (true, None) => {
                    if let Some(k) = name.and_then(|n| parse_key(n)) {
                        out.push(OutEvent::Key(k, true));
                        now_held.push((dir, k));
                    }
                }
                (false, Some(k)) => out.push(OutEvent::Key(k, false)),
                (false, None) => {}
            }
        }
        *held = now_held;
        let switch = self.update_ring(cfg, s, (x, y), out);
        self.zones(profile, Analog::Stick(s), x.hypot(y).min(1.0), out) | switch
    }

    /// Sends a virtual-pad stick: the physical stick mapped to it plus gyro, macro and
    /// button-pushed deflection, limited to the stick's circle.
    pub(super) fn emit_pad_stick(&self, target: Stick, out: &mut Vec<OutEvent>) {
        let (px, py) = self.pad_sticks.get(&target).copied().unwrap_or_default();
        let (gx, gy) = match self.gyro.stick {
            Some((s, v)) if s == target => v,
            _ => (0.0, 0.0),
        };
        let (mx, my) = self.macro_sticks.get(&target).copied().unwrap_or_default();
        let (bx, by) = self.pushed(target);
        let (mut x, mut y) = (px + gx + mx + bx, py + gy + my + by);
        let mag = x.hypot(y);
        if mag > 1.0 {
            x /= mag;
            y /= mag;
        }
        let (ax, ay) = stick_axes(target);
        out.push(OutEvent::PadAxis(ax, x));
        out.push(OutEvent::PadAxis(ay, y));
    }

    /// Direction actions push a stick: one direction is full deflection, two make a
    /// diagonal of the same length.
    pub(super) fn pushed(&self, target: Stick) -> (f32, f32) {
        let (mut x, mut y) = (0.0, 0.0);
        for b in self.pushed_directions.keys() {
            if let Some((s, (dx, dy))) = b.stick_direction()
                && s == target
            {
                x += dx;
                y += dy;
            }
        }
        let mag = f32::hypot(x, y);
        if mag > 1.0 { (x / mag, y / mag) } else { (x, y) }
    }

    /// Whether any stick has continuous output to produce: a mouse or scroll stick is deflected,
    /// or a smoothed one is still settling.
    pub(super) fn sticks_need_tick(&self, profile: &Profile) -> bool {
        [Stick::Left, Stick::Right].into_iter().any(|s| {
            let cfg = profile.stick(s);
            matches!(cfg.action, StickAction::Mouse { .. } | StickAction::Scroll { .. })
                && (self.stick_pos(s, cfg.deadzone) != (0.0, 0.0) || self.stick_smooth.contains_key(&s))
        })
    }

    /// The stick's position as the mouse response sees it: smoothed with a low-pass filter when
    /// `smoothing_ms` is set, which keeps settling toward rest after the stick is let go.
    fn smoothed_pos(&mut self, s: Stick, raw: (f32, f32), smoothing_ms: u32, dt: f32) -> (f32, f32) {
        if smoothing_ms == 0 {
            self.stick_smooth.remove(&s);
            return raw;
        }
        let tau = smoothing_ms as f32 / 1000.0;
        let alpha = 1.0 - (-dt / tau).exp();
        let prev = self.stick_smooth.get(&s).copied().unwrap_or_default();
        let next = (prev.0 + (raw.0 - prev.0) * alpha, prev.1 + (raw.1 - prev.1) * alpha);
        if raw == (0.0, 0.0) && next.0.hypot(next.1) < SMOOTH_REST {
            self.stick_smooth.remove(&s);
            return (0.0, 0.0);
        }
        self.stick_smooth.insert(s, next);
        next
    }

    /// Speed multiplier from acceleration and the outer boost at deflection `mag`; advances the
    /// stick's time at full deflection.
    fn response_gain(&mut self, s: Stick, mag: f32, r: &MouseResponse, dt: f32) -> f32 {
        let edge = ((mag - OUTER_EDGE) / (1.0 - OUTER_EDGE)).clamp(0.0, 1.0);
        if mag < OUTER_EDGE {
            self.stick_ramp.remove(&s);
        } else if r.accel > 0.0 {
            *self.stick_ramp.entry(s).or_insert(0.0) += dt;
        }
        let ramp = match self.stick_ramp.get(&s) {
            Some(held) => (held * 1000.0 / r.accel_ramp_ms.max(1) as f32).min(1.0),
            None => 0.0,
        };
        (1.0 + r.outer_boost * edge) * (1.0 + r.accel * ramp)
    }

    /// Advances the sticks' continuous outputs (mouse motion, scrolling) by `dt` seconds.
    pub(super) fn tick_sticks(&mut self, profile: &Profile, dt: f32, out: &mut Vec<OutEvent>) {
        for s in [Stick::Left, Stick::Right] {
            let cfg = profile.stick(s);
            let response = match &cfg.action {
                StickAction::Mouse { response, .. } => *response,
                _ => MouseResponse::default(),
            };
            let raw = self.stick_pos(s, cfg.deadzone);
            let (x, y) = self.smoothed_pos(s, raw, response.smoothing_ms, dt);
            let mag = x.hypot(y).min(1.0);
            if mag == 0.0 {
                self.stick_ramp.remove(&s);
                continue;
            }
            let gain = mag.powf(cfg.curve.max(0.1)) / mag * self.response_gain(s, mag, &response, dt);
            let (x, y) = (x * gain, y * gain);
            match cfg.action {
                StickAction::Mouse { speed, .. } => {
                    let (dx, dy) = take_whole(&mut self.mouse_acc, x * speed * dt, y * speed * response.y_scale * dt);
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
    }
}

/// The geometry of a button ring.
#[derive(Clone, Copy)]
struct RingShape {
    sectors: u8,
    start_angle: f32,
    inner_radius: f32,
    hysteresis: f32,
}

/// The ring sector a stick at `(x, y)` (screen coordinates: up is negative y) points into, given
/// the sector it is in now. Sectors are `360 / sectors` degrees wide, the first centered
/// `start_angle` degrees clockwise from up. The current sector is kept until the stick has gone
/// `hysteresis` of a sector width past its edge, and until it drops `STICK_DIRECTION_HYSTERESIS`
/// below `inner_radius`.
fn pick_sector(current: Option<usize>, (x, y): (f32, f32), ring: &RingShape) -> Option<usize> {
    let RingShape { sectors, start_angle, inner_radius, hysteresis } = *ring;
    let mag = x.hypot(y);
    let engaged = if current.is_some() { inner_radius - STICK_DIRECTION_HYSTERESIS } else { inner_radius };
    if mag <= 0.0 || mag < engaged {
        return None;
    }
    let n = usize::from(sectors.max(1));
    let width = 360.0 / n as f32;
    let angle = x.atan2(-y).to_degrees();
    if let Some(c) = current.filter(|c| *c < n)
        && angle_diff(angle, start_angle + c as f32 * width).abs() <= width / 2.0 + hysteresis * width
    {
        return Some(c);
    }
    Some(((angle - start_angle + width / 2.0).rem_euclid(360.0) / width) as usize % n)
}

impl Engine {
    /// Holds the action of the ring sector the stick points into, releasing the previous one.
    /// A stick no longer in ring mode (a layer changed it) lets go of its sector.
    pub(super) fn update_ring(&mut self, cfg: &StickConfig, s: Stick, pos: (f32, f32), out: &mut Vec<OutEvent>) -> bool {
        let current = self.ring_sector.get(&s).copied();
        let target = match &cfg.action {
            StickAction::Ring { sectors, start_angle, inner_radius, hysteresis, .. } => {
                let shape = RingShape { sectors: *sectors, start_angle: *start_angle, inner_radius: *inner_radius, hysteresis: *hysteresis };
                pick_sector(current, pos, &shape)
            }
            _ => None,
        };
        if target == current {
            return false;
        }
        let mut switch = false;
        if let Some(old) = current {
            self.ring_sector.remove(&s);
            switch |= self.digital(&Source::RingSector(s, old), &ButtonAction::Disabled, false, out);
        }
        if let Some(new) = target {
            self.ring_sector.insert(s, new);
            let action = cfg.action.ring_actions().get(new).cloned().unwrap_or(ButtonAction::Disabled);
            switch |= self.digital(&Source::RingSector(s, new), &action, true, out);
        }
        switch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse_profile(response: MouseResponse) -> Profile {
        let mut p = Profile::passthrough("p");
        p.right_stick = StickConfig::new(StickAction::Mouse { speed: 1000.0, response }, 0.0, 1.0);
        p
    }

    fn push(e: &mut Engine, p: &Profile, x: f32, y: f32) {
        let mut out = Vec::new();
        e.handle(p, InputEvent::Axis(Axis::RightX, x), Instant::now(), &mut out);
        e.handle(p, InputEvent::Axis(Axis::RightY, y), Instant::now(), &mut out);
    }

    /// Total mouse movement over `secs` of 4 ms ticks.
    fn travel(e: &mut Engine, p: &Profile, secs: f32) -> (i32, i32) {
        let mut out = Vec::new();
        for _ in 0..(secs / 0.004).round() as usize {
            e.tick(p, 0.004, &mut out);
        }
        out.iter().fold((0, 0), |(x, y), o| match o {
            OutEvent::MouseMove(dx, dy) => (x + dx, y + dy),
            _ => (x, y),
        })
    }

    #[test]
    fn acceleration_builds_only_at_full_deflection() {
        let p = mouse_profile(MouseResponse { accel: 1.0, accel_ramp_ms: 400, ..MouseResponse::default() });
        let mut e = Engine::default();
        push(&mut e, &p, 0.5, 0.0);
        let (partial, _) = travel(&mut e, &p, 1.0);
        assert!((498..=500).contains(&partial), "partial push moved {partial}px");

        push(&mut e, &p, 1.0, 0.0);
        let (full, _) = travel(&mut e, &p, 1.0);
        // 0.4 s of ramp averaging 1.5x, then 0.6 s at 2x: 0.4 * 1500 + 0.6 * 2000.
        assert!((1790..=1810).contains(&full), "full push moved {full}px");

        // Letting go resets the ramp.
        push(&mut e, &p, 0.0, 0.0);
        travel(&mut e, &p, 0.1);
        push(&mut e, &p, 1.0, 0.0);
        let (again, _) = travel(&mut e, &p, 0.1);
        assert!(again < 200, "ramp restarted, moved {again}px in 0.1s");
    }

    #[test]
    fn vertical_scale_changes_only_vertical_speed() {
        let p = mouse_profile(MouseResponse { y_scale: 0.5, ..MouseResponse::default() });
        let mut e = Engine::default();
        push(&mut e, &p, 0.5, 0.5);
        let (x, y) = travel(&mut e, &p, 1.0);
        assert!((x - 2 * y).abs() <= 2, "x {x}, y {y}");
    }

    #[test]
    fn outer_boost_applies_only_near_the_edge() {
        let p = mouse_profile(MouseResponse { outer_boost: 0.5, ..MouseResponse::default() });
        let mut e = Engine::default();
        push(&mut e, &p, 0.8, 0.0);
        let (inner, _) = travel(&mut e, &p, 1.0);
        assert!((798..=800).contains(&inner), "{inner}");
        push(&mut e, &p, 1.0, 0.0);
        let (outer, _) = travel(&mut e, &p, 1.0);
        assert!((1498..=1500).contains(&outer), "{outer}");
    }

    #[test]
    fn smoothing_eases_in_and_settles_after_release() {
        let p = mouse_profile(MouseResponse { smoothing_ms: 100, ..MouseResponse::default() });
        let mut e = Engine::default();
        push(&mut e, &p, 1.0, 0.0);
        let (first, _) = travel(&mut e, &p, 0.1);
        assert!(first < 100, "smoothed start moved {first}px, unsmoothed would be 100");

        push(&mut e, &p, 0.0, 0.0);
        assert!(e.needs_tick(&p), "still settling after release");
        let (tail, _) = travel(&mut e, &p, 2.0);
        assert!(tail > 0, "keeps moving briefly after release");
        assert!(!e.needs_tick(&p), "settled");
    }

    #[test]
    fn response_fields_are_optional_in_config() {
        let old: StickConfig = toml::from_str("deadzone = 0.1\ncurve = 2.0\n[action.mouse]\nspeed = 900.0\n").unwrap();
        assert_eq!(old.action, StickAction::mouse(900.0));

        let tuned = StickConfig::new(
            StickAction::Mouse { speed: 900.0, response: MouseResponse { accel: 0.4, y_scale: 0.8, ..MouseResponse::default() } },
            0.1,
            2.0,
        );
        let text = toml::to_string(&tuned).unwrap();
        assert!(!text.contains("outer_boost") && !text.contains("smoothing_ms"), "{text}");
        assert_eq!(toml::from_str::<StickConfig>(&text).unwrap(), tuned);
        assert!(!toml::to_string(&old).unwrap().contains("accel"));
    }

    fn ring_profile() -> Profile {
        let mut p = Profile::passthrough("p");
        let key = |k: &str| ButtonAction::Keys(vec![k.into()]);
        p.left_stick = StickConfig::new(
            StickAction::Ring {
                sectors: 4,
                start_angle: 0.0,
                inner_radius: 0.5,
                hysteresis: 0.1,
                actions: vec![key("KEY_W"), key("KEY_D"), key("KEY_S"), key("KEY_A")],
            },
            0.0,
            1.0,
        );
        p
    }

    fn left(e: &mut Engine, p: &Profile, x: f32, y: f32) -> Vec<OutEvent> {
        let mut out = Vec::new();
        e.handle(p, InputEvent::Axis(Axis::LeftX, x), Instant::now(), &mut out);
        e.handle(p, InputEvent::Axis(Axis::LeftY, y), Instant::now(), &mut out);
        out.retain(|o| matches!(o, OutEvent::Key(..)));
        out
    }

    #[test]
    fn ring_holds_the_sector_the_stick_points_into() {
        let p = ring_profile();
        let mut e = Engine::default();
        // Up is negative y.
        assert_eq!(left(&mut e, &p, 0.0, -1.0), [OutEvent::Key(KeyCode::KEY_W, true)]);
        assert_eq!(
            left(&mut e, &p, 1.0, 0.0),
            [OutEvent::Key(KeyCode::KEY_W, false), OutEvent::Key(KeyCode::KEY_D, true)]
        );
        assert_eq!(left(&mut e, &p, 0.0, 1.0).len(), 2);
        assert_eq!(left(&mut e, &p, 0.0, 0.0), [OutEvent::Key(KeyCode::KEY_S, false)]);
    }

    #[test]
    fn ring_needs_the_inner_radius_and_keeps_a_sector_a_little_longer() {
        let p = ring_profile();
        let mut e = Engine::default();
        assert!(left(&mut e, &p, 0.0, -0.4).is_empty(), "inside the inner radius");
        assert_eq!(left(&mut e, &p, 0.0, -0.6), [OutEvent::Key(KeyCode::KEY_W, true)]);
        assert!(left(&mut e, &p, 0.0, -0.48).is_empty(), "stays on just below the radius");
        assert_eq!(left(&mut e, &p, 0.0, -0.3), [OutEvent::Key(KeyCode::KEY_W, false)]);

        // 50 degrees right of up is past the 45 degree edge but within the 10% stickiness (54).
        let (x, y) = (50f32.to_radians().sin(), -50f32.to_radians().cos());
        left(&mut e, &p, 0.0, -1.0);
        assert!(left(&mut e, &p, x, y).is_empty(), "still up");
        let (x, y) = (60f32.to_radians().sin(), -60f32.to_radians().cos());
        assert_eq!(left(&mut e, &p, x, y).len(), 2, "moved to right");
    }

    #[test]
    fn ring_start_angle_turns_the_sectors() {
        let mut p = ring_profile();
        if let StickAction::Ring { start_angle, .. } = &mut p.left_stick.action {
            *start_angle = 45.0;
        }
        let mut e = Engine::default();
        // 80 degrees right of up is the second sector from up, but the first once the sectors
        // start at 45 degrees. Y is set first, so no half-way position lands in another sector.
        let (x, y) = (80f32.to_radians().sin(), -80f32.to_radians().cos());
        left(&mut e, &p, 0.0, y);
        assert_eq!(left(&mut e, &p, x, y), [OutEvent::Key(KeyCode::KEY_W, true)]);
        let mut e = Engine::default();
        let p = ring_profile();
        left(&mut e, &p, 0.0, y);
        assert_eq!(left(&mut e, &p, x, y), [OutEvent::Key(KeyCode::KEY_D, true)]);
    }

    #[test]
    fn ring_actions_are_released_when_the_mode_changes_and_follow_toggles() {
        let mut p = ring_profile();
        let mut e = Engine::default();
        left(&mut e, &p, 0.0, -1.0);
        p.left_stick.action = StickAction::Disabled;
        assert_eq!(left(&mut e, &p, 0.0, -1.0), [OutEvent::Key(KeyCode::KEY_W, false)]);

        // A toggle-wrapped sector action stays on after the stick leaves.
        let mut p = ring_profile();
        if let StickAction::Ring { actions, .. } = &mut p.left_stick.action {
            actions[0] = ButtonAction::toggle(ButtonAction::Keys(vec!["KEY_W".into()]));
        }
        let mut e = Engine::default();
        assert_eq!(left(&mut e, &p, 0.0, -1.0), [OutEvent::Key(KeyCode::KEY_W, true)]);
        assert!(left(&mut e, &p, 0.0, 0.0).is_empty(), "toggle holds it");
    }

    #[test]
    fn ring_config_roundtrips_and_fills_defaults() {
        let ring: StickConfig = toml::from_str("deadzone = 0.1\n[action.ring]\nsectors = 4\n").unwrap();
        assert!(matches!(ring.action, StickAction::Ring { sectors: 4, inner_radius, .. } if (inner_radius - 0.5).abs() < 1e-6));
        let full = StickConfig::new(StickAction::ring(8), 0.1, 2.0);
        assert_eq!(toml::from_str::<StickConfig>(&toml::to_string(&full).unwrap()).unwrap(), full);
    }

    #[test]
    fn ring_actions_are_part_of_the_profile_actions() {
        let mut p = ring_profile();
        assert!(p.actions().contains(&&ButtonAction::Keys(vec!["KEY_D".into()])));
        for a in p.actions_mut() {
            if *a == ButtonAction::Keys(vec!["KEY_D".into()]) {
                *a = ButtonAction::Keys(vec!["KEY_E".into()]);
            }
        }
        assert_eq!(p.left_stick.action.ring_actions()[1], ButtonAction::Keys(vec!["KEY_E".into()]));
    }
}

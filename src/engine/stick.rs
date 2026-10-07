//! Stick processing: virtual-pad feeds, direction keys and zones, and the continuous mouse and
//! scroll output driven by `tick`.

use super::*;

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
        self.zones(profile, Analog::Stick(s), x.hypot(y).min(1.0), out)
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

    /// Whether any stick has continuous output to produce: a mouse or scroll stick is deflected.
    pub(super) fn sticks_need_tick(&self, profile: &Profile) -> bool {
        [Stick::Left, Stick::Right].into_iter().any(|s| {
            let cfg = profile.stick(s);
            matches!(cfg.action, StickAction::Mouse { .. } | StickAction::Scroll { .. })
                && self.stick_pos(s, cfg.deadzone) != (0.0, 0.0)
        })
    }

    /// Advances the sticks' continuous outputs (mouse motion, scrolling) by `dt` seconds.
    pub(super) fn tick_sticks(&mut self, profile: &Profile, dt: f32, out: &mut Vec<OutEvent>) {
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
    }
}

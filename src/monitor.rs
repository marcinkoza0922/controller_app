//! Live one-line view of gamepad input, shown when the daemon runs in a terminal.

use std::{
    collections::{BTreeSet, HashMap},
    fmt::{self, Write as _},
    io::{IsTerminal, Write},
    sync::{Mutex, OnceLock},
};

use evdev::KeyCode;

use crate::{
    config::{Button, MouseButton},
    input::{Axis, InputEvent},
    ipc::InputSnapshot,
    output::OutEvent,
};

/// Smoothing for the displayed pointer/scroll speed (per 4 ms tick).
const SPEED_SMOOTHING: f32 = 0.1;

/// Last status line drawn, so log messages can be printed above it and the line redrawn.
static LINE: Mutex<Option<String>> = Mutex::new(None);

fn is_terminal() -> bool {
    static TTY: OnceLock<bool> = OnceLock::new();
    *TTY.get_or_init(|| std::io::stderr().is_terminal())
}

/// Logs a message to stderr without garbling the live status line.
macro_rules! log {
    ($($arg:tt)*) => { $crate::monitor::log_line(format_args!($($arg)*)) };
}
pub(crate) use log;

pub fn log_line(msg: fmt::Arguments) {
    let line = LINE.lock().unwrap();
    let mut err = std::io::stderr().lock();
    match line.as_deref() {
        Some(status) => {
            let _ = write!(err, "\r\x1b[2K{msg}\n{status}");
        }
        None => {
            let _ = writeln!(err, "{msg}");
        }
    }
    let _ = err.flush();
}

/// Replaces the status line in place. No-op unless stderr is a terminal.
pub fn show(status: &str) {
    if !is_terminal() {
        return;
    }
    let status = truncate(status, terminal_width());
    let mut err = std::io::stderr().lock();
    let _ = write!(err, "\r\x1b[2K{status}");
    let _ = err.flush();
    *LINE.lock().unwrap() = Some(status);
}

/// Removes the status line, e.g. when its device disconnects.
pub fn clear() {
    let mut line = LINE.lock().unwrap();
    if line.take().is_some() {
        let _ = write!(std::io::stderr(), "\r\x1b[2K");
    }
}

pub fn enabled() -> bool {
    is_terminal()
}

/// Formats a signed reading for live display. Values that round to zero print as an
/// unsigned zero, so sensor noise around rest (-0.3, +0.2) doesn't flicker between "-0" and
/// "+0"; the fixed width keeps the line from jumping as digits come and go.
pub fn signed(v: f32, decimals: usize) -> String {
    let scale = 10f32.powi(decimals as i32);
    let rounded = (v * scale).round() / scale;
    let width = if decimals == 0 { 4 } else { decimals + 3 };
    if rounded == 0.0 {
        format!("{:>width$.decimals$}", 0.0)
    } else {
        format!("{rounded:>+width$.decimals$}")
    }
}

/// A wrapped line breaks the `\r` overwrite, so cut it to the terminal width.
fn truncate(s: &str, width: usize) -> String {
    s.chars().take(width.saturating_sub(1)).collect()
}

fn terminal_width() -> usize {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: TIOCGWINSZ writes a winsize into the pointed-to struct.
    let ok = unsafe { libc::ioctl(libc::STDERR_FILENO, libc::TIOCGWINSZ, &mut ws) } == 0;
    if ok && ws.ws_col > 0 { ws.ws_col as usize } else { 120 }
}

/// Current input state of one physical device.
#[derive(Default)]
pub struct InputView {
    buttons: BTreeSet<Button>,
    axes: HashMap<Axis, f32>,
    gyro: Option<[f32; 3]>,
}

impl InputView {
    pub fn apply(&mut self, ev: &InputEvent) {
        match *ev {
            InputEvent::Button(b, true) => {
                self.buttons.insert(b);
            }
            InputEvent::Button(b, false) => {
                self.buttons.remove(&b);
            }
            InputEvent::Axis(a, v) => {
                self.axes.insert(a, v);
            }
            InputEvent::Touchpad(_) => {}
        }
    }

    pub fn buttons(&self) -> impl Iterator<Item = Button> + '_ {
        self.buttons.iter().copied()
    }

    pub fn axes(&self) -> impl Iterator<Item = (Axis, f32)> + '_ {
        self.axes.iter().map(|(a, v)| (*a, *v))
    }

    pub fn set_gyro(&mut self, gyro: [f32; 3]) {
        self.gyro = Some(gyro);
    }

    pub fn snapshot(&self, device: &str) -> InputSnapshot {
        let a = |axis| self.axes.get(&axis).copied().unwrap_or(0.0);
        InputSnapshot {
            device: device.to_string(),
            model: None,
            family: None,
            buttons: self.buttons.iter().copied().collect(),
            left_stick: (a(Axis::LeftX), a(Axis::LeftY)),
            right_stick: (a(Axis::RightX), a(Axis::RightY)),
            left_trigger: a(Axis::LeftTrigger),
            right_trigger: a(Axis::RightTrigger),
            gyro: self.gyro,
        }
    }

    pub fn render(&self, device: &str) -> String {
        let a = |axis| self.axes.get(&axis).copied().unwrap_or(0.0);
        let mut s = format!(
            "{device} │ LS {} {}  RS {} {}  LT {:.2}  RT {:.2} │",
            signed(a(Axis::LeftX), 2),
            signed(a(Axis::LeftY), 2),
            signed(a(Axis::RightX), 2),
            signed(a(Axis::RightY), 2),
            a(Axis::LeftTrigger),
            a(Axis::RightTrigger),
        );
        for b in &self.buttons {
            let _ = write!(s, " {b:?}");
        }
        if let Some([pitch, yaw, roll]) = self.gyro {
            let _ = write!(s, " │ gyro p{} y{} r{}", signed(pitch, 0), signed(yaw, 0), signed(roll, 0));
        }
        s
    }
}

/// What one device's mapping is currently sending to the virtual devices.
#[derive(Default)]
pub struct OutputView {
    pad_buttons: BTreeSet<Button>,
    pad_axes: HashMap<Axis, f32>,
    /// Reference counted, like the virtual keyboard itself.
    keys: HashMap<KeyCode, u32>,
    mouse_buttons: HashMap<MouseButton, u32>,
    /// Smoothed pointer speed in px/s and scroll speed in notches/s.
    motion: (f32, f32),
    scroll: (f32, f32),
    tick_motion: (i32, i32),
    tick_scroll: (i32, i32),
}

impl OutputView {
    pub fn apply(&mut self, ev: &OutEvent) {
        match *ev {
            OutEvent::PadButton(b, true) => {
                self.pad_buttons.insert(b);
            }
            OutEvent::PadButton(b, false) => {
                self.pad_buttons.remove(&b);
            }
            OutEvent::PadAxis(a, v) => {
                self.pad_axes.insert(a, v);
            }
            OutEvent::Key(k, pressed) => refcount(&mut self.keys, k, pressed),
            OutEvent::MouseButton(b, pressed) => refcount(&mut self.mouse_buttons, b, pressed),
            OutEvent::MouseMove(dx, dy) => {
                self.tick_motion.0 += dx;
                self.tick_motion.1 += dy;
            }
            OutEvent::Wheel { vertical, horizontal } => {
                self.tick_scroll.0 += horizontal;
                self.tick_scroll.1 += vertical;
            }
        }
    }

    /// Folds motion emitted during a tick of `dt` seconds into the displayed speeds.
    pub fn end_tick(&mut self, dt: f32) {
        let smooth = |avg: &mut (f32, f32), (x, y): (i32, i32), scale: f32| {
            avg.0 += (x as f32 * scale / dt - avg.0) * SPEED_SMOOTHING;
            avg.1 += (y as f32 * scale / dt - avg.1) * SPEED_SMOOTHING;
        };
        smooth(&mut self.motion, std::mem::take(&mut self.tick_motion), 1.0);
        smooth(&mut self.scroll, std::mem::take(&mut self.tick_scroll), 1.0 / 120.0);
    }

    /// Clears speeds once continuous output has stopped (ticks no longer run then).
    pub fn stop_motion(&mut self) {
        self.motion = (0.0, 0.0);
        self.scroll = (0.0, 0.0);
    }

    /// Compact summary listing only active outputs.
    pub fn render(&self) -> String {
        let mut parts = Vec::new();

        let mut pad: Vec<String> = self.pad_buttons.iter().map(|b| format!("{b:?}")).collect();
        let a = |axis| self.pad_axes.get(&axis).copied().unwrap_or(0.0);
        for (name, x, y) in [
            ("LS", a(Axis::LeftX), a(Axis::LeftY)),
            ("RS", a(Axis::RightX), a(Axis::RightY)),
        ] {
            if x != 0.0 || y != 0.0 {
                pad.push(format!("{name} {} {}", signed(x, 2), signed(y, 2)));
            }
        }
        for (name, v) in [("LT", a(Axis::LeftTrigger)), ("RT", a(Axis::RightTrigger))] {
            if v > 0.0 {
                pad.push(format!("{name} {v:.2}"));
            }
        }
        if !pad.is_empty() {
            parts.push(format!("pad {}", pad.join(" ")));
        }

        let mut keys: Vec<_> = self.keys.keys().collect();
        keys.sort();
        if !keys.is_empty() {
            let names: Vec<String> = keys
                .iter()
                .map(|k| format!("{k:?}").trim_start_matches("KEY_").to_string())
                .collect();
            parts.push(format!("keys {}", names.join("+")));
        }

        let mut buttons: Vec<String> = self.mouse_buttons.keys().map(std::string::ToString::to_string).collect();
        buttons.sort();
        if !buttons.is_empty() {
            parts.push(format!("click {}", buttons.join(" ")));
        }
        // Hide speeds that round to nothing so the line settles once the stick is still.
        if self.motion.0.abs() >= 1.0 || self.motion.1.abs() >= 1.0 {
            parts.push(format!("move {} {} px/s", signed(self.motion.0, 0), signed(self.motion.1, 0)));
        }
        if self.scroll.0.abs() >= 0.05 || self.scroll.1.abs() >= 0.05 {
            parts.push(format!("scroll {} {}/s", signed(self.scroll.0, 1), signed(self.scroll.1, 1)));
        }

        if parts.is_empty() { "-".into() } else { parts.join(" · ") }
    }
}

fn refcount<K: std::hash::Hash + Eq>(map: &mut HashMap<K, u32>, k: K, pressed: bool) {
    if pressed {
        *map.entry(k).or_insert(0) += 1;
    } else if let Some(n) = map.get_mut(&k) {
        *n -= 1;
        if *n == 0 {
            map.remove(&k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_axes_and_held_buttons() {
        let mut v = InputView::default();
        v.apply(&InputEvent::Axis(Axis::LeftY, -1.0));
        v.apply(&InputEvent::Axis(Axis::RightTrigger, 0.5));
        v.apply(&InputEvent::Button(Button::South, true));
        v.apply(&InputEvent::Button(Button::DpadUp, true));
        v.apply(&InputEvent::Button(Button::South, false));
        assert_eq!(
            v.render("Pad"),
            "Pad │ LS  0.00 -1.00  RS  0.00  0.00  LT 0.00  RT 0.50 │ DpadUp"
        );
    }

    #[test]
    fn output_lists_only_active_parts() {
        let mut v = OutputView::default();
        assert_eq!(v.render(), "-");
        v.apply(&OutEvent::Key(KeyCode::KEY_LEFTCTRL, true));
        v.apply(&OutEvent::Key(KeyCode::KEY_C, true));
        v.apply(&OutEvent::MouseButton(MouseButton::Left, true));
        v.apply(&OutEvent::PadButton(Button::South, true));
        v.apply(&OutEvent::PadAxis(Axis::LeftY, -1.0));
        assert_eq!(v.render(), "pad South LS  0.00 -1.00 · keys LEFTCTRL+C · click Left");

        v.apply(&OutEvent::Key(KeyCode::KEY_C, false));
        v.apply(&OutEvent::MouseButton(MouseButton::Left, false));
        v.apply(&OutEvent::PadButton(Button::South, false));
        v.apply(&OutEvent::PadAxis(Axis::LeftY, 0.0));
        assert_eq!(v.render(), "keys LEFTCTRL");
    }

    #[test]
    fn motion_speed_converges_to_px_per_second() {
        let mut v = OutputView::default();
        for _ in 0..200 {
            v.apply(&OutEvent::MouseMove(4, 0));
            v.end_tick(0.004);
        }
        assert_eq!(v.render(), "move +1000    0 px/s");
        v.stop_motion();
        assert_eq!(v.render(), "-");
    }

    #[test]
    fn noise_around_zero_shows_an_unsigned_zero() {
        // A still gyro reads small noise of either sign; it must not flicker "-0"/"+0".
        for noise in [-0.4, -0.01, 0.0, -0.0, 0.2, 0.49] {
            assert_eq!(signed(noise, 0), "   0", "{noise}");
        }
        assert_eq!(signed(-0.004, 2), " 0.00");
        assert_eq!(signed(0.004, 2), " 0.00");
        // Real readings keep their sign, at a steady width.
        assert_eq!(signed(12.4, 0), " +12");
        assert_eq!(signed(-0.6, 0), "  -1");
        assert_eq!(signed(-1.0, 2), "-1.00");
        assert_eq!(signed(0.25, 2), "+0.25");
    }

    #[test]
    fn truncates_to_width() {
        assert_eq!(truncate("abcdef", 4), "abc");
    }
}

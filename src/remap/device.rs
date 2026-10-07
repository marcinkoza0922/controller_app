//! Finding keyboards and mice, and reading them while they are grabbed.

use std::{
    io::ErrorKind,
    os::fd::AsRawFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
};

use evdev::{Device, EventSummary, KeyCode, RelativeAxisCode as Rel, SynchronizationCode};

use super::RawEvent;
use crate::config::MouseButton;

const POLL_TIMEOUT_MS: i32 = 200;
/// The first code past the keyboard keys; mouse and gamepad buttons start here.
const BTN_MISC: u16 = 0x100;

/// What a device is, for remapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    pub keyboard: bool,
    pub mouse: bool,
}

/// Whether `dev` is a keyboard and/or a mouse. Gamepads are neither, whatever extra keys they report.
pub fn classify(dev: &Device) -> Kind {
    let keys = dev.supported_keys();
    let has = |k: KeyCode| keys.is_some_and(|set| set.contains(k));
    let rel = dev.supported_relative_axes();
    let moves = rel.is_some_and(|r| r.contains(Rel::REL_X) && r.contains(Rel::REL_Y));
    let gamepad = has(KeyCode::BTN_SOUTH);
    Kind { keyboard: !gamepad && has(KeyCode::KEY_A) && has(KeyCode::KEY_ENTER), mouse: !gamepad && moves && has(KeyCode::BTN_LEFT) }
}

fn mouse_button(code: KeyCode) -> Option<MouseButton> {
    Some(match code {
        KeyCode::BTN_LEFT => MouseButton::Left,
        KeyCode::BTN_RIGHT => MouseButton::Right,
        KeyCode::BTN_MIDDLE => MouseButton::Middle,
        KeyCode::BTN_SIDE => MouseButton::Back,
        KeyCode::BTN_EXTRA => MouseButton::Forward,
        _ => return None,
    })
}

/// Turns raw evdev events into [`RawEvent`]s: movement is summed up to each report, and the
/// wheel comes out in high-resolution units whichever kind of wheel events the mouse sends.
struct Translator {
    dx: i32,
    dy: i32,
    hi_res_wheel: bool,
}

impl Translator {
    fn event(&mut self, ev: evdev::InputEvent, out: &mut Vec<RawEvent>) {
        match ev.destructure() {
            // Value 2 is a repeat.
            EventSummary::Key(_, code, value @ (0 | 1)) => {
                let pressed = value == 1;
                match mouse_button(code) {
                    Some(button) => out.push(RawEvent::Button(button, pressed)),
                    None if code.0 < BTN_MISC => out.push(RawEvent::Key(code, pressed)),
                    None => {}
                }
            }
            EventSummary::RelativeAxis(_, Rel::REL_X, v) => self.dx += v,
            EventSummary::RelativeAxis(_, Rel::REL_Y, v) => self.dy += v,
            EventSummary::RelativeAxis(_, Rel::REL_WHEEL_HI_RES, v) => out.push(RawEvent::Wheel { vertical: v, horizontal: 0 }),
            EventSummary::RelativeAxis(_, Rel::REL_HWHEEL_HI_RES, v) => out.push(RawEvent::Wheel { vertical: 0, horizontal: v }),
            EventSummary::RelativeAxis(_, Rel::REL_WHEEL, v) if !self.hi_res_wheel => out.push(RawEvent::Wheel { vertical: v * 120, horizontal: 0 }),
            EventSummary::RelativeAxis(_, Rel::REL_HWHEEL, v) if !self.hi_res_wheel => out.push(RawEvent::Wheel { vertical: 0, horizontal: v * 120 }),
            EventSummary::Synchronization(_, SynchronizationCode::SYN_REPORT, _) if self.dx != 0 || self.dy != 0 => {
                out.push(RawEvent::Move(self.dx, self.dy));
                self.dx = 0;
                self.dy = 0;
            }
            _ => {}
        }
    }
}

/// Reads `dev` until `stop` is set or it disappears, sending its events as `make(events)`;
/// `gone` is sent once it ends. Dropping the device on the way out releases the grab.
pub fn read_device<M>(mut dev: Device, stop: &AtomicBool, tx: &Sender<M>, make: impl Fn(Vec<RawEvent>) -> M, gone: impl FnOnce() -> M) {
    let hi_res_wheel = dev.supported_relative_axes().is_some_and(|r| r.contains(Rel::REL_WHEEL_HI_RES));
    let mut translator = Translator { dx: 0, dy: 0, hi_res_wheel };
    if dev.set_nonblocking(true).is_err() {
        let _ = tx.send(gone());
        return;
    }
    let mut pfd = libc::pollfd { fd: dev.as_raw_fd(), events: libc::POLLIN, revents: 0 };
    let mut events = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        // SAFETY: pfd points to a single valid pollfd for the duration of the call.
        let n = unsafe { libc::poll(&mut pfd, 1, POLL_TIMEOUT_MS) };
        if n < 0 {
            if std::io::Error::last_os_error().kind() == ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if n == 0 {
            continue;
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            break;
        }
        match dev.fetch_events() {
            Ok(evs) => evs.for_each(|ev| translator.event(ev, &mut events)),
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(_) => break,
        }
        if !events.is_empty() && tx.send(make(std::mem::take(&mut events))).is_err() {
            return;
        }
    }
    let _ = tx.send(gone());
}

//! Controllers that no hardware backs, for `padwight debug --attach`: input comes in over IPC,
//! runs through the same mappings as a real controller's, and the output is recorded. Only a
//! daemon started with `--debug` accepts them, since what they trigger (screenshots, force
//! quit, the on-screen keyboard) is real.

use std::sync::{Arc, atomic::AtomicBool};

use super::*;
use crate::{
    input::MotionSample,
    ipc::{DebugEvent, DeviceInfo},
    output::OutEvent,
};

/// How many output lines an injected controller keeps; the oldest go first.
const RECORDED_MAX: usize = 5000;

const OFF: &str = "debug injection is off; start the daemon with `padwight daemon --debug`";

/// Adds `out` to the record, as the lines `describe` writes.
pub(super) fn record(recorded: &mut Vec<String>, out: &[OutEvent]) {
    recorded.extend(out.iter().map(describe));
    if recorded.len() > RECORDED_MAX {
        recorded.drain(..recorded.len() - RECORDED_MAX);
    }
}

/// One output event in words: `key KEY_A down`, `pad South up`, `pad-axis LeftX 0.50`,
/// `mouse Left down`, `mouse-move 3 -2`, `wheel 120 0` (vertical, horizontal; 120 is a notch).
fn describe(ev: &OutEvent) -> String {
    let state = |down: &bool| if *down { "down" } else { "up" };
    match ev {
        OutEvent::PadButton(b, down) => format!("pad {b:?} {}", state(down)),
        OutEvent::PadAxis(axis, v) => format!("pad-axis {axis:?} {v:.2}"),
        OutEvent::Key(k, down) => format!("key {k:?} {}", state(down)),
        OutEvent::MouseButton(b, down) => format!("mouse {b:?} {}", state(down)),
        OutEvent::MouseMove(dx, dy) => format!("mouse-move {dx} {dy}"),
        OutEvent::Wheel { vertical, horizontal } => format!("wheel {vertical} {horizontal}"),
    }
}

/// Whether the daemon takes injected controllers (it was started with `--debug`).
pub(super) struct Injection {
    enabled: bool,
    /// Numbers the controllers' device paths.
    next: u32,
}

impl Injection {
    /// Debug mode also keeps screenshots and recordings out of the user's own folders: they go
    /// to a private folder in the temp directory instead.
    pub(super) fn new(enabled: bool) -> Result<Self> {
        if enabled {
            // SAFETY: getuid cannot fail.
            let dir = std::env::temp_dir().join(format!("padwight-debug-{}", unsafe { libc::getuid() }));
            prepare_private_dir(&dir)?;
            log!("debug mode: screenshots and recordings go to {}", dir.display());
            crate::capture::redirect_to(dir);
        }
        Ok(Self { enabled, next: 0 })
    }
}

impl Daemon {
    pub(super) fn debug_request(&mut self, req: Request) -> Response {
        if !self.injection.enabled {
            return Response::Error(OFF.into());
        }
        match req {
            Request::DebugAttach { model, live } => self.debug_attach(model, live),
            Request::DebugInput { path, events } => self.debug_input(&path, events),
            Request::DebugOutput { path, clear } => self.debug_output(&path, clear),
            Request::DebugDetach(path) => self.debug_detach(&path),
            Request::DebugIdentify(identity) => self.debug_identify(identity),
            _ => Response::Error("not a debug request".into()),
        }
    }

    fn injected_id(&self, path: &str) -> Option<u64> {
        self.devices.iter().find(|(_, d)| d.recorded.is_some() && d.path.as_os_str() == path).map(|(id, _)| *id)
    }

    /// Forces every virtual pad to present as `identity`, or lets the game and controller decide.
    fn debug_identify(&mut self, identity: Option<crate::pad_identity::PadIdentity>) -> Response {
        log!("debug: identity forced to {identity:?}");
        self.identity_override = identity;
        self.sync_identities();
        Response::Ok
    }

    fn debug_attach(&mut self, model: Option<crate::info::PadModel>, live: bool) -> Response {
        let pad = if live {
            match VirtualPad::new(None) {
                Ok(pad) => Some(pad),
                Err(e) => return Response::Error(format!("cannot create the virtual pad: {e:#}")),
            }
        } else {
            None
        };
        let id = self.next_id;
        self.next_id += 1;
        let path = format!("debug:{}", self.injection.next);
        self.injection.next += 1;
        let name = format!("Debug controller ({})", model.map_or("generic", crate::info::PadModel::slug));
        log!("injected controller {name} ({path}){}", if live { ", live" } else { "" });
        let mut managed = self.new_managed(path.clone().into(), name, pad, Arc::new(AtomicBool::new(false)));
        managed.family = model.map(crate::info::PadModel::family);
        managed.model = model;
        managed.recorded = Some(Vec::new());
        self.add_managed(id, managed);
        // What it output while settling into the active profile isn't what a test asks about.
        if let Some(recorded) = self.devices.get_mut(&id).and_then(|d| d.recorded.as_mut()) {
            recorded.clear();
        }
        Response::Attached(path)
    }

    fn debug_input(&mut self, path: &str, events: Vec<DebugEvent>) -> Response {
        let Some(id) = self.injected_id(path) else { return Response::Error(format!("no injected controller {path}")) };
        let mut batch = Vec::new();
        for ev in events {
            match ev {
                DebugEvent::Button(b, down) => batch.push(InputEvent::Button(b, down)),
                DebugEvent::Axis(axis, v) => batch.push(InputEvent::Axis(axis, v)),
                DebugEvent::Motion { gyro, ms } => {
                    // Motion goes in order with the buttons around it.
                    if !batch.is_empty() {
                        self.input(id, std::mem::take(&mut batch));
                    }
                    self.motion(id, MotionSample { gyro, accel: [0.0; 3], dt: ms as f32 / 1000.0 });
                }
            }
        }
        if !batch.is_empty() {
            self.input(id, batch);
        }
        Response::Ok
    }

    fn debug_output(&mut self, path: &str, clear: bool) -> Response {
        let Some(id) = self.injected_id(path) else { return Response::Error(format!("no injected controller {path}")) };
        let recorded = self.devices.get_mut(&id).and_then(|d| d.recorded.as_mut());
        Response::Lines(recorded.map(|r| if clear { std::mem::take(r) } else { r.clone() }).unwrap_or_default())
    }

    fn debug_detach(&mut self, path: &str) -> Response {
        let ids: Vec<u64> = if path == "all" {
            self.devices.iter().filter(|(_, d)| d.recorded.is_some()).map(|(id, _)| *id).collect()
        } else {
            self.injected_id(path).into_iter().collect()
        };
        if ids.is_empty() && path != "all" {
            return Response::Error(format!("no injected controller {path}"));
        }
        for id in ids {
            self.device_gone(id);
        }
        Response::Ok
    }

    /// Injected controllers, for the status listing.
    pub(super) fn injected_devices(&self) -> Vec<DeviceInfo> {
        let mut list: Vec<DeviceInfo> = self
            .devices
            .values()
            .filter(|d| d.recorded.is_some())
            .map(|d| DeviceInfo {
                name: d.name.clone(),
                path: d.path.display().to_string(),
                managed: true,
                ignored: false,
                analog_triggers: true,
                rumble: false,
                gyro: true,
                paddles: false,
                family: d.family,
                model: d.model,
            })
            .collect();
        list.sort_by(|a, b| a.path.cmp(&b.path));
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Button;

    #[test]
    fn output_is_described_in_words() {
        let out = [
            OutEvent::Key(evdev::KeyCode::KEY_A, true),
            OutEvent::PadButton(Button::South, false),
            OutEvent::PadAxis(crate::input::Axis::LeftX, 0.5),
            OutEvent::MouseMove(3, -2),
        ];
        let mut lines = Vec::new();
        record(&mut lines, &out);
        assert_eq!(lines, ["key KEY_A down", "pad South up", "pad-axis LeftX 0.50", "mouse-move 3 -2"]);
    }

    #[test]
    fn the_record_keeps_only_the_newest() {
        let mut lines = Vec::new();
        for _ in 0..RECORDED_MAX + 10 {
            record(&mut lines, &[OutEvent::MouseMove(1, 1)]);
        }
        record(&mut lines, &[OutEvent::MouseMove(9, 9)]);
        assert_eq!(lines.len(), RECORDED_MAX);
        assert_eq!(lines.last().map(String::as_str), Some("mouse-move 9 9"));
    }
}

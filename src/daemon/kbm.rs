//! Keyboard and mouse remapping in the daemon: grabbing the devices while a keyboard profile
//! is active, and feeding what they do through it.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::Instant,
};

use evdev::Device;

use super::{Daemon, Msg, NodeKey};
use crate::{
    monitor::log,
    output::{OutEvent, VirtualKbm},
    remap::{self, RawEvent},
};

/// A keyboard or mouse we could remap.
pub struct Candidate {
    pub path: PathBuf,
    pub name: String,
}

/// A keyboard or mouse being read, with the grab on it.
pub struct Grabbed {
    path: PathBuf,
    name: String,
    stop: Arc<AtomicBool>,
}

/// Sends the keyboard and mouse part of `out` to the virtual devices.
pub fn dispatch_kbm(kbm: &mut VirtualKbm, out: Vec<OutEvent>) {
    for ev in out {
        let res = match ev {
            OutEvent::Key(k, pressed) => kbm.key(k, pressed),
            OutEvent::MouseButton(b, pressed) => kbm.mouse_button(b, pressed),
            OutEvent::MouseMove(dx, dy) => kbm.mouse_move(dx, dy),
            OutEvent::Wheel { vertical, horizontal } => kbm.wheel(vertical, horizontal),
            OutEvent::PadButton(..) | OutEvent::PadAxis(..) => Ok(()),
        };
        if let Err(e) = res {
            log!("output error: {e:#}");
        }
    }
}

impl Daemon {
    /// Files a device that isn't a gamepad: a keyboard or mouse we could remap, or one to ignore.
    pub(super) fn note_other_device(&mut self, key: NodeKey, path: PathBuf, name: String, dev: &Device) {
        let kind = remap::classify(dev);
        if kind.keyboard || kind.mouse {
            self.remappable.insert(key, Candidate { path, name });
        } else {
            self.skipped.insert(key);
        }
    }

    /// Grabs the keyboards and mice while a keyboard profile is active, and lets them go
    /// otherwise. Safe to call whenever anything it depends on may have changed.
    pub(super) fn sync_remap(&mut self) {
        if !self.config.enabled || self.config.active_keyboard_profile().is_none() {
            self.release_remap();
            return;
        }
        let todo: Vec<(PathBuf, String)> = self
            .remappable
            .values()
            .filter(|c| !self.grabbed.values().any(|g| g.path == c.path))
            .map(|c| (c.path.clone(), c.name.clone()))
            .collect();
        for (path, name) in todo {
            self.grab_remappable(path, name);
        }
    }

    fn grab_remappable(&mut self, path: PathBuf, name: String) {
        let Ok(mut dev) = Device::open(&path) else { return };
        // A key held down now would stay down for the desktop once the grab hides its release.
        // Try again at the next scan.
        if dev.get_key_state().is_ok_and(|keys| keys.iter().next().is_some()) {
            return;
        }
        if let Err(e) = dev.grab() {
            log!("cannot grab {name} ({}): {e}", path.display());
            return;
        }
        log!("remapping {name} ({})", path.display());
        let id = self.next_id;
        self.next_id += 1;
        let stop = Arc::new(AtomicBool::new(false));
        let (flag, tx) = (stop.clone(), self.tx.clone());
        thread::spawn(move || remap::read_device(dev, &flag, &tx, move |events| Msg::Remap { id, events }, move || Msg::RemapGone { id }));
        self.grabbed.insert(id, Grabbed { path, name, stop });
    }

    /// Lets go of every grabbed device and of everything the remapping holds down.
    pub(super) fn release_remap(&mut self) {
        for (_, g) in self.grabbed.drain() {
            g.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            log!("released {} ({})", g.name, g.path.display());
        }
        self.reset_remap();
    }

    /// Lets go of what the remapping holds down, when the profile changes.
    pub(super) fn reset_remap(&mut self) {
        let mut out = Vec::new();
        self.remapper.release_all(&mut out);
        dispatch_kbm(&mut self.kbm, out);
    }

    pub(super) fn remap_input(&mut self, id: u64, events: Vec<RawEvent>) {
        if !self.grabbed.contains_key(&id) {
            return;
        }
        let Some(profile) = self.config.active_keyboard_profile() else { return };
        let mut out = Vec::new();
        let panic = events.into_iter().any(|ev| self.remapper.event(&profile.keyboard, ev, &mut out));
        dispatch_kbm(&mut self.kbm, out);
        if panic {
            self.panic_off();
        }
    }

    pub(super) fn remap_gone(&mut self, id: u64) {
        if let Some(g) = self.grabbed.remove(&id) {
            log!("device gone: {} ({})", g.name, g.path.display());
        }
    }

    /// The panic chord was pressed: turn all remapping off, like the header switch does.
    fn panic_off(&mut self) {
        log!("panic chord pressed: remapping off");
        self.config.enabled = false;
        self.release_devices(|_| true);
        self.release_remap();
        let lines = vec!["Remapping turned off (panic chord)".to_string(), "Turn it back on in the settings window.".to_string()];
        self.toast = Some(crate::info::Toast::new(lines, Instant::now()));
        self.broadcast_overlay();
        if let Err(e) = self.config.save() {
            log!("saving config: {e:#}");
        }
    }
}

//! Keyboard and mouse remapping in the daemon: grabbing the devices while a keyboard profile
//! is active, and feeding what they do through it.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::Instant,
};

use evdev::Device;

use super::{Daemon, Msg, NodeKey};
use crate::{
    config::{Config, Profile},
    monitor::log,
    output::{OutEvent, VirtualKbm, VirtualPad},
    remap::{self, RawEvent, Remapper},
};

/// Keyboard and mouse remapping: the devices, the remapper, and what it outputs to.
#[derive(Default)]
pub struct Remap {
    /// Keyboards and mice we could remap, grabbed or not.
    pub candidates: HashMap<NodeKey, Candidate>,
    /// The ones grabbed now, while a keyboard profile is active.
    pub grabbed: HashMap<u64, Grabbed>,
    pub remapper: Remapper,
    /// The virtual gamepad a keyboard profile outputs to, while it has any such output.
    pub pad: Option<VirtualPad>,
    /// The keyboard profile with the keyboard's layers on top, while it has any.
    pub layered: Option<(Vec<String>, Profile)>,
}

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

/// The keyboard profile as it applies now: with the layers the keyboard has on.
fn effective<'a>(config: &'a Config, layered: &'a Option<(Vec<String>, Profile)>) -> Option<&'a Profile> {
    let base = config.active_keyboard_profile()?;
    Some(layered.as_ref().map_or(base, |(_, p)| p))
}

/// Brings `cached` up to date with the layers on. True if they changed.
fn refresh_layers(config: &Config, layers: Vec<String>, cached: &mut Option<(Vec<String>, Profile)>) -> bool {
    let current = cached.as_ref().map(|(names, _)| names.as_slice()).unwrap_or_default();
    if layers == current {
        return false;
    }
    *cached = match config.active_keyboard_profile() {
        Some(base) if !layers.is_empty() => {
            let with = base.with_layers(config.active_game().layers_named(&layers));
            Some((layers, with))
        }
        _ => None,
    };
    true
}

/// Sends what a keyboard profile outputs to the virtual devices; gamepad output is dropped
/// without a pad.
pub fn dispatch_kbm(kbm: &mut VirtualKbm, mut pad: Option<&mut VirtualPad>, out: Vec<OutEvent>) {
    for ev in out {
        let res = match ev {
            OutEvent::Key(k, pressed) => kbm.key(k, pressed),
            OutEvent::MouseButton(b, pressed) => kbm.mouse_button(b, pressed),
            OutEvent::MouseMove(dx, dy) => kbm.mouse_move(dx, dy),
            OutEvent::Wheel { vertical, horizontal } => kbm.wheel(vertical, horizontal),
            OutEvent::PadButton(b, pressed) => pad.as_deref_mut().map_or(Ok(()), |p| p.button(b, pressed)),
            OutEvent::PadAxis(axis, v) => pad.as_deref_mut().map_or(Ok(()), |p| p.axis(axis, v)),
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
            self.remap.candidates.insert(key, Candidate { path, name });
        } else {
            self.skipped.insert(key);
        }
    }

    /// Grabs the keyboards and mice while a keyboard profile is active, and lets them go
    /// otherwise. Safe to call whenever anything it depends on may have changed.
    pub(super) fn sync_remap(&mut self) {
        let Some(needs_pad) = self.config.active_keyboard_profile().map(|p| p.keyboard.uses_pad()).filter(|_| self.config.enabled) else {
            self.release_remap();
            return;
        };
        self.sync_remap_pad(needs_pad);
        let todo: Vec<(PathBuf, String)> = self.remap.candidates
            .values()
            .filter(|c| !self.remap.grabbed.values().any(|g| g.path == c.path))
            .map(|c| (c.path.clone(), c.name.clone()))
            .collect();
        for (path, name) in todo {
            self.grab_remappable(path, name);
        }
    }

    /// Keeps a virtual gamepad only while the keyboard profile outputs to one.
    fn sync_remap_pad(&mut self, needed: bool) {
        if !needed {
            self.reset_remap();
            self.remap.pad = None;
        } else if self.remap.pad.is_none() {
            match VirtualPad::new(None) {
                Ok(pad) => self.remap.pad = Some(pad),
                Err(e) => log!("cannot create virtual pad for the keyboard profile: {e:#}"),
            }
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
        self.remap.grabbed.insert(id, Grabbed { path, name, stop });
    }

    /// Lets go of every grabbed device and of everything the remapping holds down.
    pub(super) fn release_remap(&mut self) {
        for (_, g) in self.remap.grabbed.drain() {
            g.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            log!("released {} ({})", g.name, g.path.display());
        }
        self.reset_remap();
        self.remap.pad = None;
    }

    /// Lets go of what the remapping holds down, when the profile changes.
    pub(super) fn reset_remap(&mut self) {
        let mut out = Vec::new();
        self.remap.remapper.release_all(&mut out);
        self.remap.layered = None;
        dispatch_kbm(&mut self.kbm, self.remap.pad.as_mut(), out);
    }

    pub(super) fn remap_input(&mut self, id: u64, events: Vec<RawEvent>) {
        if !self.remap.grabbed.contains_key(&id) {
            return;
        }
        let mut out = Vec::new();
        let mut panic = false;
        for ev in events {
            let Some(profile) = effective(&self.config, &self.remap.layered) else { break };
            if self.remap.remapper.event(profile, ev, &mut out) {
                panic = true;
                break;
            }
            self.layers_changed(&mut out);
        }
        dispatch_kbm(&mut self.kbm, self.remap.pad.as_mut(), out);
        if panic {
            self.panic_off();
        } else {
            self.after_remap();
        }
    }

    /// A layer started or ended: the next events use it, and the mouse's push switches mode.
    fn layers_changed(&mut self, out: &mut Vec<OutEvent>) {
        let layers = self.remap.remapper.layers();
        if !refresh_layers(&self.config, layers, &mut self.remap.layered) {
            return;
        }
        if let Some(profile) = effective(&self.config, &self.remap.layered) {
            self.remap.remapper.resync(profile, out);
        }
    }

    /// What the keyboard profile's actions asked for: the next profile, overlays shown or
    /// layers changed.
    fn after_remap(&mut self) {
        if self.remap.remapper.take_switch()
            && let Some(next) = self.config.next_profile()
        {
            self.switch_profile(next);
        }
        self.check_info_changes();
    }

    /// Lets gesture and combo timers that have run out act.
    pub(super) fn remap_timers(&mut self, now: Instant) {
        if self.remap.remapper.next_deadline().is_none_or(|d| d > now) {
            return;
        }
        let Some(profile) = effective(&self.config, &self.remap.layered) else { return };
        let mut out = Vec::new();
        self.remap.remapper.timers(profile, now, &mut out);
        self.layers_changed(&mut out);
        dispatch_kbm(&mut self.kbm, self.remap.pad.as_mut(), out);
        self.after_remap();
    }

    pub(super) fn remap_deadline(&self) -> Option<Instant> {
        self.remap.remapper.next_deadline().filter(|_| !self.remap.grabbed.is_empty())
    }

    /// Whether the keyboard profile has something to advance (a fading mouse push, a repeat).
    pub(super) fn remap_needs_tick(&self) -> bool {
        effective(&self.config, &self.remap.layered).is_some_and(|p| !self.remap.grabbed.is_empty() && self.remap.remapper.needs_tick(p))
    }

    pub(super) fn tick_remap(&mut self, dt: f32) {
        let Some(profile) = effective(&self.config, &self.remap.layered) else { return };
        let mut out = Vec::new();
        self.remap.remapper.tick(profile, dt, &mut out);
        dispatch_kbm(&mut self.kbm, self.remap.pad.as_mut(), out);
    }

    /// The items the keyboard profile's actions can name changed (another game, or an edit).
    pub(super) fn refresh_remap_scope(&mut self) {
        self.remap.remapper.set_macros(&self.config.scope().macros);
    }

    /// The game starts: switches on the keyboard profile's toggles set to start on.
    pub(super) fn remap_game_started(&mut self) {
        let Some(profile) = effective(&self.config, &self.remap.layered) else { return };
        let mut out = Vec::new();
        self.remap.remapper.start_toggles(profile, &self.scope.menus, &mut out);
        self.layers_changed(&mut out);
        dispatch_kbm(&mut self.kbm, self.remap.pad.as_mut(), out);
    }

    pub(super) fn remap_gone(&mut self, id: u64) {
        if let Some(g) = self.remap.grabbed.remove(&id) {
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

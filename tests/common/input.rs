//! Finding the daemon's virtual devices, and reading the keys and buttons the kernel reports on them.

use std::{
    collections::BTreeSet,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use evdev::{Device, EventType, InputEvent};

use super::wait_for;

pub fn find_node(name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir("/dev/input").ok()?.flatten() {
        let path = entry.path();
        if let Ok(dev) = Device::open(&path)
            && dev.name() == Some(name)
        {
            return Some(path);
        }
    }
    None
}

/// True when another process holds the controller's grab.
pub fn grabbed(path: &Path) -> bool {
    let mut dev = Device::open(path).unwrap();
    match dev.grab() {
        Ok(()) => {
            let _ = dev.ungrab();
            false
        }
        Err(e) => e.raw_os_error() == Some(libc::EBUSY),
    }
}

/// Reads the key and button events the kernel reports on one of the daemon's virtual devices, and
/// keeps the set it shows as pressed. The device is kept open, so its release events are seen
/// even after the daemon has gone.
pub struct Reader {
    pub dev: Device,
    pub pressed: BTreeSet<u16>,
}

impl Reader {
    pub fn open(name: &str) -> Self {
        let path = wait_for(&format!("the virtual device {name:?}"), || find_node(name));
        let dev = Device::open(&path).unwrap();
        dev.set_nonblocking(true).unwrap();
        Reader { dev, pressed: BTreeSet::new() }
    }

    /// Reads what is queued. Returns true once the device has gone (its read fails with ENODEV).
    pub fn drain(&mut self) -> bool {
        loop {
            let events: Vec<InputEvent> = match self.dev.fetch_events() {
                Ok(events) => events.collect(),
                Err(e) if e.kind() == ErrorKind::WouldBlock => return false,
                Err(e) if e.raw_os_error() == Some(libc::ENODEV) => return true,
                Err(e) => panic!("reading a virtual device: {e}"),
            };
            if events.is_empty() {
                return false;
            }
            for ev in events.iter().filter(|ev| ev.event_type() == EventType::KEY) {
                match ev.value() {
                    1 => {
                        self.pressed.insert(ev.code());
                    }
                    0 => {
                        self.pressed.remove(&ev.code());
                    }
                    _ => {}
                }
            }
        }
    }
}

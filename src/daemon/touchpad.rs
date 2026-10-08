//! Touchpads of managed controllers. A controller's touchpad is its own input node: it is taken
//! from the desktop together with the controller, and read on its own thread, so its click and
//! finger movement reach the engine like the controller's buttons.

use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, mpsc::Sender},
    thread,
};

use evdev::Device;

use crate::{input::TouchNormalizer, monitor::log};

use super::{Daemon, Msg, PairedNode, read_input};

impl Daemon {
    /// Pairs managed controllers with their touchpads, and grabs and starts reading them.
    pub(super) fn attach_touchpad(&mut self) {
        let pairs: Vec<(u64, PathBuf, String)> = self
            .devices
            .iter()
            .filter(|(_, d)| d.touchpad.is_none())
            .filter_map(|(id, d)| {
                let taken = |n: &PairedNode| self.devices.values().any(|o| o.touchpad.as_ref() == Some(&n.path));
                let node = self.touch_nodes.values().find(|n| !taken(n) && n.belongs_to(d))?;
                Some((*id, node.path.clone(), node.name.clone()))
            })
            .collect();
        for (id, path, touch_name) in pairs {
            let Ok(mut touch) = Device::open(&path) else { continue };
            let Some(dev) = self.devices.get_mut(&id) else { continue };
            // Recorded even when the grab fails, so a touchpad we can't take isn't retried every scan.
            dev.touchpad = Some(path);
            if let Err(e) = touch.grab() {
                log!("touchpad: cannot take {touch_name} from the desktop: {e}");
                continue;
            }
            log!("touchpad: using {touch_name} for {}", dev.name);
            let (stop, tx) = (dev.stop.clone(), self.tx.clone());
            thread::spawn(move || read_touchpad(id, touch, &stop, &tx));
        }
    }
}

/// Reads a touchpad until its controller goes away or the node does, then tells the daemon.
fn read_touchpad(id: u64, mut dev: Device, stop: &AtomicBool, tx: &Sender<Msg>) {
    let mut norm = TouchNormalizer::new(&dev);
    read_input(id, &mut dev, stop, tx, |ev, out| norm.translate(ev, out));
    // Dropping `dev` closes the fd, which also releases the grab.
    let _ = tx.send(Msg::TouchpadGone { id });
}

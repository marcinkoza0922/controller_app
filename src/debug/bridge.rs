//! Connects the virtual controller to the daemon (`padwight debug --attach`): the daemon runs it
//! as a controller of its own, so mappings, layers, menus and overlays react to it.
//!
//! Changes are sent as they are made, on the thread that made them, and the daemon answers once
//! it has handled them. So a command returns only when its input has had its effect, and a test
//! can read the output right after.

use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use super::server::Session;
use crate::{
    info::PadModel,
    ipc::{self, DebugEvent, Request, Response},
    pad_identity::PadIdentity,
};

/// The controller the daemon runs for this session.
pub struct Bridge {
    /// The daemon's name for it.
    path: String,
    model: Option<PadModel>,
    live: bool,
}

/// Where a session stands with the daemon.
#[derive(Default)]
pub struct Link(Mutex<Option<Bridge>>);

impl Link {
    fn lock(&self) -> MutexGuard<'_, Option<Bridge>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One request to the daemon, with its failures in terms of the debug session.
fn ask(req: &Request) -> Result<Response> {
    ipc::request(req).context("the daemon refused or could not be reached")
}

fn attach_pad(model: Option<PadModel>, live: bool) -> Result<String> {
    match ask(&Request::DebugAttach { model, live })? {
        Response::Attached(path) => Ok(path),
        _ => bail!("unexpected reply from the daemon"),
    }
}

impl Session {
    /// Has the daemon run this pad as a controller, from now on. `live` also sends its output
    /// to the real virtual devices. The daemon must run with `--debug`.
    pub fn attach(&self, live: bool) -> Result<String> {
        let mut link = self.link.lock();
        let model = {
            let mut pad = self.pad();
            pad.track_events();
            pad.model
        };
        let path = attach_pad(model, live)?;
        *link = Some(Bridge { path: path.clone(), model, live });
        drop(link);
        // Whatever is already held starts out held.
        let rebuilt = self.pad().state_events();
        self.send(&path, rebuilt)?;
        Ok(path)
    }

    /// Removes the controller from the daemon, letting go of what it held.
    pub fn detach(&self) {
        if let Some(bridge) = self.link.lock().take() {
            let _ = ipc::request(&Request::DebugDetach(bridge.path));
        }
    }

    /// The daemon's name for the controller, while attached.
    pub fn attached(&self) -> Option<String> {
        self.link.lock().as_ref().map(|b| b.path.clone())
    }

    /// Sends the changes made to the pad since the last time. Changing the model replaces the
    /// controller at the daemon, keeping what is held.
    pub fn flush(&self) -> Result<()> {
        let mut link = self.link.lock();
        let Some(bridge) = link.as_mut() else { return Ok(()) };
        let (events, model) = {
            let mut pad = self.pad();
            (pad.take_events(), pad.model)
        };
        let events = if model == bridge.model {
            events
        } else {
            let _ = ipc::request(&Request::DebugDetach(bridge.path.clone()));
            bridge.path = attach_pad(model, bridge.live)?;
            bridge.model = model;
            self.pad().state_events()
        };
        self.send(&bridge.path, events)
    }

    fn send(&self, path: &str, events: Vec<DebugEvent>) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        ask(&Request::DebugInput { path: path.to_string(), events }).map(drop)
    }

    /// What the daemon's mappings have output for this controller (see `padwight debug help`).
    pub fn output(&self, clear: bool) -> Result<Vec<String>> {
        let Some(path) = self.attached() else { bail!("not attached to the daemon (start the session with --attach)") };
        match ask(&Request::DebugOutput { path, clear })? {
            Response::Lines(lines) => Ok(lines),
            _ => bail!("unexpected reply from the daemon"),
        }
    }

    /// Makes every virtual pad on the daemon present as `identity` (`None` lets it choose). This
    /// is daemon-wide, so it doesn't need the session to be attached. The daemon must run with
    /// `--debug`.
    pub fn identify(&self, identity: Option<PadIdentity>) -> Result<()> {
        match ask(&Request::DebugIdentify(identity))? {
            Response::Ok => Ok(()),
            Response::Error(e) => bail!(e),
            _ => bail!("unexpected reply from the daemon"),
        }
    }

    /// Turns the pad at `rates` (degrees per second) for `ms`, as a stream of motion readings
    /// like a real controller's, then lets it stop.
    pub fn turn(&self, rates: [f32; 3], ms: u64) -> Result<()> {
        const STEP: Duration = Duration::from_millis(4);
        let Some(path) = self.attached() else { return Ok(()) };
        self.pad().set_gyro(Some(rates));
        let (start, mut last) = (Instant::now(), Instant::now());
        let result = loop {
            std::thread::sleep(STEP);
            let now = Instant::now();
            let step = DebugEvent::Motion { gyro: rates, ms: now.duration_since(last).as_millis().max(1) as u32 };
            last = now;
            if let Err(e) = self.send(&path, vec![step]) {
                break Err(e);
            }
            if now.duration_since(start) >= Duration::from_millis(ms) {
                break Ok(());
            }
        };
        self.pad().set_gyro(None);
        result
    }
}

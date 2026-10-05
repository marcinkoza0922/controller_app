//! Background service: grabs physical gamepads, runs the mapping engine and drives virtual devices.

use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, ErrorKind, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::MetadataExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use evdev::Device;

use crate::{
    config::Config,
    engine::Engine,
    focus::{self, FocusEvent},
    input::{self, InputEvent, MotionFrame, MotionNormalizer, MotionSample, Normalizer},
    ipc::{self, DeviceInfo, FocusBackend, InputSnapshot, Request, Response, Status, WindowInfo},
    monitor::{self, InputView, OutputView, log},
    output::{FfCaps, OutEvent, VIRTUAL_PREFIX, VirtualKbm, VirtualPad},
    rumble,
};

const TICK: Duration = Duration::from_millis(4);
const SCAN_INTERVAL: Duration = Duration::from_secs(2);
const POLL_TIMEOUT_MS: i32 = 200;
/// Minimum gap between snapshots streamed to one watcher (~60 fps).
const WATCH_INTERVAL: Duration = Duration::from_millis(16);
/// Status line redraw rate while only continuous output (mouse/scroll) is changing.
const STATUS_REDRAW: Duration = Duration::from_millis(50);
/// How long "Calibrate gyro" samples a still controller.
const GYRO_CALIBRATION: Duration = Duration::from_secs(2);
/// How many recently focused windows the GUI can offer for new rules.
const RECENT_WINDOWS: usize = 8;

enum Msg {
    Input { id: u64, events: Vec<InputEvent> },
    Gone { id: u64 },
    Ipc { req: Request, reply: Sender<Response> },
    Watch(Sender<Option<InputSnapshot>>),
    Focus(FocusEvent),
    Motion { id: u64, sample: MotionSample },
    MotionGone { id: u64 },
}

struct Managed {
    path: PathBuf,
    name: String,
    engine: Engine,
    pad: VirtualPad,
    stop: Arc<AtomicBool>,
    view: InputView,
    out_view: OutputView,
    /// For pairing with the controller's motion-sensor device.
    parent: Option<PathBuf>,
    uniq: Option<String>,
    /// Motion-sensor device feeding this controller's gyro, once found.
    motion: Option<PathBuf>,
    motion_frame: MotionFrame,
    /// Drift per raw sensor axis (the controller's own frame), subtracted before remapping.
    gyro_bias: [f32; 3],
    calibrating: Option<Calibration>,
}

struct Calibration {
    started: Instant,
    sum: [f64; 3],
    samples: u32,
}

/// A controller's motion-sensor input device, waiting to be paired with its gamepad.
struct MotionNode {
    path: PathBuf,
    name: String,
    parent: Option<PathBuf>,
    uniq: Option<String>,
}

impl MotionNode {
    /// Same physical controller: same parent HID device or same unique ID (e.g. Bluetooth
    /// MAC). Only when the parent can't be compared does the driver's naming ("<pad> Motion
    /// Sensors", "<pad> IMU") count, since two identical controllers share a name.
    fn belongs_to(&self, pad: &Managed) -> bool {
        self.matches(&pad.name, &pad.parent, &pad.uniq)
    }

    fn matches(&self, name: &str, parent: &Option<PathBuf>, uniq: &Option<String>) -> bool {
        let same_parent = self.parent.is_some() && &self.parent == parent;
        let same_uniq = self.uniq.as_deref().is_some_and(|u| !u.is_empty()) && &self.uniq == uniq;
        let comparable = self.parent.is_some() && parent.is_some();
        let named_after = !comparable && self.name.len() > name.len() && self.name.starts_with(name);
        same_parent || same_uniq || named_after
    }
}

impl Managed {
    /// Terminal status line: physical input, then what the mapping outputs.
    fn draw_status(&self) {
        if monitor::enabled() {
            monitor::show(&format!("{} → {}", self.view.render(&self.name), self.out_view.render()));
        }
    }
}

struct SeenGamepad {
    name: String,
    analog_triggers: bool,
    rumble: bool,
}

/// A device node identity; the inode changes when a node is recreated for a new device.
type NodeKey = (PathBuf, u64);

struct Daemon {
    config: Config,
    kbm: VirtualKbm,
    devices: HashMap<u64, Managed>,
    next_id: u64,
    /// Nodes already checked that we never manage (not a gamepad, or a virtual device).
    skipped: HashSet<NodeKey>,
    /// Gamepads we have seen, managed or not, for status reporting.
    gamepads: HashMap<NodeKey, SeenGamepad>,
    motion_nodes: HashMap<NodeKey, MotionNode>,
    /// Controller motion sensors we lack permission to open (names), for the GUI to explain.
    motion_denied: HashMap<NodeKey, String>,
    /// Clients streaming live input (the GUI's controller view).
    watchers: Vec<Sender<Option<InputSnapshot>>>,
    /// Device whose input watchers are shown.
    last_active: Option<u64>,
    last_draw: Instant,
    focus_backend: FocusBackend,
    focused: Option<WindowInfo>,
    recent_windows: Vec<WindowInfo>,
    /// Last profile chosen by process scanning, so manual switches stick until it changes.
    scan_target: Option<String>,
    tx: Sender<Msg>,
}

pub fn run() -> Result<()> {
    let config = Config::load()?;
    let (tx, rx) = mpsc::channel();
    let listener = bind_socket()?;
    {
        let tx = tx.clone();
        thread::spawn(move || ipc_server(listener, tx));
    }

    let kbm = VirtualKbm::new().context(
        "creating virtual keyboard/mouse (do you have write access to /dev/uinput?)",
    )?;
    let mut daemon = Daemon {
        config,
        kbm,
        devices: HashMap::new(),
        next_id: 0,
        skipped: HashSet::new(),
        gamepads: HashMap::new(),
        motion_nodes: HashMap::new(),
        motion_denied: HashMap::new(),
        watchers: Vec::new(),
        last_active: None,
        last_draw: Instant::now(),
        focus_backend: FocusBackend::ProcessScan,
        focused: None,
        recent_windows: Vec::new(),
        scan_target: None,
        tx,
    };
    log!("controller_app daemon started, socket at {}", ipc::socket_path().display());
    {
        let tx = daemon.tx.clone();
        focus::spawn(move |ev| {
            let _ = tx.send(Msg::Focus(ev));
        });
    }
    daemon.scan();
    daemon.run(rx);
    Ok(())
}

fn bind_socket() -> Result<UnixListener> {
    let path = ipc::socket_path();
    if path.exists() {
        if UnixStream::connect(&path).is_ok() {
            bail!("another daemon is already running ({})", path.display());
        }
        std::fs::remove_file(&path)?;
    }
    UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))
}

impl Daemon {
    fn run(&mut self, rx: Receiver<Msg>) {
        let mut next_scan = Instant::now() + SCAN_INTERVAL;
        let mut last_tick = Instant::now();
        loop {
            let ticking = self.needs_tick();
            let mut timeout = if ticking {
                TICK.saturating_sub(last_tick.elapsed())
            } else {
                next_scan.saturating_duration_since(Instant::now())
            };
            if let Some(deadline) = self.next_deadline() {
                timeout = timeout.min(deadline.saturating_duration_since(Instant::now()));
            }
            match rx.recv_timeout(timeout) {
                Ok(msg) => {
                    self.handle(msg);
                    while let Ok(msg) = rx.try_recv() {
                        self.handle(msg);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }

            self.run_timers();
            let now = Instant::now();
            if ticking {
                let dt = now.duration_since(last_tick);
                if dt >= TICK {
                    // Clamp so a stall never turns into a huge cursor jump.
                    self.tick(dt.min(Duration::from_millis(20)).as_secs_f32());
                    last_tick = now;
                }
            } else {
                last_tick = now;
            }
            if now >= next_scan {
                self.scan();
                next_scan = now + SCAN_INTERVAL;
            }
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.devices.values().filter_map(|d| d.engine.next_deadline()).min()
    }

    /// Fires combo members whose combo window has expired.
    fn run_timers(&mut self) {
        let Some(profile) = self.config.active() else { return };
        let now = Instant::now();
        let mut switch = false;
        for dev in self.devices.values_mut() {
            if dev.engine.next_deadline().is_none_or(|d| d > now) {
                continue;
            }
            let mut out = Vec::new();
            switch |= dev.engine.timers(profile, now, &mut out);
            dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
            dev.draw_status();
        }
        if switch && let Some(next) = self.config.next_profile_name() {
            self.switch_profile(next);
        }
    }

    fn needs_tick(&self) -> bool {
        let Some(profile) = self.config.active() else { return false };
        self.devices.values().any(|d| d.engine.needs_tick(profile))
    }

    fn tick(&mut self, dt: f32) {
        let Some(profile) = self.config.active() else { return };
        for dev in self.devices.values_mut() {
            let mut out = Vec::new();
            dev.engine.tick(profile, dt, &mut out);
            dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
            dev.out_view.end_tick(dt);
        }
        if self.last_draw.elapsed() >= STATUS_REDRAW
            && let Some(dev) = self.last_active.and_then(|id| self.devices.get(&id))
        {
            dev.draw_status();
            self.last_draw = Instant::now();
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Input { id, events } => self.input(id, events),
            Msg::Gone { id } => {
                if let Some(mut dev) = self.devices.remove(&id) {
                    monitor::clear();
                    log!("device gone: {} ({})", dev.name, dev.path.display());
                    self.forget_active(id);
                    let mut out = Vec::new();
                    dev.engine.release_all(&mut out);
                    dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
                }
            }
            Msg::Ipc { req, reply } => {
                let _ = reply.send(self.request(req));
            }
            Msg::Focus(FocusEvent::Backend(backend)) => {
                self.focus_backend = backend;
                // Re-evaluate from scratch the next time processes are scanned.
                self.scan_target = None;
            }
            Msg::Focus(FocusEvent::Focused(window)) => self.window_focused(window),
            Msg::Motion { id, sample } => self.motion(id, sample),
            Msg::MotionGone { id } => {
                if let Some(dev) = self.devices.get_mut(&id) {
                    dev.motion = None;
                }
            }
            Msg::Watch(watcher) => {
                let current = self.last_active.and_then(|id| self.devices.get(&id));
                if watcher.send(current.map(|d| d.view.snapshot(&d.name))).is_ok() {
                    self.watchers.push(watcher);
                }
            }
        }
    }

    fn motion(&mut self, id: u64, mut sample: MotionSample) {
        let Some(dev) = self.devices.get_mut(&id) else { return };
        if let Some(cal) = &mut dev.calibrating {
            for (sum, v) in cal.sum.iter_mut().zip(sample.gyro) {
                *sum += v as f64;
            }
            cal.samples += 1;
            if cal.started.elapsed() < GYRO_CALIBRATION {
                return;
            }
            let n = cal.samples.max(1) as f64;
            let bias = cal.sum.map(|s| (s / n) as f32);
            dev.gyro_bias = bias;
            dev.calibrating = None;
            let name = dev.name.clone();
            log!("gyro calibrated for {name}: bias {bias:.2?} deg/s");
            self.config.gyro_calibration.insert(name, bias);
            if let Err(e) = self.config.save() {
                log!("saving config: {e:#}");
            }
            return;
        }
        for (v, bias) in sample.gyro.iter_mut().zip(dev.gyro_bias) {
            *v -= bias;
        }
        let sample = dev.motion_frame.to_standard(sample);
        dev.view.set_gyro(sample.gyro);
        let Some(profile) = self.config.active() else { return };
        let mut out = Vec::new();
        dev.engine.motion(profile, sample, &mut out);
        dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
        if self.last_draw.elapsed() >= STATUS_REDRAW {
            dev.draw_status();
            self.last_draw = Instant::now();
        }
        if self.last_active == Some(id) && !self.watchers.is_empty() {
            let snapshot = dev.view.snapshot(&dev.name);
            self.watchers.retain(|w| w.send(Some(snapshot.clone())).is_ok());
        }
    }

    /// Pairs managed controllers with their motion-sensor devices and starts reading them.
    fn attach_motion(&mut self) {
        let pairs: Vec<(u64, PathBuf, String)> = self
            .devices
            .iter()
            .filter(|(_, d)| d.motion.is_none())
            .filter_map(|(id, d)| {
                let taken = |n: &MotionNode| self.devices.values().any(|o| o.motion.as_ref() == Some(&n.path));
                let node = self.motion_nodes.values().find(|n| !taken(n) && n.belongs_to(d))?;
                Some((*id, node.path.clone(), node.name.clone()))
            })
            .collect();
        for (id, path, motion_name) in pairs {
            let Ok(motion_dev) = Device::open(&path) else { continue };
            let Some(dev) = self.devices.get_mut(&id) else { continue };
            log!("gyro: using {motion_name} for {}", dev.name);
            dev.motion_frame = MotionFrame::of(&path);
            dev.motion = Some(path);
            dev.gyro_bias = self.config.gyro_calibration.get(&dev.name).copied().unwrap_or_default();
            let (stop, tx) = (dev.stop.clone(), self.tx.clone());
            thread::spawn(move || read_motion(id, motion_dev, stop, tx));
        }
    }

    fn window_focused(&mut self, window: WindowInfo) {
        // Editing settings while a game runs must not flip the profile.
        if focus::is_own_window(&window) {
            return;
        }
        let same = |w: &WindowInfo| w.class == window.class && w.exe == window.exe && w.steam_app_id == window.steam_app_id;
        self.recent_windows.retain(|w| !same(w));
        self.recent_windows.insert(0, window.clone());
        self.recent_windows.truncate(RECENT_WINDOWS);
        self.focused = Some(window.clone());

        if !self.config.auto_switch.enabled {
            return;
        }
        let target = focus::profile_for(&self.config.auto_switch, &window).map(str::to_string);
        if let Some(name) = target {
            self.auto_switch_to(name, &describe(&window));
        }
    }

    /// Process-scan fallback for desktops without focus tracking.
    fn scan_processes(&mut self) {
        let auto = &self.config.auto_switch;
        if self.focus_backend != FocusBackend::ProcessScan || !auto.enabled || auto.rules.is_empty() {
            return;
        }
        let target = focus::profile_for_processes(auto, &focus::running_processes()).map(str::to_string);
        if target == self.scan_target {
            return;
        }
        self.scan_target = target.clone();
        if let Some(name) = target {
            self.auto_switch_to(name, "running processes");
        }
    }

    fn auto_switch_to(&mut self, name: String, reason: &str) {
        if name == self.config.active_profile {
            return;
        }
        if !self.config.profiles.iter().any(|p| p.name == name) {
            log!("per-game rule points to missing profile {name:?}");
            return;
        }
        log!("{reason} → profile {name:?}");
        self.switch_profile(name);
    }

    fn broadcast(&mut self, snapshot: Option<InputSnapshot>) {
        self.watchers.retain(|w| w.send(snapshot.clone()).is_ok());
    }

    /// Called when a device stops being managed; watchers fall back to "no controller".
    fn forget_active(&mut self, id: u64) {
        if self.last_active == Some(id) {
            self.last_active = None;
            self.broadcast(None);
        }
    }

    fn input(&mut self, id: u64, events: Vec<InputEvent>) {
        let Some(profile) = self.config.active() else { return };
        let Some(dev) = self.devices.get_mut(&id) else { return };
        let mut out = Vec::new();
        let mut switch = false;
        let now = Instant::now();
        for ev in events {
            dev.view.apply(&ev);
            switch |= dev.engine.handle(profile, ev, now, &mut out);
        }
        dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
        if !dev.engine.needs_tick(profile) {
            dev.out_view.stop_motion();
        }
        dev.draw_status();
        self.last_draw = Instant::now();
        self.last_active = Some(id);
        if !self.watchers.is_empty() {
            let snapshot = dev.view.snapshot(&dev.name);
            self.watchers.retain(|w| w.send(Some(snapshot.clone())).is_ok());
        }
        if switch && let Some(next) = self.config.next_profile_name() {
            self.switch_profile(next);
        }
    }

    fn request(&mut self, req: Request) -> Response {
        match req {
            Request::Status => Response::Status(self.status()),
            Request::GetConfig => Response::Config(self.config.clone()),
            Request::SetConfig(mut new) => {
                new.enabled = self.config.enabled;
                if new.profiles.iter().any(|p| p.name == self.config.active_profile) {
                    new.active_profile = self.config.active_profile.clone();
                }
                self.replace_config(new)
            }
            Request::Reload => match Config::load() {
                Ok(new) => self.replace_config(new),
                Err(e) => Response::Error(format!("{e:#}")),
            },
            Request::SetEnabled(enabled) => {
                self.config.enabled = enabled;
                if !enabled {
                    self.release_devices(|_| true);
                }
                self.save_and_rescan()
            }
            Request::SetProfile(name) => {
                if !self.config.profiles.iter().any(|p| p.name == name) {
                    return Response::Error(format!("no profile named {name:?}"));
                }
                self.switch_profile(name);
                Response::Ok
            }
            Request::NextProfile => {
                if let Some(next) = self.config.next_profile_name() {
                    self.switch_profile(next);
                }
                Response::Ok
            }
            // Handled by the connection thread, which registers through `Msg::Watch`.
            Request::WatchInput => Response::Error("WatchInput must be the only request".into()),
            Request::CalibrateGyro(path) => {
                let dev = self.devices.values_mut().find(|d| d.path.as_os_str() == path.as_str());
                match dev {
                    Some(dev) if dev.motion.is_some() => {
                        log!("calibrating gyro for {}: keep it still", dev.name);
                        dev.calibrating = Some(Calibration { started: Instant::now(), sum: [0.0; 3], samples: 0 });
                        Response::Ok
                    }
                    Some(_) => Response::Error(format!("{path} has no gyro")),
                    None => Response::Error(format!("{path} is not being managed")),
                }
            }
            Request::TestRumble(path) => {
                let known = self
                    .gamepads
                    .iter()
                    .find(|((p, _), _)| p.as_os_str() == path.as_str())
                    .map(|(_, pad)| pad.rumble);
                match known {
                    Some(true) => {
                        rumble::test(PathBuf::from(path));
                        Response::Ok
                    }
                    Some(false) => Response::Error(format!("{path} does not support rumble")),
                    None => Response::Error(format!("no controller at {path}")),
                }
            }
        }
    }

    fn replace_config(&mut self, new: Config) -> Response {
        if new.profiles.is_empty() {
            return Response::Error("config must contain at least one profile".into());
        }
        self.release_all();
        self.config = new;
        for dev in self.devices.values_mut() {
            dev.engine.set_macros(&self.config.macros);
        }
        let ignored = self.config.ignored_devices.clone();
        let enabled = self.config.enabled;
        self.release_devices(|d| !enabled || ignored.contains(&d.name));
        self.resync_all();
        self.save_and_rescan()
    }

    fn save_and_rescan(&mut self) -> Response {
        self.scan();
        match self.config.save() {
            Ok(()) => Response::Ok,
            Err(e) => Response::Error(format!("saving config: {e:#}")),
        }
    }

    fn switch_profile(&mut self, name: String) {
        if name == self.config.active_profile {
            return;
        }
        log!("switching to profile {name:?}");
        self.release_all();
        self.config.active_profile = name;
        self.resync_all();
        if let Err(e) = self.config.save() {
            log!("saving config: {e:#}");
        }
    }

    /// Releases every output held by every device (before a profile change).
    fn release_all(&mut self) {
        for dev in self.devices.values_mut() {
            let mut out = Vec::new();
            dev.engine.release_all(&mut out);
            dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
        }
    }

    fn resync_all(&mut self) {
        let Some(profile) = self.config.active() else { return };
        for dev in self.devices.values_mut() {
            let mut out = Vec::new();
            dev.engine.resync(profile, &mut out);
            dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
        }
    }

    /// Stops managing (ungrabs) devices matching `pred`.
    fn release_devices(&mut self, pred: impl Fn(&Managed) -> bool) {
        let ids: Vec<u64> = self.devices.iter().filter(|(_, d)| pred(d)).map(|(id, _)| *id).collect();
        for id in ids {
            let mut dev = self.devices.remove(&id).unwrap();
            dev.stop.store(true, Ordering::Relaxed);
            monitor::clear();
            self.forget_active(id);
            let mut out = Vec::new();
            dev.engine.release_all(&mut out);
            dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
            log!("released {} ({})", dev.name, dev.path.display());
        }
    }

    fn status(&self) -> Status {
        let mut devices: Vec<DeviceInfo> = self
            .gamepads
            .iter()
            .map(|((path, _), pad)| DeviceInfo {
                name: pad.name.clone(),
                path: path.display().to_string(),
                managed: self.devices.values().any(|d| &d.path == path),
                ignored: self.config.ignored_devices.contains(&pad.name),
                gyro: self.devices.values().any(|d| &d.path == path && d.motion.is_some()),
                analog_triggers: pad.analog_triggers,
                rumble: pad.rumble,
            })
            .collect();
        devices.sort_by(|a, b| a.path.cmp(&b.path));
        Status {
            enabled: self.config.enabled,
            active_profile: self.config.active().map(|p| p.name.clone()).unwrap_or_default(),
            devices,
            focus_backend: self.focus_backend,
            focused: self.focused.clone(),
            recent_windows: self.recent_windows.clone(),
            motion_access_denied: self.motion_denied.values().cloned().collect(),
        }
    }

    /// Looks for new gamepads in /dev/input and grabs the ones we should manage.
    fn scan(&mut self) {
        let Ok(entries) = std::fs::read_dir("/dev/input") else { return };
        let mut present = HashSet::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event")) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let key = (path.clone(), meta.ino());
            present.insert(key.clone());

            if self.skipped.contains(&key)
                || self.motion_nodes.contains_key(&key)
                || self.devices.values().any(|d| d.path == path)
            {
                continue;
            }
            if let Some(pad) = self.gamepads.get(&key)
                && (!self.config.enabled || self.config.ignored_devices.contains(&pad.name))
            {
                continue;
            }
            // Permission errors are not cached: udev may grant access a moment later.
            let Ok(dev) = Device::open(&path) else {
                if !self.motion_denied.contains_key(&key)
                    && let Some(name) = input::inaccessible_motion_sensor(&path)
                {
                    log!("gyro: no permission to read {name}; install dist/70-controller-app-motion.rules");
                    self.motion_denied.insert(key, name);
                }
                continue;
            };
            self.motion_denied.remove(&key);
            let name = dev.name().unwrap_or("Unknown").to_string();
            if !is_uinput(&path) && input::is_motion_sensor(&dev) {
                let uniq = dev.unique_name().map(str::to_string);
                self.motion_nodes.insert(key, MotionNode { path: path.clone(), name, parent: hid_parent(&path), uniq });
                continue;
            }
            if name.starts_with(VIRTUAL_PREFIX) || is_uinput(&path) || !input::is_gamepad(&dev) {
                self.skipped.insert(key);
                continue;
            }
            let analog_triggers = input::has_analog_triggers(&dev);
            let rumble = rumble::supports_rumble(&dev);
            self.gamepads.insert(key, SeenGamepad { name: name.clone(), analog_triggers, rumble });
            if self.config.enabled && !self.config.ignored_devices.contains(&name) {
                self.manage(path, name, dev);
            }
        }
        self.skipped.retain(|k| present.contains(k));
        self.motion_nodes.retain(|k, _| present.contains(k));
        self.motion_denied.retain(|k, _| present.contains(k));
        self.attach_motion();
        self.scan_processes();
        self.gamepads.retain(|k, _| present.contains(k));
    }

    fn manage(&mut self, path: PathBuf, name: String, mut dev: Device) {
        let parent = hid_parent(&path);
        let uniq = dev.unique_name().map(str::to_string);
        if let Err(e) = dev.grab() {
            log!("cannot grab {name} ({}): {e}", path.display());
            return;
        }
        // Mirror the controller's rumble support so games see it exactly when it exists.
        let ff = dev
            .supported_ff()
            .filter(|ff| ff.iter().next().is_some() && dev.max_ff_effects() > 0)
            .map(|effects| FfCaps { effects, max_effects: dev.max_ff_effects() as u32 });
        let has_rumble = ff.is_some();
        let pad = match VirtualPad::new(ff) {
            Ok(pad) => pad,
            Err(e) => {
                log!("cannot create virtual pad: {e:#}");
                return;
            }
        };
        log!("managing {name} ({})", path.display());
        let id = self.next_id;
        self.next_id += 1;
        let stop = Arc::new(AtomicBool::new(false));
        {
            let stop = stop.clone();
            let tx = self.tx.clone();
            let xbox_labels = input::uses_xbox_labels(&path);
            thread::spawn(move || read_device(id, dev, xbox_labels, stop, tx));
        }
        if has_rumble {
            rumble::spawn(pad.shared(), path.clone(), stop.clone());
        }
        let mut managed = Managed {
            path,
            name,
            engine: {
                let mut engine = Engine::default();
                engine.set_macros(&self.config.macros);
                engine
            },
            pad,
            stop,
            view: InputView::default(),
            out_view: OutputView::default(),
            parent,
            uniq,
            motion: None,
            motion_frame: MotionFrame::default(),
            gyro_bias: [0.0; 3],
            calibrating: None,
        };
        if let Some(profile) = self.config.active() {
            let mut out = Vec::new();
            managed.engine.resync(profile, &mut out);
            dispatch(&mut managed.pad, &mut managed.out_view, &mut self.kbm, out);
        }
        self.devices.insert(id, managed);
    }
}

/// Devices created through uinput (Steam Input, other remappers, us) live under
/// /sys/devices/virtual/input. BLE pads via uhid are under /virtual/misc and are kept.
fn is_uinput(dev_path: &Path) -> bool {
    let Some(node) = dev_path.file_name() else { return false };
    let sys = Path::new("/sys/class/input").join(node).join("device");
    std::fs::canonicalize(sys)
        .map(|p| p.to_string_lossy().contains("/devices/virtual/input/"))
        .unwrap_or(false)
}

/// The HID device an input node belongs to; a controller's gamepad and motion-sensor nodes
/// share it.
fn hid_parent(dev_path: &Path) -> Option<PathBuf> {
    let node = dev_path.file_name()?;
    std::fs::canonicalize(Path::new("/sys/class/input").join(node).join("device/device")).ok()
}

fn read_motion(id: u64, mut dev: Device, stop: Arc<AtomicBool>, tx: Sender<Msg>) {
    let mut norm = MotionNormalizer::new(&dev);
    if dev.set_nonblocking(true).is_err() {
        let _ = tx.send(Msg::MotionGone { id });
        return;
    }
    let mut pfd = libc::pollfd { fd: dev.as_raw_fd(), events: libc::POLLIN, revents: 0 };
    while !stop.load(Ordering::Relaxed) {
        // SAFETY: pfd points to a single valid pollfd for the duration of the call.
        let n = unsafe { libc::poll(&mut pfd, 1, POLL_TIMEOUT_MS) };
        if n < 0 && std::io::Error::last_os_error().kind() != ErrorKind::Interrupted {
            break;
        }
        if n <= 0 {
            continue;
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            break;
        }
        let samples: Vec<MotionSample> = match dev.fetch_events() {
            Ok(evs) => evs.filter_map(|ev| norm.translate(ev)).collect(),
            Err(e) if e.kind() == ErrorKind::WouldBlock => continue,
            Err(_) => break,
        };
        for sample in samples {
            if tx.send(Msg::Motion { id, sample }).is_err() {
                return;
            }
        }
    }
    let _ = tx.send(Msg::MotionGone { id });
}

fn read_device(id: u64, mut dev: Device, xbox_labels: bool, stop: Arc<AtomicBool>, tx: Sender<Msg>) {
    let mut norm = Normalizer::new(&dev, xbox_labels);
    if let Err(e) = dev.set_nonblocking(true) {
        log!("set_nonblocking: {e}");
        let _ = tx.send(Msg::Gone { id });
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
            Ok(evs) => evs.for_each(|ev| norm.translate(ev, &mut events)),
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(_) => break,
        }
        if !events.is_empty() && tx.send(Msg::Input { id, events: std::mem::take(&mut events) }).is_err() {
            return;
        }
    }
    // Dropping `dev` closes the fd, which also releases the grab.
    let _ = tx.send(Msg::Gone { id });
}

fn dispatch(pad: &mut VirtualPad, view: &mut OutputView, kbm: &mut VirtualKbm, out: Vec<OutEvent>) {
    for ev in out {
        view.apply(&ev);
        let res = match ev {
            OutEvent::PadButton(b, pressed) => pad.button(b, pressed),
            OutEvent::PadAxis(axis, v) => pad.axis(axis, v),
            OutEvent::Key(k, pressed) => kbm.key(k, pressed),
            OutEvent::MouseButton(b, pressed) => kbm.mouse_button(b, pressed),
            OutEvent::MouseMove(dx, dy) => kbm.mouse_move(dx, dy),
            OutEvent::Wheel { vertical, horizontal } => kbm.wheel(vertical, horizontal),
        };
        if let Err(e) = res {
            log!("output error: {e:#}");
        }
    }
}

fn ipc_server(listener: UnixListener, tx: Sender<Msg>) {
    for conn in listener.incoming() {
        let Ok(conn) = conn else { continue };
        let tx = tx.clone();
        // One thread per client: input watchers hold their connection open.
        thread::spawn(move || {
            if let Err(e) = serve_client(conn, &tx) {
                log!("ipc: {e:#}");
            }
        });
    }
}

fn serve_client(conn: UnixStream, tx: &Sender<Msg>) -> Result<()> {
    conn.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut line = String::new();
    BufReader::new(&conn).read_line(&mut line)?;
    let response = match serde_json::from_str::<Request>(&line) {
        Ok(Request::WatchInput) => return watch_input(conn, tx),
        Ok(req) => {
            let (reply_tx, reply_rx) = mpsc::channel();
            tx.send(Msg::Ipc { req, reply: reply_tx })?;
            reply_rx.recv_timeout(Duration::from_secs(2))?
        }
        Err(e) => Response::Error(format!("bad request: {e}")),
    };
    let mut out = serde_json::to_string(&response)?;
    out.push('\n');
    (&conn).write_all(out.as_bytes())?;
    Ok(())
}

/// Streams snapshots to a client until it disconnects. Bursts are coalesced to the latest
/// snapshot, so a fast-polling pad never floods the GUI.
fn watch_input(conn: UnixStream, tx: &Sender<Msg>) -> Result<()> {
    let (snap_tx, snap_rx) = mpsc::channel();
    tx.send(Msg::Watch(snap_tx))?;
    // Blocks until the next update; ends when the daemon drops this watcher.
    while let Ok(mut snapshot) = snap_rx.recv() {
        while let Ok(newer) = snap_rx.try_recv() {
            snapshot = newer;
        }
        let mut line = serde_json::to_string(&snapshot)?;
        line.push('\n');
        if (&conn).write_all(line.as_bytes()).is_err() {
            // Client went away; the daemon drops our sender on its next send.
            return Ok(());
        }
        thread::sleep(WATCH_INTERVAL);
    }
    Ok(())
}

/// Short description of a window for log lines.
fn describe(w: &WindowInfo) -> String {
    match &w.steam_app_id {
        Some(id) => format!("{} (Steam {id})", w.exe),
        None if w.exe.is_empty() => w.class.clone(),
        None => w.exe.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, parent: Option<&str>, uniq: Option<&str>) -> MotionNode {
        MotionNode {
            path: PathBuf::from("/dev/input/event99"),
            name: name.into(),
            parent: parent.map(PathBuf::from),
            uniq: uniq.map(Into::into),
        }
    }

    #[test]
    fn motion_sensors_pair_with_their_own_controller() {
        let hid = Some(PathBuf::from("/sys/devices/x/0005:054C:0CE6.0001"));
        let mac = Some("a0:5a:5c:00:00:01".to_string());
        let pad = "Sony Interactive Entertainment DualSense Wireless Controller";

        // Same HID parent (USB or Bluetooth), or same MAC.
        assert!(node("x", Some("/sys/devices/x/0005:054C:0CE6.0001"), None).matches(pad, &hid, &None));
        assert!(node("x", None, Some("a0:5a:5c:00:00:01")).matches(pad, &None, &mac));
        // Driver naming: "<pad> Motion Sensors" (hid-playstation), "<pad> IMU" (hid-nintendo).
        assert!(node(&format!("{pad} Motion Sensors"), None, None).matches(pad, &None, &None));
        assert!(node("Nintendo Switch Pro Controller IMU", None, None).matches("Nintendo Switch Pro Controller", &None, &None));

        // A second identical controller's sensors don't match by parent or MAC, even though
        // the name would.
        let other = node(&format!("{pad} Motion Sensors"), Some("/sys/devices/x/0005:054C:0CE6.0002"), Some("a0:5a:5c:00:00:02"));
        assert!(!other.matches(pad, &hid, &mac));
        // Empty unique IDs (common over USB) never count as a match.
        assert!(!node("x", None, Some("")).matches(pad, &None, &Some(String::new())));
    }
}

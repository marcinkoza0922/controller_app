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
    config::{Config, Profile, ProfileRef, Scope},
    engine::{Engine, Opener},
    focus::{self, FocusEvent},
    input::{self, InputEvent, MotionFrame, MotionNormalizer, MotionSample, Normalizer},
    keyboard::Layout,
    ipc::{self, DeviceInfo, FocusBackend, InputSnapshot, Request, Response, Status, WindowInfo},
    monitor::{self, InputView, OutputView, log},
    output::{FfCaps, OutEvent, VIRTUAL_PREFIX, VirtualKbm, VirtualPad},
    menu::{MenuOutcome, MenuSession},
    overlay::{OverlayAction, OverlayController, OverlayFrame, OverlayView},
    rumble,
};

mod logs;

const TICK: Duration = Duration::from_millis(4);
/// How often a fading info overlay is redrawn.
const FADE_FRAME: Duration = Duration::from_millis(33);
/// How often info overlays with live values (time, CPU, …) update.
const INFO_REFRESH: Duration = Duration::from_secs(1);
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
    WatchOverlay(Sender<OverlayFrame>),
}

struct Managed {
    path: PathBuf,
    name: String,
    /// Whose button names glyphs use, if recognized; otherwise `Config::info_glyphs`.
    family: Option<crate::info::PadFamily>,
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
    /// The active profile with this controller's layers on top, while it has any, and which
    /// layers those are.
    layered: Option<(Vec<String>, Profile)>,
    /// What it pressed lately, for the input log.
    log: crate::inputlog::InputLog,
    /// 0, 1, 2… in the order controllers connected, kept until this one disconnects
    /// (`{current_input_device_N}`, a log overlay's controller).
    number: u8,
}

/// What a controller's mappings come from: the active profile, or that profile with the
/// controller's layers on top.
fn layered<'a>(cached: &'a Option<(Vec<String>, Profile)>, base: &'a Profile) -> &'a Profile {
    cached.as_ref().map_or(base, |(_, p)| p)
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
    /// Brings `layered` up to date with the engine's layers. True if they changed.
    fn refresh_layers(&mut self, config: &Config) -> bool {
        let layers = self.engine.layers();
        let current = self.layered.as_ref().map(|(names, _)| names.as_slice()).unwrap_or_default();
        if layers == current {
            return false;
        }
        self.layered = if layers.is_empty() {
            None
        } else {
            let game = config.active_game();
            config.active().map(|base| (layers.clone(), base.with_layers(game.layers_named(&layers))))
        };
        true
    }

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

/// What the overlay is showing.
enum Active {
    Keyboard(OverlayController),
    /// A menu, and the controller that opened it (its engine runs the chosen item).
    Menu { session: MenuSession, device: u64 },
}

/// A device node identity; the inode changes when a node is recreated for a new device.
type NodeKey = (PathBuf, u64);

struct Daemon {
    config: Config,
    /// What the active profile can use (its game's items, then shared ones).
    scope: Scope,
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
    scan_target: Option<ProfileRef>,
    /// What the on-screen overlay is showing; while set it gets all controller input.
    active: Option<Active>,
    /// Where the overlay keyboard's cursor was, for the next time it opens.
    /// Where each on-screen layout's cursor was left, to pick up there next time.
    overlay_cursors: HashMap<Layout, crate::keyboard::Cursor>,
    overlay_watchers: Vec<Sender<OverlayFrame>>,
    /// The frame last sent to the overlay window, to skip sending it again unchanged.
    last_frame: Option<OverlayFrame>,
    /// When shown info overlays with live values next refresh.
    info_refresh: Option<Instant>,
    /// When an info overlay that hides after a while next needs redrawing (fading or gone).
    info_fade: Option<Instant>,
    /// Info overlays shown for a while: at the game's start, or lingering after being let go.
    info_timers: crate::info::Timers,
    /// A notice at the top of the screen, such as the profile just switched to.
    toast: Option<crate::info::Toast>,
    /// Log overlays lingering after being let go.
    log_timers: crate::info::Timers,
    /// When a shown log or `{current_input}` cell next changes on its own.
    log_redraw: Option<Instant>,
    /// Something on screen shows the input log, so new input redraws it.
    shows_input: bool,
    /// Each game's process when it last started, so focusing it again isn't another start.
    launches: HashMap<String, u32>,
    sampler: crate::info::Sampler,
    /// When the overlay window was last started, so a window that can't start (no
    /// layer-shell) isn't restarted every second for info overlays.
    overlay_started: Option<Instant>,
    overlay_process: Option<std::process::Child>,
    tx: Sender<Msg>,
}

pub fn run() -> Result<()> {
    let config = Config::load()?;
    let (tx, rx) = mpsc::channel();
    let listener = bind_socket()?;
    {
        let tx = tx.clone();
        thread::spawn(move || ipc_server(&listener, &tx));
    }

    let kbm = VirtualKbm::new().context(
        "creating virtual keyboard/mouse (do you have write access to /dev/uinput?)",
    )?;
    let mut daemon = Daemon {
        scope: config.scope(),
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
        active: None,
        overlay_cursors: HashMap::new(),
        overlay_watchers: Vec::new(),
        last_frame: None,
        info_refresh: None,
        info_fade: None,
        info_timers: crate::info::Timers::default(),
        toast: None,
        log_timers: crate::info::Timers::default(),
        log_redraw: None,
        shows_input: false,
        launches: HashMap::new(),
        sampler: crate::info::Sampler::default(),
        overlay_started: None,
        overlay_process: None,
        tx,
    };
    log!("controller_app daemon started, socket at {}", ipc::socket_path().display());
    {
        let tx = daemon.tx.clone();
        focus::spawn(move |ev| {
            let _ = tx.send(Msg::Focus(ev));
        });
    }
    // Start the overlay window now so menus appear instantly; it idles invisibly.
    daemon.ensure_overlay_process();
    daemon.scan();
    daemon.run(&rx);
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
    fn run(&mut self, rx: &Receiver<Msg>) {
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
        let overlay = match &self.active {
            Some(Active::Keyboard(k)) => k.next_deadline(),
            Some(Active::Menu { session, .. }) => session.next_deadline(),
            None => None,
        };
        self.devices
            .values()
            .filter_map(|d| d.engine.next_deadline())
            .chain(overlay)
            .chain(self.info_refresh)
            .chain(self.info_fade)
            .chain(self.log_redraw)
            .min()
    }

    /// Fires combo members whose combo window has expired.
    fn run_timers(&mut self) {
        let now = Instant::now();
        if self.info_refresh.is_some_and(|t| t <= now) {
            self.sampler.sample();
            self.broadcast_overlay();
        } else if self.info_fade.is_some_and(|t| t <= now) || self.log_redraw.is_some_and(|t| t <= now) {
            self.broadcast_overlay();
        }
        let (mut keyboard_actions, mut changed) = (Vec::new(), false);
        match &mut self.active {
            Some(Active::Keyboard(k)) if k.next_deadline().is_some_and(|d| d <= now) => {
                (keyboard_actions, changed) = k.tick(now);
            }
            Some(Active::Menu { session, .. }) if session.next_deadline().is_some_and(|d| d <= now) => {
                changed = session.tick(&self.scope.menus, now);
            }
            _ => {}
        }
        self.apply_keyboard(keyboard_actions);
        if changed {
            self.broadcast_overlay();
        }
        let Some(base) = self.config.active() else { return };
        let mut switch = false;
        let mut toggle_overlay = None;
        let mut menu_request = None;
        let mut fired = Vec::new();
        for (id, dev) in self.devices.iter_mut() {
            if dev.engine.next_deadline().is_none_or(|d| d > now) {
                continue;
            }
            fired.push(*id);
            let mut out = Vec::new();
            switch |= dev.engine.timers(layered(&dev.layered, base), now, &mut out);
            if dev.refresh_layers(&self.config) {
                dev.engine.resync(layered(&dev.layered, base), &mut out);
            }
            dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
            toggle_overlay = dev.engine.take_overlay_toggle().or(toggle_overlay);
            if let Some(request) = dev.engine.take_menu_request() {
                menu_request = Some((*id, request));
            }
            dev.draw_status();
        }
        if switch && let Some(next) = self.config.next_profile() {
            self.switch_profile(next);
        }
        if let Some(layout) = toggle_overlay {
            self.toggle_overlay(layout);
        }
        if let Some((id, (name, opener))) = menu_request {
            self.open_menu(id, &name, opener);
        }
        // A gesture or combo window ran out: its action labels the press it came from.
        if fired.into_iter().fold(false, |any, id| self.log_fired(id) | any) {
            self.log_changed();
        }
        self.check_info_changes();
    }

    /// Opens or closes the on-screen keyboard or numpad (one replaces the other).
    fn toggle_overlay(&mut self, layout: Layout) {
        match &self.active {
            Some(Active::Keyboard(k)) if k.layout() == layout => return self.close_overlay(),
            Some(Active::Keyboard(_)) => self.close_overlay(),
            Some(Active::Menu { .. }) => return,
            None => {}
        }
        if !self.ensure_overlay_process() {
            return;
        }
        self.release_mappings();
        log!("on-screen {} open", layout_name(layout));
        let cursor = self.overlay_cursors.get(&layout).copied().unwrap_or(layout.home());
        self.active = Some(Active::Keyboard(OverlayController::new(layout, cursor)));
        self.broadcast_overlay();
    }

    fn open_menu(&mut self, device: u64, name: &str, opener: Opener) {
        if self.active.is_some() {
            return;
        }
        let Some(mut session) = MenuSession::open(&self.scope.menus, name, opener) else {
            log!("no menu named {name:?}, or it has no items");
            return;
        };
        if !self.ensure_overlay_process() {
            return;
        }
        if let Some(dev) = self.devices.get(&device)
            && !session.prime(&self.scope.menus, dev.view.buttons(), dev.view.axes())
        {
            log!("menu {name:?} not shown: its button was already let go (use a Toggle to open it with a tap)");
            return;
        }
        self.release_mappings();
        self.active = Some(Active::Menu { session, device });
        self.broadcast_overlay();
    }

    /// Makes sure the (normally resident) overlay window process is running.
    fn ensure_overlay_process(&mut self) -> bool {
        if let Some(child) = &mut self.overlay_process
            && child.try_wait().ok().flatten().is_some()
        {
            self.overlay_process = None;
        }
        if self.overlay_process.is_some() {
            return true;
        }
        match std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg("overlay").spawn()) {
            Ok(child) => {
                self.overlay_process = Some(child);
                self.overlay_started = Some(Instant::now());
                true
            }
            Err(e) => {
                log!("cannot start the overlay window: {e}");
                false
            }
        }
    }

    /// Lets go of everything the mappings hold; the controller now drives the overlay.
    /// Toggled layers stay on, so a menu item that toggles one isn't undone by the menu.
    fn release_mappings(&mut self) {
        for dev in self.devices.values_mut() {
            let mut out = Vec::new();
            dev.engine.release_all(true, &mut out);
            dev.refresh_layers(&self.config);
            dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
        }
    }

    fn close_overlay(&mut self) {
        match self.active.take() {
            Some(Active::Keyboard(mut k)) => {
                self.overlay_cursors.insert(k.layout(), k.cursor());
                let actions = k.release_all();
                self.apply_keyboard(actions);
                log!("on-screen {} closed", layout_name(k.layout()));
            }
            Some(Active::Menu { .. }) => {}
            None => return,
        }
        // The overlay window goes back to idle (it stays running for next time).
        self.broadcast_overlay();
        for dev in self.devices.values_mut() {
            let down = dev.view.buttons().collect();
            dev.engine.forget_released(&down);
        }
        self.resync_all();
    }

    fn apply_keyboard(&mut self, actions: Vec<OverlayAction>) {
        for action in actions {
            match action {
                OverlayAction::Key(k, pressed) => {
                    if let Err(e) = self.kbm.key(k, pressed) {
                        log!("output error: {e:#}");
                    }
                }
                OverlayAction::Close => self.close_overlay(),
            }
        }
    }

    /// Runs a chosen menu item on the controller that opened the menu.
    fn run_menu_item(&mut self, device: u64, menu: &str, item: usize, action: &crate::config::ButtonAction) {
        let Some(dev) = self.devices.get_mut(&device) else { return };
        let mut out = Vec::new();
        let switch = dev.engine.tap_menu_item(menu, item, action, &mut out);
        if dev.refresh_layers(&self.config)
            && let Some(base) = self.config.active()
        {
            dev.engine.resync(layered(&dev.layered, base), &mut out);
        }
        dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
        let toggle_overlay = dev.engine.take_overlay_toggle();
        let menu_request = dev.engine.take_menu_request();
        // Switching profiles or opening the keyboard closes a menu that is still up.
        let menu_up = matches!(self.active, Some(Active::Menu { .. }));
        if menu_up && (switch || toggle_overlay.is_some()) {
            self.close_overlay();
        }
        if switch && let Some(next) = self.config.next_profile() {
            self.switch_profile(next);
        }
        if let Some(layout) = toggle_overlay {
            self.toggle_overlay(layout);
        }
        if let Some((name, opener)) = menu_request {
            self.open_menu(device, &name, opener);
        }
        self.check_info_changes();
    }

    fn overlay_view(&self) -> Option<OverlayView> {
        match &self.active {
            Some(Active::Keyboard(k)) => {
                let mut view = k.view(Instant::now());
                view.style = match k.layout() {
                    Layout::Keyboard => self.config.active_keyboard_style().clone(),
                    Layout::Numpad => self.config.active_numpad_style().clone(),
                };
                Some(OverlayView::Keyboard(view))
            }
            Some(Active::Menu { session, .. }) => session.view(&self.scope.menus).map(OverlayView::Menu),
            None => None,
        }
    }

    fn broadcast_overlay(&mut self) {
        let frame = self.overlay_frame();
        if (!frame.info.is_empty() || !frame.logs.is_empty())
            && self.overlay_process.is_none()
            && self.overlay_started.is_none_or(|t| t.elapsed() > Duration::from_secs(10))
        {
            self.ensure_overlay_process();
        }
        if self.last_frame.as_ref() == Some(&frame) {
            return;
        }
        self.overlay_watchers.retain(|w| w.send(frame.clone()).is_ok());
        self.last_frame = Some(frame);
    }

    fn overlay_frame(&mut self) -> OverlayFrame {
        let now = Instant::now();
        let shown = self.timed_info(now);
        // Live values (clock, CPU, …) refresh every second while shown.
        let live = shown.iter().any(|(o, _)| crate::info::is_live(o));
        self.info_refresh = live.then(|| Instant::now() + INFO_REFRESH);
        if live && self.sampler.sampled_at.is_none() {
            self.sampler.sample();
        }
        let mut values = self.live_values();
        let with_input: Vec<&crate::config::InfoOverlay> =
            shown.iter().map(|(o, _)| o).filter(|o| !crate::info::input_tokens(o).is_empty()).collect();
        if !with_input.is_empty() {
            values.inputs = self.inputs(now);
        }
        let token_redraw = self.token_redraw(&with_input, &values.inputs, now);
        let (logs, logs_up) = self.log_frame(now, values.family);
        self.shows_input = !with_input.is_empty() || logs_up;
        self.log_redraw = self.log_redraw.into_iter().chain(token_redraw).min();
        let mut info: Vec<_> = shown
            .iter()
            .map(|(o, opacity)| crate::info::InfoView { opacity: *opacity, ..crate::info::resolve(o, &values) })
            .collect();
        let now = Instant::now();
        let toast = self.toast.as_ref().and_then(|t| Some((t.view(now)?, t.next_redraw(now, FADE_FRAME))));
        match toast {
            Some((view, redraw)) => {
                info.push(view);
                self.info_fade = Some(self.info_fade.map_or(redraw, |t| t.min(redraw)));
            }
            None => self.toast = None,
        }
        OverlayFrame { info, logs, active: self.overlay_view(), font: self.config.active_font().map(str::to_string) }
    }

    /// The info overlays to draw now and how visible each is: steady ones fully, and those
    /// shown for a while (at the game's start, or lingering after being let go) fading out at
    /// the end of it.
    fn timed_info(&mut self, now: Instant) -> Vec<(crate::config::InfoOverlay, f32)> {
        let held: HashSet<String> = self.devices.values().flat_map(|d| d.engine.shown_info()).cloned().collect();
        self.info_timers.update(held, &self.scope.info, now);
        self.info_fade = self.info_timers.next_redraw(now, FADE_FRAME);
        self.visible_info()
            .into_iter()
            .map(|(o, steady)| {
                let opacity = if steady { 1.0 } else { self.info_timers.opacity(&o.name, now) };
                (o, opacity)
            })
            .filter(|(_, opacity)| *opacity > 0.0)
            .collect()
    }

    /// Info overlays to show now, and whether each is up steadily: those of the active game set
    /// to always show, any an action holds up, those shown for a while (at the game's start,
    /// or lingering), and active layers' indicators.
    fn visible_info(&self) -> Vec<(crate::config::InfoOverlay, bool)> {
        use crate::config::{Indicator, InfoOverlay};
        let mut shown: Vec<(InfoOverlay, bool)> = self
            .scope
            .info
            .iter()
            .filter_map(|o| {
                let steady = o.always || self.info_timers.held(&o.name);
                let timed = self.info_timers.timed(&o.name);
                (steady || timed).then(|| (o.clone(), steady))
            })
            .collect();
        // Each active layer's indicator: its name, or an info overlay of the game's.
        for name in self.active_layers() {
            let Some(layer) = self.scope.layers.iter().find(|l| l.name == name) else { continue };
            match &layer.indicator {
                Indicator::Name => shown.push((
                    InfoOverlay {
                        name: format!("layer {name}"),
                        always: true,
                        on_start: None,
                        linger: None,
                        current_input: Default::default(),
                        style: layer.indicator_style.clone(),
                        rows: vec![vec![name.clone()]],
                    },
                    true,
                )),
                Indicator::Info(info) => match shown.iter_mut().find(|(o, _)| &o.name == info) {
                    Some((_, steady)) => *steady = true,
                    None => {
                        if let Some(o) = self.scope.info.iter().find(|o| &o.name == info) {
                            shown.push((o.clone(), true));
                        }
                    }
                },
                Indicator::Off => {}
            }
        }
        shown
    }

    /// Layers active on any controller (the last one used first), oldest first, that the
    /// active game has.
    fn active_layers(&self) -> Vec<String> {
        let last = self.last_active.and_then(|id| self.devices.get(&id));
        let others = self.devices.iter().filter(|(id, _)| Some(**id) != self.last_active).map(|(_, d)| d);
        let mut layers: Vec<String> = Vec::new();
        for name in last.into_iter().chain(others).flat_map(|d| d.engine.layers()) {
            if !layers.contains(&name) && self.scope.layers.iter().any(|l| l.name == name) {
                layers.push(name);
            }
        }
        layers
    }

    fn live_values(&self) -> crate::info::Live {
        let window = self.focused.clone().unwrap_or_default();
        let pad = self.last_active.and_then(|id| self.devices.get(&id)).or_else(|| self.devices.values().next());
        crate::info::Live {
            profile: self.config.active().map(|p| p.name.clone()).unwrap_or_default(),
            app: window.exe,
            title: window.title,
            pid: window.pid,
            controller: pad.map(|d| d.name.clone()).unwrap_or_default(),
            layers: self.active_layers(),
            family: pad.and_then(|d| d.family).unwrap_or(self.config.info_glyphs),
            system: self.sampler.stats.clone(),
            inputs: crate::inputlog::Inputs::default(),
        }
    }

    /// Redraws info overlays if a ShowInfo action changed what is held up, or a layer (with its
    /// indicator and `{layer}`) started or ended.
    fn check_info_changes(&mut self) {
        let mut changed = false;
        for dev in self.devices.values_mut() {
            changed |= dev.engine.take_info_changed();
            changed |= dev.engine.take_layers_changed();
        }
        if changed {
            self.broadcast_overlay();
        }
    }

    /// If the overlay window died (crashed, or no compositor yet), leave overlay mode so the
    /// controller isn't stuck driving something invisible. It restarts on next use.
    fn check_overlay_process(&mut self) {
        let exited = self.overlay_process.as_mut().is_some_and(|c| c.try_wait().ok().flatten().is_some());
        if exited {
            self.overlay_process = None;
            if self.active.is_some() {
                log!("overlay window went away");
                self.close_overlay();
            }
        }
    }

    fn needs_tick(&self) -> bool {
        let Some(base) = self.config.active() else { return false };
        self.devices.values().any(|d| d.engine.needs_tick(layered(&d.layered, base)))
    }

    fn tick(&mut self, dt: f32) {
        let Some(base) = self.config.active() else { return };
        for dev in self.devices.values_mut() {
            let mut out = Vec::new();
            dev.engine.tick(layered(&dev.layered, base), dt, &mut out);
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
                    dev.engine.release_all(false, &mut out);
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
            Msg::Focus(FocusEvent::Focused(window)) => self.window_focused(&window),
            Msg::Motion { id, sample } => self.motion(id, sample),
            Msg::WatchOverlay(watcher) => {
                if watcher.send(self.overlay_frame()).is_ok() {
                    self.overlay_watchers.push(watcher);
                }
            }
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
        let Some(base) = self.config.active().filter(|_| self.active.is_none()) else { return };
        let mut out = Vec::new();
        dev.engine.motion(layered(&dev.layered, base), sample, &mut out);
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
            thread::spawn(move || read_motion(id, motion_dev, &stop, &tx));
        }
    }

    fn window_focused(&mut self, window: &WindowInfo) {
        // Editing settings while a game runs must not flip the profile.
        if focus::is_own_window(window) {
            return;
        }
        let same = |w: &WindowInfo| w.class == window.class && w.exe == window.exe && w.steam_app_id == window.steam_app_id;
        self.recent_windows.retain(|w| !same(w));
        self.recent_windows.insert(0, window.clone());
        self.recent_windows.truncate(RECENT_WINDOWS);
        self.focused = Some(window.clone());
        if self.info_refresh.is_some() {
            self.broadcast_overlay();
        }

        if !self.config.auto_switch.enabled {
            return;
        }
        match focus::profile_for(&self.config, window) {
            Some(target) => self.auto_switch_to(target, &describe(window), true),
            None => self.leave_game(&describe(window)),
        }
        if let Some((game, pid)) = focus::game_launch(&self.config, window) {
            self.launched(&game, pid);
        }
    }

    /// A rule matched `game` with process `pid`: if that's a process the game hasn't had,
    /// the game has just started.
    fn launched(&mut self, game: &str, pid: u32) {
        if self.launches.get(game) == Some(&pid) {
            return;
        }
        self.launches.insert(game.to_owned(), pid);
        self.game_started(game);
    }

    /// The active game has just started: shows its "when the game starts" info overlays and
    /// switches on toggles set to start on.
    fn game_started(&mut self, game: &str) {
        if self.config.active_ref().game.as_deref() != Some(game) {
            return;
        }
        log!("{game} started");
        self.toast_profile();
        self.info_timers.game_started(&self.scope.info, Instant::now());
        let Some(base) = self.config.active() else { return };
        for dev in self.devices.values_mut() {
            let mut out = Vec::new();
            // A toggle starting "Next profile" would be odd; it isn't followed.
            dev.engine.start_toggles(layered(&dev.layered, base), &self.scope.menus, &mut out);
            if dev.refresh_layers(&self.config) {
                dev.engine.resync(layered(&dev.layered, base), &mut out);
            }
            dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
        }
        self.check_info_changes();
        self.broadcast_overlay();
    }

    /// Process-scan fallback for desktops without focus tracking.
    fn scan_processes(&mut self) {
        let no_rules = focus::rules(&self.config).next().is_none();
        if self.focus_backend != FocusBackend::ProcessScan || !self.config.auto_switch.enabled || no_rules {
            return;
        }
        let processes = focus::running_processes();
        let target = focus::profile_for_processes(&self.config, &processes);
        if target != self.scan_target {
            self.scan_target = target.clone();
            match target {
                Some(target) => self.auto_switch_to(target, "running processes", true),
                None => self.leave_game("no game running"),
            }
        }
        if let Some((game, pid)) = focus::game_launch_in(&self.config, &processes) {
            self.launched(&game, pid);
        }
    }

    /// Switches to a profile a rule (or the default) picked, with a toast if `toast`.
    fn auto_switch_to(&mut self, target: ProfileRef, reason: &str, toast: bool) {
        if target == self.config.active_ref() {
            return;
        }
        if self.config.profile(&target).is_none() {
            log!("auto-switch points to missing profile {target}");
            return;
        }
        log!("{reason} → profile {target}");
        if toast {
            self.switch_profile(target);
        } else {
            self.set_profile(target);
        }
    }

    /// No game has focus: a profile of a game that rules pick gives way to the default one.
    /// No toast, since alt-tabbing out of a game would show one every time.
    fn leave_game(&mut self, reason: &str) {
        let in_game = self.config.active_ref().game.is_some_and(|g| focus::has_rules(&self.config, &g));
        if in_game && let Some(default) = self.config.auto_switch.default_profile.clone() {
            self.auto_switch_to(default, reason, false);
        }
    }

    /// Briefly says which controller profile is now active, and for which game, at the top
    /// of the screen.
    fn toast_profile(&mut self) {
        let active = self.config.active_ref();
        let lines = [format!("Controller profile active: {}", active.profile)]
            .into_iter()
            .chain(active.game.map(|game| format!("Game: {game}")))
            .collect();
        self.toast = Some(crate::info::Toast::new(lines, Instant::now()));
        self.broadcast_overlay();
    }

    fn broadcast(&mut self, snapshot: Option<&InputSnapshot>) {
        self.watchers.retain(|w| w.send(snapshot.cloned()).is_ok());
    }

    /// Called when a device stops being managed; watchers fall back to "no controller".
    fn forget_active(&mut self, id: u64) {
        if self.last_active == Some(id) {
            self.last_active = None;
            self.broadcast(None);
        }
    }

    fn input(&mut self, id: u64, events: Vec<InputEvent>) {
        let logged = self.log_input(id, &events, Instant::now());
        if self.active.is_some() {
            return self.overlay_input(id, events);
        }
        let Some(base) = self.config.active() else { return };
        let Some(dev) = self.devices.get_mut(&id) else { return };
        let mut out = Vec::new();
        let mut switch = false;
        let now = Instant::now();
        for ev in events {
            dev.view.apply(&ev);
            switch |= dev.engine.handle(layered(&dev.layered, base), ev, now, &mut out);
            // A layer started or ended: the next events use it, and sticks, triggers and
            // gyro switch modes right away.
            if dev.refresh_layers(&self.config) {
                dev.engine.resync(layered(&dev.layered, base), &mut out);
            }
        }
        dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
        if !dev.engine.needs_tick(layered(&dev.layered, base)) {
            dev.out_view.stop_motion();
        }
        dev.draw_status();
        self.last_draw = Instant::now();
        self.last_active = Some(id);
        if !self.watchers.is_empty() {
            let snapshot = dev.view.snapshot(&dev.name);
            self.watchers.retain(|w| w.send(Some(snapshot.clone())).is_ok());
        }
        let toggle_overlay = dev.engine.take_overlay_toggle();
        let menu_request = dev.engine.take_menu_request();
        if switch && let Some(next) = self.config.next_profile() {
            self.switch_profile(next);
        }
        if let Some(layout) = toggle_overlay {
            self.toggle_overlay(layout);
        }
        if let Some((name, opener)) = menu_request {
            self.open_menu(id, &name, opener);
        }
        if self.log_fired(id) | logged {
            self.log_changed();
        }
        self.check_info_changes();
    }

    /// While the overlay shows something, controller input drives it instead of the mappings.
    /// (Every way out redraws, so the input log's view of it shows too.)
    fn overlay_input(&mut self, id: u64, events: Vec<InputEvent>) {
        let now = Instant::now();
        if let Some(dev) = self.devices.get_mut(&id) {
            for ev in &events {
                dev.view.apply(ev);
            }
            dev.draw_status();
        }
        for ev in events {
            match &mut self.active {
                Some(Active::Keyboard(k)) => {
                    let actions = k.handle(ev, now);
                    self.log_keyboard(id, ev, &actions);
                    self.apply_keyboard(actions);
                }
                Some(Active::Menu { session, device }) => {
                    let device = *device;
                    match session.handle(&self.scope.menus, ev, now) {
                        Some(MenuOutcome::Choose { menu, item, action, close }) => {
                            self.log_menu_choice(id, ev, &action);
                            if close {
                                self.close_overlay();
                                self.run_menu_item(device, &menu, item, &action);
                                return;
                            }
                            // The menu stays up for more picks.
                            self.run_menu_item(device, &menu, item, &action);
                            if !matches!(self.active, Some(Active::Menu { .. })) {
                                return;
                            }
                        }
                        Some(MenuOutcome::Close) => return self.close_overlay(),
                        None => {}
                    }
                }
                None => return,
            }
        }
        self.broadcast_overlay();
    }

    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    fn request(&mut self, req: Request) -> Response {
        match req {
            Request::Status => Response::Status(self.status()),
            Request::GetConfig => Response::Config(Box::new(self.config.clone())),
            Request::SetConfig(new) => {
                let mut new = *new;
                new.enabled = self.config.enabled;
                new.carry_active(&self.config.active);
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
                let Some(target) = self.config.find_profile(&name) else {
                    return Response::Error(format!("no profile named {name:?}"));
                };
                self.switch_profile(target);
                Response::Ok
            }
            Request::Activate(target) => {
                if self.config.profile(&target).is_none() {
                    return Response::Error(format!("no profile {target}"));
                }
                self.switch_profile(target);
                Response::Ok
            }
            Request::NextProfile => {
                if let Some(next) = self.config.next_profile() {
                    self.switch_profile(next);
                }
                Response::Ok
            }
            // Handled by the connection thread, which registers through `Msg::Watch`.
            Request::WatchInput => Response::Error("WatchInput must be the only request".into()),
            Request::ToggleOverlay => {
                self.toggle_overlay(Layout::Keyboard);
                Response::Ok
            }
            Request::ToggleNumpad => {
                self.toggle_overlay(Layout::Numpad);
                Response::Ok
            }
            Request::OpenMenu(name) => {
                if !self.scope.menus.iter().any(|m| m.name == name) {
                    return Response::Error(format!("no menu named {name:?}"));
                }
                let device = self.last_active.or_else(|| self.devices.keys().next().copied()).unwrap_or(u64::MAX);
                self.open_menu(device, &name, Opener::default());
                Response::Ok
            }
            Request::WatchOverlay => Response::Error("WatchOverlay must be the only request".into()),
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
        if new.general.profiles.is_empty() {
            return Response::Error("General must have at least one profile".into());
        }
        // An open menu refers to menus by position, which the new config may change.
        if matches!(self.active, Some(Active::Menu { .. })) {
            self.close_overlay();
        }
        // Toggled layers stay on; they apply again if the game still has them.
        self.release_all(true);
        self.config = new;
        self.refresh_scope();
        let ignored = self.config.ignored_devices.clone();
        let enabled = self.config.enabled;
        self.release_devices(|d| !enabled || ignored.contains(&d.name));
        self.resync_all();
        self.broadcast_overlay();
        self.save_and_rescan()
    }

    fn save_and_rescan(&mut self) -> Response {
        self.scan();
        match self.config.save() {
            Ok(()) => Response::Ok,
            Err(e) => Response::Error(format!("saving config: {e:#}")),
        }
    }

    /// Recomputes what the active profile can use, after a config or game change.
    fn refresh_scope(&mut self) {
        self.scope = self.config.scope();
        for dev in self.devices.values_mut() {
            dev.engine.set_macros(&self.scope.macros);
        }
    }

    /// Switches profile, showing the new one in a toast.
    fn switch_profile(&mut self, target: ProfileRef) {
        if self.set_profile(target) {
            self.toast_profile();
        }
    }

    /// Switches profile; false if it was already active.
    fn set_profile(&mut self, target: ProfileRef) -> bool {
        if target == self.config.active_ref() {
            return false;
        }
        log!("switching to profile {target}");
        // An open menu refers to menus by position, which another game's list changes.
        let game_changes = target.game != self.config.active_ref().game;
        if game_changes {
            self.info_timers.clear();
            self.log_timers.clear();
        }
        if game_changes && matches!(self.active, Some(Active::Menu { .. })) {
            self.close_overlay();
        }
        // Layers belong to a game: toggled ones stay on within it.
        self.release_all(!game_changes);
        self.config.active = target;
        if game_changes {
            self.refresh_scope();
        }
        self.resync_all();
        // Other info overlays may belong to the new profile.
        self.broadcast_overlay();
        if let Err(e) = self.config.save() {
            log!("saving config: {e:#}");
        }
        true
    }

    /// Releases every output held by every device (before a profile change).
    fn release_all(&mut self, keep_toggled_layers: bool) {
        for dev in self.devices.values_mut() {
            let mut out = Vec::new();
            dev.engine.release_all(keep_toggled_layers, &mut out);
            dispatch(&mut dev.pad, &mut dev.out_view, &mut self.kbm, out);
        }
    }

    /// Re-applies analog state under the active profile (and each controller's layers, which
    /// are rebuilt from it).
    fn resync_all(&mut self) {
        let Some(base) = self.config.active() else { return };
        for dev in self.devices.values_mut() {
            dev.layered = None;
            dev.refresh_layers(&self.config);
            let mut out = Vec::new();
            dev.engine.resync(layered(&dev.layered, base), &mut out);
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
            dev.engine.release_all(false, &mut out);
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
            active_game: self.config.active_ref().game,
            active_layers: self.active_layers(),
            devices,
            focus_backend: self.focus_backend,
            focused: self.focused.clone(),
            recent_windows: self.recent_windows.clone(),
            motion_access_denied: self.motion_denied.values().cloned().collect(),
            overlay_visible: matches!(&self.active, Some(Active::Keyboard(k)) if k.layout() == Layout::Keyboard),
            numpad_visible: matches!(&self.active, Some(Active::Keyboard(k)) if k.layout() == Layout::Numpad),
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
        self.check_overlay_process();
        self.attach_motion();
        self.scan_processes();
        self.gamepads.retain(|k, _| present.contains(k));
    }

    fn manage(&mut self, path: PathBuf, name: String, mut dev: Device) {
        let parent = hid_parent(&path);
        let family = crate::info::PadFamily::detect(dev.input_id().vendor(), &name);
        let uniq = dev.unique_name().map(str::to_string);
        if let Err(e) = dev.grab() {
            log!("cannot grab {name} ({}): {e}", path.display());
            return;
        }
        let Some((pad, has_rumble)) = virtual_pad_for(&dev) else { return };
        log!("managing {name} ({})", path.display());
        let id = self.next_id;
        self.next_id += 1;
        let stop = Arc::new(AtomicBool::new(false));
        {
            let stop = stop.clone();
            let tx = self.tx.clone();
            let xbox_labels = input::uses_xbox_labels(&path);
            thread::spawn(move || read_device(id, dev, xbox_labels, &stop, &tx));
        }
        if has_rumble {
            rumble::spawn(pad.shared(), path.clone(), stop.clone());
        }
        let mut managed = Managed {
            path,
            name,
            family,
            engine: {
                let mut engine = Engine::default();
                engine.set_macros(&self.scope.macros);
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
            layered: None,
            log: crate::inputlog::InputLog::default(),
            number: logs::free_number(self.devices.values().map(|d| d.number)),
        };
        if let Some(profile) = self.config.active() {
            let mut out = Vec::new();
            managed.engine.resync(profile, &mut out);
            dispatch(&mut managed.pad, &mut managed.out_view, &mut self.kbm, out);
        }
        self.devices.insert(id, managed);
    }
}

/// The virtual pad a controller feeds, and whether it rumbles: it mirrors the controller's
/// rumble support so games see it exactly when it exists.
fn virtual_pad_for(dev: &Device) -> Option<(VirtualPad, bool)> {
    let ff = dev
        .supported_ff()
        .filter(|ff| ff.iter().next().is_some() && dev.max_ff_effects() > 0)
        .map(|effects| FfCaps { effects, max_effects: dev.max_ff_effects() as u32 });
    let has_rumble = ff.is_some();
    match VirtualPad::new(ff) {
        Ok(pad) => Some((pad, has_rumble)),
        Err(e) => {
            log!("cannot create virtual pad: {e:#}");
            None
        }
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

fn read_motion(id: u64, mut dev: Device, stop: &AtomicBool, tx: &Sender<Msg>) {
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

fn read_device(id: u64, mut dev: Device, xbox_labels: bool, stop: &AtomicBool, tx: &Sender<Msg>) {
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

fn layout_name(layout: Layout) -> &'static str {
    match layout {
        Layout::Keyboard => "keyboard",
        Layout::Numpad => "numpad",
    }
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

fn ipc_server(listener: &UnixListener, tx: &Sender<Msg>) {
    for conn in listener.incoming() {
        let Ok(conn) = conn else { continue };
        let tx = tx.clone();
        // One thread per client: input watchers hold their connection open.
        thread::spawn(move || {
            if let Err(e) = serve_client(&conn, &tx) {
                log!("ipc: {e:#}");
            }
        });
    }
}

fn serve_client(mut conn: &UnixStream, tx: &Sender<Msg>) -> Result<()> {
    conn.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut line = String::new();
    BufReader::new(conn).read_line(&mut line)?;
    let response = match serde_json::from_str::<Request>(&line) {
        Ok(Request::WatchInput) => return watch_input(conn, tx),
        Ok(Request::WatchOverlay) => return watch_overlay(conn, tx),
        Ok(req) => {
            let (reply_tx, reply_rx) = mpsc::channel();
            tx.send(Msg::Ipc { req, reply: reply_tx })?;
            reply_rx.recv_timeout(Duration::from_secs(2))?
        }
        Err(e) => Response::Error(format!("bad request: {e}")),
    };
    let mut out = serde_json::to_string(&response)?;
    out.push('\n');
    conn.write_all(out.as_bytes())?;
    Ok(())
}

/// Streams overlay state to the (resident) overlay window until it goes away. An empty frame
/// means nothing is shown; the window idles and keeps listening.
fn watch_overlay(mut conn: &UnixStream, tx: &Sender<Msg>) -> Result<()> {
    let (view_tx, view_rx) = mpsc::channel::<OverlayFrame>();
    tx.send(Msg::WatchOverlay(view_tx))?;
    while let Ok(mut view) = view_rx.recv() {
        while let Ok(newer) = view_rx.try_recv() {
            view = newer;
        }
        let mut line = serde_json::to_string(&view)?;
        line.push('\n');
        if conn.write_all(line.as_bytes()).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

/// Streams snapshots to a client until it disconnects. Bursts are coalesced to the latest
/// snapshot, so a fast-polling pad never floods the GUI.
fn watch_input(mut conn: &UnixStream, tx: &Sender<Msg>) -> Result<()> {
    let (snap_tx, snap_rx) = mpsc::channel();
    tx.send(Msg::Watch(snap_tx))?;
    // Blocks until the next update; ends when the daemon drops this watcher.
    while let Ok(mut snapshot) = snap_rx.recv() {
        while let Ok(newer) = snap_rx.try_recv() {
            snapshot = newer;
        }
        let mut line = serde_json::to_string(&snapshot)?;
        line.push('\n');
        if conn.write_all(line.as_bytes()).is_err() {
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

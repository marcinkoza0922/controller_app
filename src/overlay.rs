//! On-screen overlay: a keyboard you type on with the controller, and action menus. The
//! daemon owns the state (`OverlayController` here, `menu::MenuSession`) and routes
//! controller input to it while something is shown; the overlay process
//! (`padwight overlay`) only draws it, on a Wayland layer-shell surface that never
//! takes keyboard focus, so typed keys reach the window underneath. The process stays
//! running as an invisible 1×1 surface between uses so menus appear instantly.

use std::{
    collections::HashSet,
    str::FromStr,
    time::{Duration, Instant},
};

use evdev::KeyCode;
use serde::{Deserialize, Serialize};

use crate::{
    config::Button,
    input::{Axis, InputEvent},
    keyboard::{self, Cursor, Layout},
};

/// Holding East this long closes the overlay.
pub const HOLD_TO_CLOSE: Duration = Duration::from_millis(700);
/// Holding a direction moves once, then repeats after this delay...
const REPEAT_DELAY: Duration = Duration::from_millis(350);
/// ...this often.
const REPEAT_EVERY: Duration = Duration::from_millis(90);
/// Left trigger pull that holds Shift on the keyboard, and where it lets go.
const SHIFT_PRESS: f32 = 0.5;
const SHIFT_RELEASE: f32 = 0.3;
/// Stick deflection that counts as a direction, and where it lets go.
const STICK_PRESS: f32 = 0.6;
const STICK_RELEASE: f32 = 0.45;

const MODIFIERS: [&str; 8] = [
    "KEY_LEFTSHIFT",
    "KEY_RIGHTSHIFT",
    "KEY_LEFTCTRL",
    "KEY_RIGHTCTRL",
    "KEY_LEFTALT",
    "KEY_RIGHTALT",
    "KEY_LEFTMETA",
    "KEY_RIGHTMETA",
];

/// What the overlay process draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OverlayView {
    Keyboard(KeyboardView),
    Menu(crate::menu::MenuView),
    Media(crate::media::MediaView),
    Offer(crate::offer::OfferView),
}

/// Everything the overlay window shows at once: info overlays, plus the keyboard or a menu.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OverlayFrame {
    pub info: Vec<crate::info::InfoView>,
    /// Absent from an older daemon's frames.
    #[serde(default)]
    pub logs: Vec<crate::inputlog::LogView>,
    pub active: Option<OverlayView>,
    /// The font everything here is drawn in; the system's when unset.
    #[serde(default)]
    pub font: Option<String>,
    /// Whether menu rows are tinted in colors color-blind people can tell apart. An older daemon
    /// spells it `colourblind`.
    #[serde(default, alias = "colourblind")]
    pub colorblind: bool,
    /// The controller in use, whose button glyphs menus draw as the info overlays do.
    #[serde(default)]
    pub family: crate::info::PadFamily,
    /// Whether menus draw their button glyphs in the Nintendo layout.
    #[serde(default)]
    pub nintendo_layout: bool,
    /// How each kind of overlay moves as it opens, closes, and as the cursor and picks move.
    #[serde(default)]
    pub motion: crate::motion::MotionSet,
}

/// The on-screen keyboard's (or numpad's) state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyboardView {
    #[serde(default)]
    pub layout: Layout,
    /// Set by the daemon from the config.
    #[serde(default = "crate::config::OverlayStyle::keyboard")]
    pub style: crate::config::OverlayStyle,
    pub cursor: Cursor,
    /// Modifiers latched for the next key (evdev names).
    pub latched: Vec<String>,
    /// A key currently held down with A.
    pub pressed: Option<String>,
    /// 0..1 while East is being held to close.
    pub closing: f32,
}

/// Where a key sits in its layout, counting every slot in reading order: the number the motion
/// tracking and the drawing both use for the cursor and the pressed key.
fn key_index(layout: Layout, code: &str) -> Option<u32> {
    layout.rows().iter().flat_map(|keys| keys.iter()).position(|key| key.code == code).map(|i| i as u32)
}

impl crate::motion::Tracked for OverlayView {
    fn key(&self) -> String {
        match self {
            OverlayView::Keyboard(k) => if k.layout == Layout::Numpad { "numpad" } else { "keyboard" }.into(),
            OverlayView::Menu(m) => format!("{}\u{0}{}", m.crumbs.join(" › "), m.title),
            OverlayView::Media(_) => "media".into(),
            OverlayView::Offer(_) => "offer".into(),
        }
    }

    fn kind(&self) -> crate::motion::OverlayKind {
        use crate::motion::OverlayKind;
        match self {
            OverlayView::Keyboard(k) if k.layout == Layout::Numpad => OverlayKind::Numpad,
            OverlayView::Keyboard(_) => OverlayKind::Keyboard,
            OverlayView::Menu(_) => OverlayKind::Menu,
            OverlayView::Media(_) => OverlayKind::Media,
            OverlayView::Offer(_) => OverlayKind::Offer,
        }
    }

    fn cursor(&self) -> Option<u32> {
        match self {
            OverlayView::Keyboard(k) => key_index(k.layout, keyboard::key_at(k.layout, k.cursor).code),
            OverlayView::Menu(m) => m.selected.map(|i| i as u32),
            OverlayView::Offer(o) => (!o.choices.is_empty()).then_some(o.selected as u32),
            OverlayView::Media(_) => None,
        }
    }

    fn pick(&self) -> Option<u32> {
        match self {
            OverlayView::Keyboard(k) => k.pressed.as_deref().and_then(|code| key_index(k.layout, code)),
            OverlayView::Menu(m) => (m.picks > 0).then_some(m.picks),
            OverlayView::Offer(_) | OverlayView::Media(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OverlayAction {
    Key(KeyCode, bool),
    Close,
}

/// Turns controller input into keyboard navigation and key presses.
pub struct OverlayController {
    layout: Layout,
    cursor: Cursor,
    latched: Vec<KeyCode>,
    pressed: Option<KeyCode>,
    dpad: HashSet<Button>,
    stick: (f32, f32),
    stick_dir: Option<(i32, i32)>,
    /// Direction being held and when it next repeats.
    repeat: Option<((i32, i32), Instant)>,
    east_since: Option<Instant>,
    /// Guide is down: Guide + X closes the keyboard (Guide + Y the numpad), as it opened it.
    guide_held: bool,
    /// Shift held down by the left trigger.
    trigger_shift: bool,
}

fn code(name: &str) -> Option<KeyCode> {
    KeyCode::from_str(name).ok()
}

fn tap(name: &str) -> Vec<OverlayAction> {
    code(name).map(|k| vec![OverlayAction::Key(k, true), OverlayAction::Key(k, false)]).unwrap_or_default()
}

impl OverlayController {
    pub fn new(layout: Layout, cursor: Cursor) -> Self {
        OverlayController {
            layout,
            cursor,
            latched: Vec::new(),
            pressed: None,
            dpad: HashSet::new(),
            stick: (0.0, 0.0),
            stick_dir: None,
            repeat: None,
            east_since: None,
            guide_held: false,
            trigger_shift: false,
        }
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn view(&self, now: Instant) -> KeyboardView {
        let name = |k: KeyCode| format!("{k:?}");
        KeyboardView {
            layout: self.layout,
            style: crate::config::OverlayStyle::keyboard(),
            cursor: self.cursor,
            latched: self.latched.iter().chain(self.trigger_shift.then_some(&KeyCode::KEY_LEFTSHIFT)).map(|k| name(*k)).collect(),
            pressed: self.pressed.map(name),
            closing: self
                .east_since
                .map(|t| (now.duration_since(t).as_secs_f32() / HOLD_TO_CLOSE.as_secs_f32()).min(1.0))
                .unwrap_or(0.0),
        }
    }

    /// Tells the controller Guide is already down as it opens (the chord that opened it).
    pub fn set_guide_held(&mut self, held: bool) {
        self.guide_held = held;
    }

    pub fn handle(&mut self, ev: InputEvent, now: Instant) -> Vec<OverlayAction> {
        match ev {
            InputEvent::Button(b, pressed) => self.button(b, pressed, now),
            InputEvent::Axis(Axis::LeftX, v) => {
                self.stick.0 = v;
                self.stick_moved(now);
                Vec::new()
            }
            InputEvent::Axis(Axis::LeftY, v) => {
                self.stick.1 = v;
                self.stick_moved(now);
                Vec::new()
            }
            InputEvent::Axis(Axis::LeftTrigger, v) if self.layout == Layout::Keyboard => self.left_trigger(v),
            InputEvent::Axis(..) | InputEvent::Touchpad(_) => Vec::new(),
        }
    }

    /// Holding the left trigger holds Shift (alongside any latched modifiers).
    fn left_trigger(&mut self, value: f32) -> Vec<OverlayAction> {
        let shift = KeyCode::KEY_LEFTSHIFT;
        let latched = self.latched.contains(&shift);
        if !self.trigger_shift && value >= SHIFT_PRESS {
            self.trigger_shift = true;
            // Already down if latched; still mark it so letting go of the trigger is quiet.
            if !latched {
                return vec![OverlayAction::Key(shift, true)];
            }
        } else if self.trigger_shift && value < SHIFT_RELEASE {
            self.trigger_shift = false;
            if !latched {
                return vec![OverlayAction::Key(shift, false)];
            }
        }
        Vec::new()
    }

    fn button(&mut self, b: Button, pressed: bool, now: Instant) -> Vec<OverlayAction> {
        let dir = match b {
            Button::DpadUp => Some((0, -1)),
            Button::DpadDown => Some((0, 1)),
            Button::DpadLeft => Some((-1, 0)),
            Button::DpadRight => Some((1, 0)),
            _ => None,
        };
        if let Some(dir) = dir {
            if pressed && self.dpad.insert(b) {
                self.start_moving(dir, now);
            } else if !pressed && self.dpad.remove(&b) && self.repeat.is_some_and(|(d, _)| d == dir) {
                self.repeat = None;
            }
            return Vec::new();
        }
        let closes_with = if self.layout == Layout::Keyboard { Button::West } else { Button::North };
        match (b, pressed) {
            (Button::Guide, _) => {
                self.guide_held = pressed;
                Vec::new()
            }
            (b, true) if b == closes_with && self.guide_held => vec![OverlayAction::Close],
            (Button::South, true) => self.press_selected(),
            (Button::South, false) => self.release_selected(),
            (Button::West, true) => self.shortcut("KEY_BACKSPACE"),
            (Button::North, true) if self.layout == Layout::Keyboard => self.shortcut("KEY_SPACE"),
            (Button::Start, true) => self.shortcut("KEY_ENTER"),
            (Button::East, true) => {
                self.east_since = Some(now);
                Vec::new()
            }
            (Button::East, false) => {
                self.east_since = None;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The stick acts like a D-pad: its strongest direction past the threshold.
    fn stick_moved(&mut self, now: Instant) {
        let (x, y) = self.stick;
        let threshold = if self.stick_dir.is_some() { STICK_RELEASE } else { STICK_PRESS };
        let dir = if x.abs().max(y.abs()) < threshold {
            None
        } else if x.abs() > y.abs() {
            Some((x.signum() as i32, 0))
        } else {
            Some((0, y.signum() as i32))
        };
        if dir == self.stick_dir {
            return;
        }
        self.stick_dir = dir;
        match dir {
            Some(d) => self.start_moving(d, now),
            None => self.repeat = None,
        }
    }

    fn start_moving(&mut self, dir: (i32, i32), now: Instant) {
        self.cursor = keyboard::step(self.layout, self.cursor, dir.0, dir.1);
        self.repeat = Some((dir, now + REPEAT_DELAY));
    }

    fn press_selected(&mut self) -> Vec<OverlayAction> {
        let name = keyboard::key_at(self.layout, self.cursor).code;
        let Some(k) = code(name) else { return Vec::new() };
        // Modifiers latch (held down) until the next key, like sticky keys.
        if MODIFIERS.contains(&name) {
            if let Some(i) = self.latched.iter().position(|l| *l == k) {
                self.latched.remove(i);
                return vec![OverlayAction::Key(k, false)];
            }
            self.latched.push(k);
            return vec![OverlayAction::Key(k, true)];
        }
        if self.pressed.is_some() {
            return Vec::new();
        }
        self.pressed = Some(k);
        vec![OverlayAction::Key(k, true)]
    }

    fn release_selected(&mut self) -> Vec<OverlayAction> {
        let Some(k) = self.pressed.take() else { return Vec::new() };
        let mut out = vec![OverlayAction::Key(k, false)];
        out.extend(self.release_latched());
        out
    }

    fn shortcut(&mut self, name: &str) -> Vec<OverlayAction> {
        let mut out = tap(name);
        out.extend(self.release_latched());
        out
    }

    fn release_latched(&mut self) -> Vec<OverlayAction> {
        // Shift stays down while the trigger holds it.
        let held = self.trigger_shift.then_some(KeyCode::KEY_LEFTSHIFT);
        self.latched.drain(..).rev().filter(|k| Some(*k) != held).map(|k| OverlayAction::Key(k, false)).collect()
    }

    /// Key repeat for held directions and hold-to-close. Returns what happened and whether
    /// the view changed.
    pub fn tick(&mut self, now: Instant) -> (Vec<OverlayAction>, bool) {
        let mut changed = self.east_since.is_some();
        if let Some(since) = self.east_since
            && now.duration_since(since) >= HOLD_TO_CLOSE
        {
            self.east_since = None;
            return (vec![OverlayAction::Close], true);
        }
        if let Some((dir, mut next)) = self.repeat {
            while now >= next {
                self.cursor = keyboard::step(self.layout, self.cursor, dir.0, dir.1);
                next += REPEAT_EVERY;
                changed = true;
            }
            self.repeat = Some((dir, next));
        }
        (Vec::new(), changed)
    }

    /// When `tick` next has work (key repeat or the close countdown).
    pub fn next_deadline(&self) -> Option<Instant> {
        let close = self.east_since.map(|t| t + HOLD_TO_CLOSE);
        let repeat = self.repeat.map(|(_, t)| t);
        // While East is held the closing bar animates, so keep ticking.
        let animate = self.east_since.map(|_| Instant::now() + Duration::from_millis(30));
        [close, repeat, animate].into_iter().flatten().min()
    }

    /// Lets go of everything the overlay is holding (when it closes).
    pub fn release_all(&mut self) -> Vec<OverlayAction> {
        let mut out: Vec<OverlayAction> = self.pressed.take().map(|k| OverlayAction::Key(k, false)).into_iter().collect();
        out.extend(self.release_latched());
        if std::mem::take(&mut self.trigger_shift) {
            out.push(OverlayAction::Key(KeyCode::KEY_LEFTSHIFT, false));
        }
        self.repeat = None;
        self.east_since = None;
        self.dpad.clear();
        self.stick_dir = None;
        out
    }
}


/// Runs the overlay window: a layer-shell surface at the bottom of the screen drawing the
/// state the daemon streams, and exiting when the daemon hides it.
pub fn run() -> anyhow::Result<()> {
    // iced_layershell unwraps when the compositor has no layer-shell (GNOME): report that
    // as an error instead of a panic and backtrace.
    std::panic::set_hook(Box::new(|_| {}));
    std::panic::catch_unwind(ui::run).unwrap_or_else(|panic| {
        let why = panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(ToString::to_string));
        anyhow::bail!(
            "the overlay window could not start (the compositor probably lacks wlr-layer-shell, as GNOME does): {}",
            why.unwrap_or_default()
        )
    })
}

mod ui {
    use std::time::{Duration, Instant};

    use iced::{
        Color, Element, Size, Subscription, Task,
        futures::{SinkExt, channel::mpsc},
        widget::{column, responsive, space, stack},
    };
    use iced_layershell::{
        reexport::{Anchor, KeyboardInteractivity, Layer},
        settings::{LayerShellSettings, Settings, StartMode},
        to_layer_message,
    };
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::{OverlayFrame, OverlayView, draw, fit::Fit};
    use crate::{
        config::ScreenPosition,
        info::InfoView,
        inputlog::LogView,
        ipc::{self, Request},
        motion::{Anim, Presence},
    };

    #[to_layer_message]
    #[derive(Debug, Clone)]
    enum Message {
        State(OverlayFrame),
        /// The daemon went away: nothing left to draw for.
        Disconnected,
        /// A frame of animation.
        Tick,
    }

    /// What is on screen, and what is fading out, each with its own timing.
    struct Overlay {
        frame: OverlayFrame,
        panels: Presence<OverlayView>,
        info: Presence<InfoView>,
        logs: Presence<LogView>,
    }

    impl Overlay {
        fn new() -> Self {
            Overlay { frame: OverlayFrame::default(), panels: Presence::single(), info: Presence::default(), logs: Presence::default() }
        }

        /// Whether anything is drawn: something shown, or something still fading out.
        fn shown(&self) -> bool {
            !self.panels.is_empty() || !self.info.is_empty() || !self.logs.is_empty()
        }

        /// Whether something is still moving at `now`, so the screen needs redrawing.
        fn busy(&self, now: Instant) -> bool {
            let motion = &self.frame.motion;
            self.panels.busy(now, motion) || self.info.busy(now, motion) || self.logs.busy(now, motion)
        }

        /// Drops what has finished closing.
        fn prune(&mut self, now: Instant) {
            let motion = &self.frame.motion;
            self.panels.prune(now, motion);
            self.info.prune(now, motion);
            self.logs.prune(now, motion);
        }
    }

    /// Idle: a 1×1 invisible surface in a corner. Showing: the whole screen, transparent
    /// except for what is drawn, with clicks passing through.
    fn idle() -> (Anchor, (u32, u32)) {
        (Anchor::Top | Anchor::Right, (1, 1))
    }

    fn full_screen() -> (Anchor, (u32, u32)) {
        (Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right, (0, 0))
    }

    pub fn run() -> anyhow::Result<()> {
        let (anchor, size) = idle();
        let app = iced_layershell::application(boot, namespace, update, view)
            .style(|_, _| iced::theme::Style { background_color: Color::TRANSPARENT, text_color: Color::WHITE });
        crate::font::BUNDLED
            .iter()
            .fold(app, |app, b| app.font(b.bytes))
            .subscription(subscription)
            .settings(Settings {
                layer_settings: LayerShellSettings {
                    size: Some(size),
                    exclusive_zone: -1,
                    anchor,
                    layer: Layer::Overlay,
                    margin: (0, 0, 0, 0),
                    // Never take keyboard focus: typed keys must reach the window underneath.
                    keyboard_interactivity: KeyboardInteractivity::None,
                    events_transparent: true,
                    start_mode: StartMode::Active,
                },
                ..Default::default()
            })
            .run()
            .map_err(|e| anyhow::anyhow!("overlay: {e}"))
    }

    fn boot() -> (Overlay, Task<Message>) {
        (Overlay::new(), Task::none())
    }

    fn namespace() -> String {
        "padwight_overlay".into()
    }

    fn update(state: &mut Overlay, message: Message) -> Task<Message> {
        match message {
            Message::State(frame) => {
                timing_log(&format!("recv {}", unix_ns()));
                let now = Instant::now();
                let was_shown = state.shown();
                state.panels.update(now, frame.active.iter().cloned().collect());
                state.info.update(now, frame.info.clone());
                state.logs.update(now, frame.logs.clone());
                state.frame = frame;
                state.prune(now);
                return resize(was_shown, state.shown());
            }
            Message::Tick => {
                let was_shown = state.shown();
                state.prune(Instant::now());
                return resize(was_shown, state.shown());
            }
            Message::Disconnected => return iced::exit(),
            _ => {}
        }
        Task::none()
    }

    /// Goes to the whole screen when something starts to show, and back to the corner once
    /// nothing is left to draw.
    fn resize(was_shown: bool, shown: bool) -> Task<Message> {
        let (anchor, size) = match (was_shown, shown) {
            (false, true) => full_screen(),
            (true, false) => idle(),
            _ => return Task::none(),
        };
        Task::done(Message::AnchorSizeChange(anchor, size))
    }

    fn subscription(state: &Overlay) -> Subscription<Message> {
        let watch = Subscription::run(watch);
        if state.busy(Instant::now()) {
            Subscription::batch([watch, iced::time::every(Duration::from_millis(16)).map(|_| Message::Tick)])
        } else {
            watch
        }
    }

    fn watch() -> impl iced::futures::Stream<Item = Message> {
        iced::stream::channel(16, async |mut output: mpsc::Sender<Message>| {
            let mut request = serde_json::to_string(&Request::WatchOverlay).unwrap_or_default();
            request.push('\n');
            if let Ok(mut stream) = tokio::net::UnixStream::connect(ipc::socket_path()).await
                && stream.write_all(request.as_bytes()).await.is_ok()
            {
                let mut lines = BufReader::new(stream).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Ok(state) = serde_json::from_str::<OverlayFrame>(&line) {
                        let _ = output.send(Message::State(state)).await;
                    }
                }
            }
            let _ = output.send(Message::Disconnected).await;
            // Keep the stream alive until iced exits.
            tokio::time::sleep(Duration::from_secs(3600)).await;
        })
    }

    fn view(state: &Overlay) -> Element<'_, Message> {
        if !state.shown() {
            return space().into();
        }
        // The surface covers the whole display, so each overlay sizes itself as shares of it.
        responsive(move |display| {
            let started = Instant::now();
            let built: Element<'_, Message> = stack(layers(state, display)).into();
            timing_log(&format!("built {} {}", unix_ns(), started.elapsed().as_nanos()));
            built
        })
        .into()
    }

    /// Profiling only. When `PADWIGHT_OVERLAY_TIMING` names a file, each received frame and each
    /// build of the overlay's widgets is appended to it, as `recv <ns>` and `built <ns> <build ns>`.
    /// Unset, this does nothing; the file is opened per call, so it is only for measuring.
    fn timing_log(entry: &str) {
        use std::io::Write;
        let Some(path) = std::env::var_os("PADWIGHT_OVERLAY_TIMING") else { return };
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{entry}");
        }
    }

    fn unix_ns() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
    }

    /// The overlays on screen, sized for a display of `display`.
    fn layers(state: &Overlay, display: Size) -> Vec<Element<'_, Message>> {
        let now = Instant::now();
        let motion = &state.frame.motion;
        let font = crate::font::resolve(state.frame.font.as_deref());
        // Info and log overlays sharing a spot stack up there; the keyboard or a menu goes on top.
        let mut layers: Vec<Element<'_, Message>> = Vec::new();
        for spot in ScreenPosition::GRID {
            let infos: Vec<_> = state.info.shown(now, motion).filter(|(v, _)| v.style.position == spot).collect();
            let logs: Vec<_> = state.logs.shown(now, motion).filter(|(v, _)| v.style.position == spot).collect();
            let style = infos.first().map(|(v, _)| &v.style).or_else(|| logs.first().map(|(v, _)| &v.style));
            if let Some(style) = style {
                let panels = infos
                    .iter()
                    .map(|(v, anim)| draw::info_panel(v, font, &Fit::of(&v.style, display), anim))
                    .chain(logs.iter().map(|(v, anim)| draw::log_panel(v, font, &Fit::of(&v.style, display), anim)))
                    .collect::<Vec<Element<'_, Message>>>();
                let fit = Fit::of(style, display);
                layers.push(draw::place(column(panels).spacing(12).into(), style, &fit, &Anim::still()));
            }
        }
        for (view, anim) in state.panels.shown(now, motion) {
            layers.push(match view {
                OverlayView::Keyboard(k) => {
                    let fit = Fit::of(&k.style, display);
                    draw::place(draw::keyboard_panel(k, font, &fit, &anim), &k.style, &fit, &anim)
                }
                OverlayView::Menu(m) => {
                    let look = draw::MenuLook {
                        colorblind: state.frame.colorblind,
                        family: state.frame.family,
                        nintendo_layout: state.frame.nintendo_layout,
                        anim,
                    };
                    let fit = Fit::of(&m.style, display);
                    draw::place(draw::menu_panel(m, font, look, &fit), &m.style, &fit, &anim)
                }
                OverlayView::Media(m) => {
                    let fit = Fit::of(&m.style, display);
                    draw::place(draw::media_panel(m, font, &fit, &anim), &m.style, &fit, &anim)
                }
                OverlayView::Offer(o) => {
                    let fit = Fit::of(&o.style, display);
                    draw::place(draw::offer_panel(o, font, &fit, &anim), &o.style, &fit, &anim)
                }
            });
        }
        layers
    }

    #[cfg(test)]
    mod motion_tests {
        use super::*;
        use crate::{config::MenuKind, menu::MenuView, motion::MotionStyle};

        fn menu_frame(motion: MotionStyle) -> OverlayFrame {
            let view = MenuView {
                title: "Belt".into(),
                kind: MenuKind::List,
                items: Vec::new(),
                selected: Some(0),
                picks: 0,
                crumbs: Vec::new(),
                hint: String::new(),
                style: Default::default(),
            };
            OverlayFrame {
                active: Some(OverlayView::Menu(view)),
                motion: crate::motion::MotionSet { menu: motion, ..Default::default() },
                ..Default::default()
            }
        }

        #[test]
        fn a_menu_that_closes_stays_up_for_its_close() {
            let mut state = Overlay::new();
            let _ = update(&mut state, Message::State(menu_frame(MotionStyle::Subtle)));
            assert!(state.shown());
            let _ = update(&mut state, Message::State(OverlayFrame { motion: menu_frame(MotionStyle::Subtle).motion, ..Default::default() }));
            assert!(state.shown(), "fading out, so the surface stays up");
            assert!(state.busy(Instant::now()));
        }

        #[test]
        fn with_motion_off_a_closed_menu_goes_at_once() {
            let mut state = Overlay::new();
            let _ = update(&mut state, Message::State(menu_frame(MotionStyle::Off)));
            let closing = OverlayFrame { motion: menu_frame(MotionStyle::Off).motion, ..Default::default() };
            let _ = update(&mut state, Message::State(closing));
            assert!(!state.shown());
        }
    }
}

/// Drawing for overlays, generic over the message type so the settings GUI can show the
/// same thing as a live preview.
pub mod draw;
pub mod fit;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_from_an_older_daemon_still_load() {
        let frame: OverlayFrame = serde_json::from_str(r#"{"info":[],"active":null}"#).unwrap();
        assert!(frame.logs.is_empty() && frame.info.is_empty() && frame.active.is_none());
        assert!(frame.motion.is_default(), "older frames have no motion, so the default");
    }

    fn at(c: &OverlayController) -> &'static str {
        keyboard::key_at(Layout::Keyboard, c.cursor()).code
    }

    fn ms(t: Instant, n: u64) -> Instant {
        t + Duration::from_millis(n)
    }

    fn start() -> OverlayController {
        OverlayController::new(Layout::Keyboard, keyboard::find(Layout::Keyboard, "KEY_Q").unwrap())
    }

    #[test]
    fn dpad_moves_once_then_repeats_while_held() {
        let mut c = start();
        let t0 = Instant::now();
        c.handle(InputEvent::Button(Button::DpadRight, true), t0);
        assert_eq!(at(&c), "KEY_W");
        assert!(!c.tick(ms(t0, 300)).1, "no repeat before the delay");
        c.tick(ms(t0, 350));
        assert_eq!(at(&c), "KEY_E");
        c.tick(ms(t0, 350 + 90 * 2));
        assert_eq!(at(&c), "KEY_T");
        c.handle(InputEvent::Button(Button::DpadRight, false), ms(t0, 600));
        c.tick(ms(t0, 2000));
        assert_eq!(at(&c), "KEY_T", "stops when released");
    }

    #[test]
    fn stick_moves_like_a_dpad_with_hysteresis() {
        let mut c = start();
        let t0 = Instant::now();
        c.handle(InputEvent::Axis(Axis::LeftY, 0.3), t0);
        assert_eq!(at(&c), "KEY_Q", "a light push does nothing");
        c.handle(InputEvent::Axis(Axis::LeftY, 0.9), t0);
        assert_eq!(at(&c), "KEY_A");
        c.handle(InputEvent::Axis(Axis::LeftY, 0.5), t0);
        assert_eq!(at(&c), "KEY_A", "still held inside the release margin");
        c.handle(InputEvent::Axis(Axis::LeftY, 0.0), t0);
        c.handle(InputEvent::Axis(Axis::LeftY, 0.9), t0);
        assert_eq!(at(&c), "KEY_Z", "a fresh push moves again");
    }

    #[test]
    fn a_holds_the_selected_key_and_modifiers_latch_for_one_key() {
        let mut c = OverlayController::new(Layout::Keyboard, keyboard::find(Layout::Keyboard, "KEY_LEFTSHIFT").unwrap());
        let t0 = Instant::now();
        let shift = KeyCode::KEY_LEFTSHIFT;
        assert_eq!(c.handle(InputEvent::Button(Button::South, true), t0), vec![OverlayAction::Key(shift, true)]);
        assert!(c.handle(InputEvent::Button(Button::South, false), t0).is_empty(), "shift stays latched");
        assert_eq!(c.view(t0).latched, ["KEY_LEFTSHIFT"]);

        c.handle(InputEvent::Button(Button::DpadRight, true), t0);
        c.handle(InputEvent::Button(Button::DpadRight, false), t0);
        assert_eq!(at(&c), "KEY_Z");
        assert_eq!(c.handle(InputEvent::Button(Button::South, true), t0), vec![OverlayAction::Key(KeyCode::KEY_Z, true)]);
        assert_eq!(
            c.handle(InputEvent::Button(Button::South, false), t0),
            vec![OverlayAction::Key(KeyCode::KEY_Z, false), OverlayAction::Key(shift, false)],
            "capital Z, then shift lets go"
        );
        assert!(c.view(t0).latched.is_empty());
    }

    #[test]
    fn left_trigger_holds_shift() {
        let mut c = start();
        let t0 = Instant::now();
        let shift = KeyCode::KEY_LEFTSHIFT;
        assert_eq!(c.handle(InputEvent::Axis(Axis::LeftTrigger, 0.8), t0), vec![OverlayAction::Key(shift, true)]);
        assert!(c.handle(InputEvent::Axis(Axis::LeftTrigger, 0.9), t0).is_empty());
        assert_eq!(c.view(t0).latched, ["KEY_LEFTSHIFT"]);
        // Typing keeps it held: capitals until the trigger lets go.
        c.handle(InputEvent::Button(Button::South, true), t0);
        assert_eq!(c.handle(InputEvent::Button(Button::South, false), t0), vec![OverlayAction::Key(KeyCode::KEY_Q, false)]);
        assert_eq!(c.handle(InputEvent::Axis(Axis::LeftTrigger, 0.1), t0), vec![OverlayAction::Key(shift, false)]);
        assert!(c.view(t0).latched.is_empty());
        // Closing while held lets go of it.
        c.handle(InputEvent::Axis(Axis::LeftTrigger, 1.0), t0);
        assert_eq!(c.release_all(), vec![OverlayAction::Key(shift, false)]);
        // Not on the numpad.
        let mut n = OverlayController::new(Layout::Numpad, Layout::Numpad.home());
        assert!(n.handle(InputEvent::Axis(Axis::LeftTrigger, 1.0), t0).is_empty());
    }

    #[test]
    fn face_button_shortcuts() {
        let mut c = start();
        let t0 = Instant::now();
        let tap = |k| vec![OverlayAction::Key(k, true), OverlayAction::Key(k, false)];
        assert_eq!(c.handle(InputEvent::Button(Button::West, true), t0), tap(KeyCode::KEY_BACKSPACE));
        assert_eq!(c.handle(InputEvent::Button(Button::North, true), t0), tap(KeyCode::KEY_SPACE));
        assert_eq!(c.handle(InputEvent::Button(Button::Start, true), t0), tap(KeyCode::KEY_ENTER));
    }

    #[test]
    fn guide_with_x_closes_the_keyboard_and_with_y_the_numpad() {
        let t0 = Instant::now();
        let mut keyboard = OverlayController::new(Layout::Keyboard, Layout::Keyboard.home());
        keyboard.handle(InputEvent::Button(Button::Guide, true), t0);
        assert_eq!(keyboard.handle(InputEvent::Button(Button::West, true), t0), vec![OverlayAction::Close]);
        let mut keyboard = OverlayController::new(Layout::Keyboard, Layout::Keyboard.home());
        assert_eq!(keyboard.handle(InputEvent::Button(Button::West, true), t0).len(), 2, "X alone is backspace");

        let mut numpad = OverlayController::new(Layout::Numpad, Layout::Numpad.home());
        numpad.set_guide_held(true);
        assert!(!numpad.handle(InputEvent::Button(Button::West, true), t0).contains(&OverlayAction::Close), "X doesn't close the numpad");
        assert_eq!(numpad.handle(InputEvent::Button(Button::North, true), t0), vec![OverlayAction::Close]);
        numpad.handle(InputEvent::Button(Button::Guide, false), t0);
        assert!(numpad.handle(InputEvent::Button(Button::North, true), t0).is_empty());
    }

    #[test]
    fn numpad_types_digits_and_enter_and_closes_with_b() {
        let mut c = OverlayController::new(Layout::Numpad, Layout::Numpad.home());
        let t0 = Instant::now();
        let tap = |k| vec![OverlayAction::Key(k, true), OverlayAction::Key(k, false)];
        c.handle(InputEvent::Button(Button::DpadUp, true), t0);
        c.handle(InputEvent::Button(Button::DpadUp, false), t0);
        assert_eq!(c.handle(InputEvent::Button(Button::South, true), t0), vec![OverlayAction::Key(KeyCode::KEY_8, true)]);
        assert_eq!(c.handle(InputEvent::Button(Button::South, false), t0), vec![OverlayAction::Key(KeyCode::KEY_8, false)]);
        assert_eq!(c.handle(InputEvent::Button(Button::Start, true), t0), tap(KeyCode::KEY_ENTER));
        assert!(c.handle(InputEvent::Button(Button::North, true), t0).is_empty(), "no space bar on a numpad");
        assert_eq!(c.view(t0).layout, Layout::Numpad);
        c.handle(InputEvent::Button(Button::East, true), t0);
        assert_eq!(c.tick(ms(t0, 700)).0, vec![OverlayAction::Close]);
    }

    #[test]
    fn holding_east_closes_but_a_tap_does_not() {
        let mut c = start();
        let t0 = Instant::now();
        c.handle(InputEvent::Button(Button::East, true), t0);
        assert!(c.view(ms(t0, 350)).closing > 0.4);
        c.handle(InputEvent::Button(Button::East, false), ms(t0, 400));
        assert_eq!(c.tick(ms(t0, 2000)).0, vec![]);
        assert_eq!(c.view(ms(t0, 2000)).closing, 0.0);

        c.handle(InputEvent::Button(Button::East, true), ms(t0, 3000));
        assert_eq!(c.next_deadline().map(|d| d <= ms(t0, 3700)), Some(true));
        assert_eq!(c.tick(ms(t0, 3700)).0, vec![OverlayAction::Close]);
    }

    #[test]
    fn closing_releases_everything_held() {
        let mut c = OverlayController::new(Layout::Keyboard, keyboard::find(Layout::Keyboard, "KEY_LEFTCTRL").unwrap());
        let t0 = Instant::now();
        c.handle(InputEvent::Button(Button::South, true), t0);
        c.handle(InputEvent::Button(Button::South, false), t0);
        c.handle(InputEvent::Button(Button::DpadUp, true), t0);
        assert!(at(&c) != "KEY_LEFTCTRL");
        c.handle(InputEvent::Button(Button::South, true), t0);
        let out = c.release_all();
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(out.contains(&OverlayAction::Key(KeyCode::KEY_LEFTCTRL, false)));
        assert_eq!(c.next_deadline(), None);
    }
}

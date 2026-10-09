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
    /// Whether menu rows are tinted in colours colour-blind people can tell apart.
    #[serde(default)]
    pub colourblind: bool,
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
        Color, Element, Subscription, Task,
        futures::{SinkExt, channel::mpsc},
        widget::{column, space, stack},
    };
    use iced_layershell::{
        reexport::{Anchor, KeyboardInteractivity, Layer},
        settings::{LayerShellSettings, Settings, StartMode},
        to_layer_message,
    };
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::{OverlayFrame, OverlayView, draw};
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
                    .map(|(v, anim)| draw::info_panel(v, font, anim))
                    .chain(logs.iter().map(|(v, anim)| draw::log_panel(v, font, anim)))
                    .collect::<Vec<Element<'_, Message>>>();
                layers.push(draw::place(column(panels).spacing(12).into(), style, &Anim::still()));
            }
        }
        for (view, anim) in state.panels.shown(now, motion) {
            layers.push(match view {
                OverlayView::Keyboard(k) => draw::place(draw::keyboard_panel(k, font, &anim), &k.style, &anim),
                OverlayView::Menu(m) => {
                    let look = draw::MenuLook {
                        colourblind: state.frame.colourblind,
                        family: state.frame.family,
                        nintendo_layout: state.frame.nintendo_layout,
                        anim,
                    };
                    draw::place(draw::menu_panel(m, font, look), &m.style, &anim)
                }
                OverlayView::Media(m) => draw::place(draw::media_panel(m, font, &anim), &m.style, &anim),
                OverlayView::Offer(o) => draw::place(draw::offer_panel(o, font, &anim), &o.style, &anim),
            });
        }
        stack(layers).into()
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
pub mod draw {
    mod log_panel;
    mod radial;

    use std::f32::consts::TAU;

    pub use log_panel::log_panel;

    use radial::radial;

    use iced::{
        Alignment, Border, Color, Element, Length, Padding, Shadow, Vector,
        alignment::{Horizontal, Vertical},
        widget::{column, container, progress_bar, row, space, text},
    };
    use iced::Font;

    use super::KeyboardView;
    use crate::{
        config::{MenuKind, OverlayStyle, Paint, ScreenPosition},
        info::{Charge, FormFactor, Icon, InfoView, PadFamily, Segment},
        keyboard, media,
        media::{MediaView, PlayState},
        menu::MenuView,
        motion::Anim,
        offer::OfferView,
    };

    const UNIT: f32 = 46.0;
    const GAP: f32 = 4.0;
    /// Distance from the screen edge for edge and corner positions.
    const EDGE_MARGIN: f32 = 40.0;

    /// What a menu is drawn with besides its own style: the controller in use (for its button
    /// glyphs), the colour-blind tints, and how the menu is moving.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct MenuLook {
        pub colourblind: bool,
        pub family: crate::info::PadFamily,
        pub nintendo_layout: bool,
        pub anim: Anim,
    }

    fn blend(a: Color, b: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let mix = |x: f32, y: f32| x + (y - x) * t;
        Color { r: mix(a.r, b.r), g: mix(a.g, b.g), b: mix(a.b, b.b), a: mix(a.a, b.a) }
    }

    #[derive(Clone, Copy)]
    pub struct Colors {
        anim: Anim,
        font: Font,
        /// The controller in use, for button glyphs.
        family: crate::info::PadFamily,
        /// Face buttons drawn with the Nintendo layout's labels.
        nintendo_layout: bool,
        /// Tints for added and removed rows in colours colour-blind people can tell apart.
        colourblind: bool,
        background: Color,
        background_text: Color,
        muted: Color,
        item: Color,
        item_text: Color,
        selected: Color,
        selected_text: Color,
        /// 0 square .. 1 round, from the style's corners setting.
        corners: f32,
    }

    /// Panels round to this radius at full corners; their size depends on their content.
    const PANEL_CORNER: f32 = 40.0;

    fn paint(p: &Paint, fallback: [u8; 3]) -> Color {
        let [r, g, b] = p.rgb().unwrap_or(fallback);
        Color::from_rgba8(r, g, b, p.opacity.clamp(0.0, 1.0))
    }

    /// White on dark colors, black on light ones.
    fn text_on(c: Color) -> Color {
        let luminance = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
        if luminance > 0.6 { Color::from_rgb(0.08, 0.08, 0.1) } else { Color::WHITE }
    }

    impl Colors {
        /// How lit item `i` is, 0 to 1: the cursor's item, and the one it came from as it slides away.
        pub fn lit(&self, m: &MenuView, i: usize) -> f32 {
            self.anim.lit(m.selected == Some(i), self.anim.timing.from == Some(i as u32))
        }

        /// These colours for item `i`: its highlight blended in, its pick flashed, and its stagger fade.
        pub fn lit_item(&self, m: &MenuView, i: usize) -> Colors {
            let lit = self.lit(m, i);
            let flash = if m.selected == Some(i) { self.anim.flash() } else { 0.0 };
            let fade = self.anim.item_fade(i);
            let faded = |c: Color| Color { a: c.a * fade, ..c };
            Colors {
                item: faded(blend(blend(self.item, self.selected, lit), Color::WHITE, flash * 0.5)),
                item_text: faded(blend(self.item_text, self.selected_text, lit)),
                ..*self
            }
        }

        /// Every color at `opacity` times its own, for fading out.
        fn faded(self, opacity: f32) -> Colors {
            let f = |c: Color| Color { a: c.a * opacity.clamp(0.0, 1.0), ..c };
            Colors {
                anim: self.anim,
                font: self.font,
                family: self.family,
                nintendo_layout: self.nintendo_layout,
                colourblind: self.colourblind,
                background: f(self.background),
                background_text: f(self.background_text),
                muted: f(self.muted),
                item: f(self.item),
                item_text: f(self.item_text),
                selected: f(self.selected),
                selected_text: f(self.selected_text),
                corners: self.corners,
            }
        }
    }

    fn colors(style: &OverlayStyle, font: Font) -> Colors {
        let background = paint(&style.background, [0x16, 0x18, 0x1c]);
        let item = paint(&style.items, [0x30, 0x34, 0x3c]);
        let selected = paint(&style.selected, [0x2f, 0x5d, 0xb0]);
        let background_text = text_on(background);
        Colors {
            anim: Anim::still(),
            font,
            family: crate::info::PadFamily::default(),
        nintendo_layout: false,
            colourblind: false,
            background,
            background_text,
            muted: Color { a: 0.75, ..background_text },
            item,
            item_text: text_on(item),
            selected,
            selected_text: text_on(selected),
            corners: style.corners,
        }
    }

    /// Positions an overlay panel on the full-screen surface, moved by its animation.
    pub fn place<'a, M: 'a>(panel: Element<'a, M>, style: &OverlayStyle, anim: &Anim) -> Element<'a, M> {
        let (col, row) = style.position.cell();
        let horizontal = [Horizontal::Left, Horizontal::Center, Horizontal::Right][col];
        let vertical = [Vertical::Top, Vertical::Center, Vertical::Bottom][row];
        let margin = if style.position == ScreenPosition::Center { 0.0 } else { EDGE_MARGIN };
        let (x, y) = anim.offset();
        let (left, right) = sides(margin, x, col);
        let (top, bottom) = sides(margin, y, row);
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(horizontal)
            .align_y(vertical)
            .padding(Padding { top, right, bottom, left })
            .into()
    }

    /// The padding on the two sides of a panel, for a panel `shift` pixels off where it sits in
    /// `slot` (0 start, 1 centre, 2 end) of a `margin` from the edge.
    fn sides(margin: f32, shift: f32, slot: usize) -> (f32, f32) {
        match slot {
            0 => ((margin + shift).max(0.0), margin),
            2 => (margin, (margin - shift).max(0.0)),
            // A centred panel sits halfway between its two paddings.
            _ => (margin + (2.0 * shift).max(0.0), margin + (-2.0 * shift).max(0.0)),
        }
    }

    fn panel_style(c: Colors) -> impl Fn(&iced::Theme) -> container::Style {
        move |_| container::Style {
            background: Some(c.background.into()),
            border: Border { width: 1.0, radius: (c.corners * PANEL_CORNER).into(), color: Color { a: 0.15, ..c.background_text } },
            shadow: Shadow { color: Color { a: 0.35 * c.background.a, ..Color::BLACK }, offset: Vector::new(0.0, 6.0), blur_radius: 18.0 },
            ..container::Style::default()
        }
    }

    /// The colours of a row of this kind: adding rows are tinted green and removing ones red, so
    /// they stand out from the items they act on.
    fn toned(c: Colors, tone: crate::menu::Tone) -> Colors {
        // Lime with white text stays readable at this mix (about 4.7:1 on the default items).
        let (tint, share) = match (tone, c.colourblind) {
            (crate::menu::Tone::Normal, _) => return c,
            (crate::menu::Tone::Add, false) => (Color::from_rgb8(0x5a, 0x9a, 0x1a), 0.75),
            (crate::menu::Tone::Remove, false) => (Color::from_rgb8(0xd0, 0x64, 0x64), 0.4),
            (crate::menu::Tone::Add, true) => (Color::from_rgb8(0x4a, 0x8f, 0xe0), 0.4),
            (crate::menu::Tone::Remove, true) => (Color::from_rgb8(0xe8, 0x96, 0x2e), 0.4),
        };
        let mix = |a: f32, b: f32| a + (b - a) * share;
        let item = Color { r: mix(c.item.r, tint.r), g: mix(c.item.g, tint.g), b: mix(c.item.b, tint.b), a: c.item.a };
        Colors { item, item_text: text_on(item), ..c }
    }

    /// An item's box, `height` tall: its corners round by half of that at full corners, so a
    /// square cell becomes a circle.
    /// A cell whose colours already include its highlight (see `Colors::lit_item`).
    fn lit_cell_style(c: Colors, selected: bool, height: f32) -> impl Fn(&iced::Theme) -> container::Style {
        move |_| container::Style {
            background: Some(c.item.into()),
            border: Border {
                width: if selected { 2.0 } else { 1.0 },
                radius: (c.corners * height / 2.0).into(),
                color: if selected { c.selected_text } else { Color { a: 0.12, ..c.item_text } },
            },
            ..container::Style::default()
        }
    }

    fn cell_style(c: Colors, selected: bool, height: f32) -> impl Fn(&iced::Theme) -> container::Style {
        move |_| container::Style {
            background: Some(if selected { c.selected } else { c.item }.into()),
            border: Border {
                width: if selected { 2.0 } else { 1.0 },
                radius: (c.corners * height / 2.0).into(),
                color: if selected { c.selected_text } else { Color { a: 0.12, ..c.item_text } },
            },
            ..container::Style::default()
        }
    }

    /// An info overlay: its cells in a grid, columns as wide as their widest cell.
    pub fn info_panel<'a, M: 'a>(v: &InfoView, font: Font, anim: &Anim) -> Element<'a, M> {
        let opacity = v.opacity * anim.opacity();
        let c = colors(&v.style, font).faded(opacity);
        let s = v.style.scale.clamp(0.5, 2.0);
        let line = 30.0 * s;
        let columns = v.rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut grid = row![].spacing(20.0 * s);
        for j in 0..columns {
            let mut col = column![].spacing(4.0 * s);
            for r in &v.rows {
                let cell: Element<'a, M> = match r.get(j) {
                    Some(segments) => info_cell(segments, c, s, opacity),
                    None => space().into(),
                };
                col = col.push(container(cell).height(line).align_y(Vertical::Center));
            }
            grid = grid.push(col);
        }
        let body: Element<'a, M> = match &v.title {
            Some(title) => column![
                container(info_cell(title, c, s, opacity)).padding([0.0, 0.0]).height(line).align_y(Vertical::Center),
                grid,
            ]
            .spacing(8.0 * s)
            .into(),
            None => grid.into(),
        };
        container(body).padding([12.0 * s, 16.0 * s]).style(panel_style(c)).into()
    }

    fn info_cell<'a, M: 'a>(segments: &[Segment], c: Colors, s: f32, opacity: f32) -> Element<'a, M> {
        let mut line = row![].spacing(2.0 * s).align_y(Alignment::Center);
        for seg in segments {
            line = line.push(segment_element(seg, c, s, opacity));
        }
        line.into()
    }

    /// One piece of an info overlay's line, or of a menu row's glyphs.
    fn segment_element<'a, M: 'a>(seg: &Segment, c: Colors, s: f32, opacity: f32) -> Element<'a, M> {
        match seg {
            Segment::Text(t) => Element::from(text(t.clone()).font(c.font).size(16.0 * s).color(c.background_text)),
            Segment::Glyph { label, fill, round } => glyph(label, *fill, *round, c, s, opacity),
            Segment::Dpad(lit) => dpad_glyph(*lit, c, s),
            Segment::StickClick { right } => stick_click_glyph(*right, c, s),
            Segment::Icon(icon) => icon_glyph(icon, c, s),
        }
    }

    /// A cross-shaped D-pad with the pressed arms (`[up, down, left, right]`) lit.
    fn dpad_glyph<'a, M: 'a>([up, down, left, right]: [bool; 4], c: Colors, s: f32) -> Element<'a, M> {
        let cell = 8.0 * s;
        let arm = |on: bool| -> Element<'a, M> {
            let color = if on { c.selected } else { c.item };
            container(space())
                .width(cell)
                .height(cell)
                .style(move |_: &iced::Theme| container::Style {
                    background: Some(color.into()),
                    border: Border { width: 1.0, radius: (1.5 * s).into(), color: Color { a: 0.35, ..c.item_text } },
                    ..container::Style::default()
                })
                .into()
        };
        let gap = || -> Element<'a, M> { space().width(cell).height(cell).into() };
        column![row![gap(), arm(up), gap()], row![arm(left), arm(false), arm(right)], row![gap(), arm(down), gap()]]
        .into()
    }

    /// A stick press: a round stick cap marked L or R, with a down arrow for the push.
    fn stick_click_glyph<'a, M: 'a>(right: bool, c: Colors, s: f32) -> Element<'a, M> {
        let size = 26.0 * s;
        let label = if right { "R" } else { "L" };
        let cap = container(row![
            text(label).font(c.font).size(12.0 * s).color(c.item_text),
            text("↓").font(c.font).size(12.0 * s).color(c.item_text),
        ])
        .center_x(size)
        .center_y(size);
        cap.style(move |_: &iced::Theme| container::Style {
            background: Some(c.item.into()),
            border: Border { width: 2.0, radius: (size / 2.0).into(), color: Color { a: 0.6, ..c.item_text } },
            ..container::Style::default()
        })
        .into()
    }

    /// A button glyph: a colored disc for face buttons, a rounded tag for the rest.
    #[expect(clippy::too_many_arguments, reason = "predates the size lints")]
    fn glyph<'a, M: 'a>(label: &str, fill: Option<[u8; 3]>, round: bool, c: Colors, s: f32, opacity: f32) -> Element<'a, M> {
        let (bg, fg) = match fill {
            Some([r, g, b]) => (Color::from_rgba8(r, g, b, opacity), Color { a: opacity, ..Color::WHITE }),
            None => (c.item, c.item_text),
        };
        let size = 24.0 * s;
        let disc = round && label.chars().count() <= 2;
        let body = container(text(label.to_string()).font(c.font).size(13.0 * s).color(fg));
        let body = if disc {
            body.center_x(size).center_y(size)
        } else {
            body.padding([0.0, 7.0 * s]).height(size).center_y(size)
        };
        body.style(move |_: &iced::Theme| container::Style {
            background: Some(bg.into()),
            border: Border {
                width: 1.0,
                radius: if disc { size / 2.0 } else { 6.0 * s }.into(),
                color: Color { a: 0.2, ..fg },
            },
            ..container::Style::default()
        })
        .into()
    }

    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    pub fn keyboard_panel<'a, M: 'a>(v: &KeyboardView, font: Font, anim: &Anim) -> Element<'a, M> {
        let c = colors(&v.style, font).faded(anim.opacity());
        let s = v.style.scale.clamp(0.5, 2.0);
        let numpad = v.layout == crate::keyboard::Layout::Numpad;
        // Numpad keys are fewer, so bigger.
        let unit = if numpad { UNIT * 1.5 } else { UNIT } * s;
        let gap = GAP * s;
        let cursor_code = keyboard::key_at(v.layout, v.cursor).code;
        let label_size = if numpad { 22.0 } else { 14.0 };
        let mut rows = column![].spacing(gap);
        // Keys are numbered in reading order, as the motion tracking numbers them.
        let mut slot = 0u32;
        for keys in v.layout.rows().iter() {
            let mut line = row![].spacing(gap);
            for key in keys.iter() {
                let here = slot;
                slot += 1;
                let width = key.width * (unit + gap) - gap;
                if key.code.is_empty() {
                    line = line.push(space().width(width).height(unit));
                    continue;
                }
                let cursor = key.code == cursor_code;
                let pressed = v.pressed.as_deref() == Some(key.code);
                let selected = cursor || pressed;
                let latched = v.latched.iter().any(|l| l == key.code);
                // The cursor's key lights up and the one it left goes dark, sliding between them.
                let lit = if pressed { 1.0 } else { anim.lit(cursor, anim.timing.from == Some(here)) };
                let (bg, fg, border) = if latched && !selected {
                    let green = Color::from_rgb8(0x2e, 0x7d, 0x46);
                    (green, Color::WHITE, Color::from_rgb8(0x3f, 0xb9, 0x50))
                } else {
                    let flash = if pressed { anim.flash() * 0.5 } else { 0.0 };
                    (
                        blend(blend(c.item, c.selected, lit), Color::WHITE, flash),
                        blend(c.item_text, c.selected_text, lit),
                        blend(Color { a: 0.12, ..c.item_text }, c.selected_text, lit),
                    )
                };
                let round = c.corners * unit / 2.0;
                let cap = container(text(key.label).font(c.font).size(if selected { label_size + 2.0 } else { label_size } * s).color(fg))
                    .center_x(width)
                    .center_y(unit)
                    .style(move |_| container::Style {
                        background: Some(bg.into()),
                        border: Border { width: if selected { 2.0 } else { 1.0 }, radius: round.into(), color: border },
                        ..container::Style::default()
                    });
                line = line.push(cap);
            }
            rows = rows.push(line);
        }
        let hint = |t: &'static str| text(t).font(c.font).size(14.0 * s).color(c.background_text);
        let (legend, width): (Element<'a, M>, f32) = if numpad {
            let legend = column![
                row![hint("A  press"), hint("X  backspace")].spacing(14.0 * s),
                row![hint("Start  enter"), hint("hold B to close")].spacing(14.0 * s),
            ]
            .spacing(4.0 * s)
            .align_x(Alignment::Center);
            (legend.into(), (3.0 * (unit + gap)).max(230.0 * s))
        } else {
            let legend = row![
                hint("A  press"),
                hint("X  backspace"),
                hint("Y  space"),
                hint("Start  enter"),
                hint("hold LT  shift"),
                hint("Shift, Ctrl and Alt stay on for one key"),
                space::horizontal(),
                hint("hold B to close"),
            ]
            .spacing(18.0 * s)
            .align_y(Alignment::Center);
            (legend.into(), 15.0 * (unit + gap))
        };
        let mut body = column![rows, legend].spacing(12.0 * s).width(width).align_x(Alignment::Center);
        if v.closing > 0.0 {
            body = body.push(progress_bar(0.0..=1.0, v.closing).girth(4.0 * s));
        }
        container(body).padding(16.0 * s).style(panel_style(c)).into()
    }

    /// An item's glyphs (the same as the info overlays'), its label, and a ▸ for submenus. A
    /// text badge (a stick's LS, say) is drawn as a small tag.
    fn item_text<'a, M: 'a>(c: Colors, item: &crate::menu::ItemView, label: &str, size: f32, fg: Color) -> Element<'a, M> {
        let font = c.font;
        let mut line = row![].spacing(size * 0.5).align_y(Alignment::Center);
        // The glyphs are sized from the label, which is 17 units when the scale is 1.
        for b in &item.buttons {
            line = line.push(segment_element(&crate::info::button_glyph(*b, c.family, c.nintendo_layout), c, size / 17.0, 1.0));
        }
        if let Some(b) = item.button.as_deref().filter(|b| !b.is_empty()) {
            line = line.push(
                container(text(b.to_string()).font(font).size(size - 2.0).color(Color::BLACK))
                    .padding([1.0, size * 0.4])
                    .style(|_| container::Style {
                        background: Some(Color::from_rgb(0.85, 0.87, 0.9).into()),
                        border: Border { radius: 5.0.into(), ..Border::default() },
                        ..container::Style::default()
                    }),
            );
        }
        if let Some(k) = item.keyword {
            let icon = iced::widget::svg::Handle::from_memory(crate::keyword_icon::svg(k).into_bytes());
            line = line.push(iced::widget::svg(icon).width(size).height(size));
        }
        line = line.push(text(label.to_string()).font(font).size(size).color(fg));
        if item.submenu {
            line = line.push(text("▸").font(font).size(size).color(Color { a: 0.7, ..fg }));
        }
        line.into()
    }

    /// An icon in an info cell, drawn as SVG so it doesn't depend on the font having the glyph.
    fn icon_glyph<'a, M: 'a>(icon: &Icon, c: Colors, s: f32) -> Element<'a, M> {
        let size = 22.0 * s;
        let svg_text = icon_svg(icon, c.background_text);
        iced::widget::svg(iced::widget::svg::Handle::from_memory(svg_text.into_bytes())).width(size).height(size).into()
    }

    /// The SVG for an info icon, in `ink`. A controller is drawn in its kind's color, where it
    /// has one; a battery's fill is green, amber or red by its level.
    pub fn icon_svg(icon: &Icon, ink: Color) -> String {
        let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        let [r, g, b, _] = ink.into_rgba8();
        let c = hex([r, g, b]);
        let body = match icon {
            Icon::Form(FormFactor::Desktop) => format!(
                "<rect x='2' y='3' width='20' height='13' rx='2' fill='none' stroke='{c}' stroke-width='2'/>\
                 <path d='M8 21 H16 M12 16 V21' fill='none' stroke='{c}' stroke-width='2' stroke-linecap='round'/>"
            ),
            Icon::Form(FormFactor::Laptop) => format!(
                "<rect x='5' y='4' width='14' height='11' rx='1.5' fill='none' stroke='{c}' stroke-width='2'/>\
                 <path d='M2 18 H22 L21 20.5 H3 Z' fill='{c}'/>"
            ),
            Icon::Form(FormFactor::Handheld) => format!(
                "<rect x='2' y='6' width='20' height='12' rx='4' fill='none' stroke='{c}' stroke-width='2'/>\
                 <rect x='8.5' y='9' width='7' height='6' rx='1' fill='{c}'/>"
            ),
            Icon::Controller(family) => {
                let tint = match family {
                    PadFamily::Xbox => hex([0x3c, 0xa0, 0x3c]),
                    PadFamily::PlayStation => hex([0x5a, 0x8c, 0xdc]),
                    PadFamily::Nintendo => c.clone(),
                };
                format!(
                    "<path d='M7 7 H17 C20.5 7 22.5 9.5 22.5 13.5 C22.5 17.5 20.8 18.5 19.2 18.5 \
                     C17.8 18.5 16.9 17.2 15.9 15.8 H8.1 C7.1 17.2 6.2 18.5 4.8 18.5 \
                     C3.2 18.5 1.5 17.5 1.5 13.5 C1.5 9.5 3.5 7 7 7 Z' fill='{tint}'/>"
                )
            }
            Icon::Battery(charge) => battery_svg(*charge, &c),
            Icon::Wifi(percent) => wifi_svg(*percent, &c),
        };
        format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' opacity='{}'>{body}</svg>", ink.a)
    }

    /// A battery outline filled to its level; an estimate is faded.
    fn battery_svg(charge: Charge, c: &str) -> String {
        let percent = charge.percent.min(100);
        let width = 14.0 * f32::from(percent) / 100.0;
        let level = match percent {
            50.. => [0x3c, 0xa0, 0x3c],
            20..50 => [0xc8, 0xa0, 0x1e],
            _ => [0xc8, 0x3c, 0x3c],
        };
        let level = format!("#{:02x}{:02x}{:02x}", level[0], level[1], level[2]);
        let opacity = if charge.estimated { 0.6 } else { 1.0 };
        format!(
            "<rect x='1.5' y='6.5' width='18' height='11' rx='2.5' fill='none' stroke='{c}' stroke-width='2'/>\
             <rect x='21' y='10' width='2' height='4' rx='1' fill='{c}'/>\
             <rect x='4' y='9' width='{width:.2}' height='6' rx='1' fill='{level}' fill-opacity='{opacity}'/>"
        )
    }

    /// Wi-Fi bars, lit as the signal passes each quarter.
    fn wifi_svg(percent: u8, c: &str) -> String {
        let lit: u8 = match percent {
            0 => 0,
            1..=25 => 1,
            26..=50 => 2,
            51..=75 => 3,
            _ => 4,
        };
        (0u8..4)
            .map(|i| {
                let height = 5.0 + 4.0 * f32::from(i);
                let opacity = if i < lit { 1.0 } else { 0.25 };
                format!(
                    "<rect x='{:.1}' y='{:.1}' width='3.5' height='{height:.1}' rx='1' fill='{c}' fill-opacity='{opacity}'/>",
                    2.0 + 5.5 * f32::from(i),
                    21.0 - height
                )
            })
            .collect()
    }

    /// A play, pause or stop symbol, drawn so it doesn't depend on the font having the glyph.
    fn state_icon<'a, M: 'a>(state: PlayState, color: Color, size: f32) -> Element<'a, M> {
        let [r, g, b, _] = color.into_rgba8();
        let shape = match state {
            PlayState::Playing => "<path d='M6 3 L21 12 L6 21 Z'/>",
            PlayState::Paused => "<rect x='5' y='3' width='5' height='18' rx='1'/><rect x='14' y='3' width='5' height='18' rx='1'/>",
            PlayState::Stopped => "<rect x='5' y='5' width='14' height='14' rx='2'/>",
        };
        let svg_text = format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='#{r:02x}{g:02x}{b:02x}' fill-opacity='{}'>{shape}</svg>", color.a);
        iced::widget::svg(iced::widget::svg::Handle::from_memory(svg_text.into_bytes())).width(size).height(size).into()
    }

    /// The media controls: what is playing, how far along, and the volume.
    pub fn media_panel<'a, M: 'a>(m: &MediaView, font: Font, anim: &Anim) -> Element<'a, M> {
        let c = colors(&m.style, font).faded(anim.opacity());
        let s = m.style.scale.clamp(0.5, 2.0);
        let width = 560.0 * s;
        let body: Element<'a, M> = match &m.player {
            None => text("No media player is running").font(c.font).size(18.0 * s).color(c.background_text).into(),
            Some(player) => {
                let title = if m.title.is_empty() { "Nothing playing" } else { &m.title };
                let by = [m.artist.as_str(), player.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" · ");
                let progress = if m.length_ms > 0 { (m.position_ms as f32 / m.length_ms as f32).clamp(0.0, 1.0) } else { 0.0 };
                let times = format!("{} / {}", media::clock(m.position_ms), if m.length_ms > 0 { media::clock(m.length_ms) } else { "--:--".into() });
                let mut meta = row![text(times).font(c.font).size(14.0 * s).color(c.muted)].spacing(16.0 * s);
                if let Some(v) = m.volume {
                    meta = meta.push(text(format!("Volume {:.0}%", v * 100.0)).font(c.font).size(14.0 * s).color(c.muted));
                }
                if m.players > 1 {
                    meta = meta.push(text(format!("{} players", m.players)).font(c.font).size(14.0 * s).color(c.muted));
                }
                column![
                    row![
                        state_icon(m.state, c.background_text, 28.0 * s),
                        column![
                            text(title.to_string()).font(c.font).size(20.0 * s).color(c.background_text),
                            text(by).font(c.font).size(14.0 * s).color(c.muted),
                        ]
                        .spacing(2.0 * s),
                    ]
                    .spacing(14.0 * s)
                    .align_y(Alignment::Center),
                    progress_bar(0.0..=1.0, progress).girth(6.0 * s),
                    meta,
                ]
                .spacing(10.0 * s)
                .into()
            }
        };
        container(
            column![body, text(m.hint.clone()).font(c.font).size(12.0 * s).color(c.muted)]
                .spacing(12.0 * s)
                .width(width)
                .align_x(Alignment::Center),
        )
        .padding(18.0 * s)
        .style(panel_style(c))
        .into()
    }

    /// The offer to add a library game: the question and its answers, or the packs to pick from.
    pub fn offer_panel<'a, M: 'a>(o: &OfferView, font: Font, anim: &Anim) -> Element<'a, M> {
        let c = colors(&o.style, font).faded(anim.opacity());
        let s = o.style.scale.clamp(0.5, 2.0);
        let cell = |label: String, lit: Colors, selected: bool| {
            container(text(label).font(c.font).size(16.0 * s).color(lit.item_text))
                .padding([8.0 * s, 16.0 * s])
                .style(lit_cell_style(lit, selected, 36.0 * s))
        };
        let body: Element<'a, M> = if o.choices.is_empty() {
            let answers = o.answers.iter().map(|(button, what)| cell(format!("{button}  {what}"), c, false).into());
            row(answers).spacing(10.0 * s).into()
        } else {
            let items = o.choices.iter().enumerate().map(|(i, name)| {
                let selected = i == o.selected;
                let lit = anim.lit(selected, anim.timing.from == Some(i as u32));
                let tinted = Colors { item: blend(c.item, c.selected, lit), item_text: blend(c.item_text, c.selected_text, lit), ..c };
                cell(name.clone(), tinted, selected).width(Length::Fill).into()
            });
            column(items).spacing(6.0 * s).width(360.0 * s).into()
        };
        container(
            column![
                text(o.title.clone()).font(c.font).size(20.0 * s).color(c.background_text),
                text(o.detail.clone()).font(c.font).size(14.0 * s).color(c.muted).width(520.0 * s).align_x(Alignment::Center),
                body,
                text(o.hint.clone()).font(c.font).size(12.0 * s).color(c.muted),
            ]
            .spacing(14.0 * s)
            .align_x(Alignment::Center),
        )
        .padding(22.0 * s)
        .style(panel_style(c))
        .into()
    }

    pub fn menu_panel<'a, M: 'a>(m: &MenuView, font: Font, look: MenuLook) -> Element<'a, M> {
        let c = Colors {
            anim: look.anim,
            family: look.family,
            nintendo_layout: look.nintendo_layout,
            colourblind: look.colourblind,
            ..colors(&m.style, font)
        };
        let c = c.faded(look.anim.opacity());
        let s = m.style.scale.clamp(0.5, 2.0);
        let body: Element<'a, M> = match m.kind {
            MenuKind::Radial { .. } => radial(m, c, s),
            MenuKind::Directional { .. } => directional(m, c, s),
            MenuKind::List | MenuKind::Buttons => list(m, c, s),
            MenuKind::Carousel { .. } => carousel(m, c, s),
            MenuKind::Grid { .. } => grid(m, c, s),
        };
        // Where this page is in the menus, above its title: "Menu › Edit Controls › A button".
        let title = text(m.title.clone()).font(c.font).size(20.0 * s).color(c.background_text);
        let heading: Element<'a, M> = if m.crumbs.is_empty() {
            title.into()
        } else {
            let crumbs = text(m.crumbs.join(" › ")).font(c.font).size(13.0 * s).color(c.muted);
            column![crumbs, title].spacing(4.0 * s).align_x(Alignment::Center).into()
        };
        container(
            column![
                heading,
                body,
                text(m.hint.clone()).font(c.font).size(13.0 * s).color(c.muted),
            ]
            .spacing(14.0 * s)
            .align_x(Alignment::Center),
        )
        .padding(22.0 * s)
        .style(panel_style(c))
        .into()
    }

    /// An item's label, its colours (highlighted, and faded in under Stagger) and whether the
    /// cursor is on it.
    fn item_cell<'a, M: 'a>(m: &MenuView, i: usize, c: Colors, size: f32) -> (Element<'a, M>, Colors, bool) {
        let item = &m.items[i];
        let selected = m.selected == Some(i);
        let lit = c.lit_item(m, i);
        (item_text(lit, item, &item.label, size, lit.item_text), lit, selected)
    }

    fn directional<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
        let (w, h) = (190.0 * s, 52.0 * s);
        let slot = |i: usize| -> Element<'a, M> {
            match m.items.get(i).filter(|item| !item.label.is_empty()) {
                Some(item) => container(item_text(c, item, &item.label, 16.0 * s, c.item_text))
                    .center_x(w)
                    .center_y(h)
                    .style(cell_style(c, false, h))
                    .into(),
                None => space().width(w).height(h).into(),
            }
        };
        column![
            slot(0),
            row![slot(3), space().width(40.0 * s), slot(1)].align_y(Alignment::Center),
            slot(2),
        ]
        .spacing(10.0 * s)
        .align_x(Alignment::Center)
        .into()
    }

    /// Items in rows of equal cells, read left to right; a short last row keeps to the left.
    fn grid<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
        let columns = m.kind.grid_columns().unwrap_or(1);
        let (w, h) = (150.0 * s, 64.0 * s);
        let mut rows = column![].spacing(8.0 * s);
        for start in (0..m.items.len()).step_by(columns) {
            let mut line = row![].spacing(8.0 * s);
            for i in start..(start + columns).min(m.items.len()) {
                let (label, lit, selected) = item_cell(m, i, c, 16.0 * s);
                line = line.push(container(label).center_x(w).center_y(h).padding([0.0, 6.0 * s]).style(lit_cell_style(lit, selected, h)));
            }
            rows = rows.push(line);
        }
        rows.into()
    }

    /// A list: one column while it is short, two once it is longer. Two columns fill top to
    /// bottom, the left one first, and scroll together with the cursor.
    fn list<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
        let len = m.items.len();
        let cursor = m.selected.unwrap_or(0);
        let cell = |i: usize| -> Element<'a, M> {
            let item = &m.items[i];
            let selected = m.selected == Some(i);
            let lit = toned(c, item.tone).lit_item(m, i);
            let label = match (c.colourblind, item.tone) {
                (true, crate::menu::Tone::Add) => format!("+ {}", item.label),
                (true, crate::menu::Tone::Remove) => format!("− {}", item.label),
                _ => item.label.clone(),
            };
            let content = item_text(lit, item, &label, 17.0 * s, lit.item_text);
            container(content).padding([10.0 * s, 14.0 * s]).width(Length::Fill).style(lit_cell_style(lit, selected, 40.0 * s)).into()
        };
        let marker = |t: &'static str| text(t).font(c.font).size(13.0 * s).color(c.muted);
        let start = crate::menu::list_start(cursor, len);
        let half = if crate::menu::list_columns(len) == 2 { len.div_ceil(2) } else { len };
        let shown = |from: usize, to: usize| (from..to.min(from + crate::menu::VISIBLE_ROWS)).map(&cell).collect::<Vec<Element<'a, M>>>();
        let column_of = |items: Vec<Element<'a, M>>, width: f32| items.into_iter().fold(column![].spacing(6.0 * s).width(width * s), iced::widget::Column::push);
        let left = column_of(shown(start, half), 300.0);
        let grid: Element<'a, M> = if half < len {
            row![left, column_of(shown(half + start, len), 300.0)].spacing(14.0 * s).into()
        } else {
            left.into()
        };
        let mut body = column![].spacing(6.0 * s);
        if start > 0 {
            body = body.push(marker("▲"));
        }
        body = body.push(grid);
        if start + crate::menu::VISIBLE_ROWS < half {
            body = body.push(marker("▼"));
        }
        body.into()
    }

    fn carousel<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
        let n = m.items.len();
        let selected = m.selected.unwrap_or(0);
        let arrow = |t: &'static str| text(t).font(c.font).size(22.0 * s).color(c.muted);
        let mut line = row![arrow("◀")].spacing(12.0 * s).align_y(Alignment::Center);
        // The selected item in the middle, with up to two neighbors on each side.
        let shown = n.min(5) as i32;
        for offset in -(shown / 2)..=(shown - 1 - shown / 2) {
            let i = (selected as i32 + offset).rem_euclid(n as i32) as usize;
            let big = offset == 0;
            let item = &m.items[i];
            let lit = c.lit_item(m, i);
            let height = if big { 80.0 } else { 60.0 } * s;
            line = line.push(
                container(item_text(lit, item, &item.label, if big { 19.0 } else { 14.0 } * s, lit.item_text))
                    .center_x(if big { 170.0 } else { 120.0 } * s)
                    .center_y(height)
                    .style(lit_cell_style(lit, big, height)),
            );
        }
        line.push(arrow("▶")).into()
    }
}

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

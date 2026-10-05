//! On-screen overlay: a keyboard you type on with the controller, and action menus. The
//! daemon owns the state (`OverlayController` here, `menu::MenuSession`) and routes
//! controller input to it while something is shown; the overlay process
//! (`controller_app overlay`) only draws it, on a Wayland layer-shell surface that never
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
    keyboard::{self, Cursor},
};

/// Holding East this long closes the overlay.
pub const HOLD_TO_CLOSE: Duration = Duration::from_millis(700);
/// Holding a direction moves once, then repeats after this delay...
const REPEAT_DELAY: Duration = Duration::from_millis(350);
/// ...this often.
const REPEAT_EVERY: Duration = Duration::from_millis(90);
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
}

/// The on-screen keyboard's state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyboardView {
    pub cursor: Cursor,
    /// Modifiers latched for the next key (evdev names).
    pub latched: Vec<String>,
    /// A key currently held down with A.
    pub pressed: Option<String>,
    /// 0..1 while East is being held to close.
    pub closing: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OverlayAction {
    Key(KeyCode, bool),
    Close,
}

/// Turns controller input into keyboard navigation and key presses.
pub struct OverlayController {
    cursor: Cursor,
    latched: Vec<KeyCode>,
    pressed: Option<KeyCode>,
    dpad: HashSet<Button>,
    stick: (f32, f32),
    stick_dir: Option<(i32, i32)>,
    /// Direction being held and when it next repeats.
    repeat: Option<((i32, i32), Instant)>,
    east_since: Option<Instant>,
}

fn code(name: &str) -> Option<KeyCode> {
    KeyCode::from_str(name).ok()
}

fn tap(name: &str) -> Vec<OverlayAction> {
    code(name).map(|k| vec![OverlayAction::Key(k, true), OverlayAction::Key(k, false)]).unwrap_or_default()
}

impl OverlayController {
    pub fn new(cursor: Cursor) -> Self {
        OverlayController {
            cursor,
            latched: Vec::new(),
            pressed: None,
            dpad: HashSet::new(),
            stick: (0.0, 0.0),
            stick_dir: None,
            repeat: None,
            east_since: None,
        }
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn view(&self, now: Instant) -> KeyboardView {
        let name = |k: KeyCode| format!("{k:?}");
        KeyboardView {
            cursor: self.cursor,
            latched: self.latched.iter().map(|k| name(*k)).collect(),
            pressed: self.pressed.map(name),
            closing: self
                .east_since
                .map(|t| (now.duration_since(t).as_secs_f32() / HOLD_TO_CLOSE.as_secs_f32()).min(1.0))
                .unwrap_or(0.0),
        }
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
            InputEvent::Axis(..) => Vec::new(),
        }
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
        match (b, pressed) {
            (Button::South, true) => self.press_selected(),
            (Button::South, false) => self.release_selected(),
            (Button::West, true) => self.shortcut("KEY_BACKSPACE"),
            (Button::North, true) => self.shortcut("KEY_SPACE"),
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
        self.cursor = keyboard::step(self.cursor, dir.0, dir.1);
        self.repeat = Some((dir, now + REPEAT_DELAY));
    }

    fn press_selected(&mut self) -> Vec<OverlayAction> {
        let name = keyboard::key_at(self.cursor).code;
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
        self.latched.drain(..).rev().map(|k| OverlayAction::Key(k, false)).collect()
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
                self.cursor = keyboard::step(self.cursor, dir.0, dir.1);
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
    ui::run()
}

mod ui {
    use std::{f32::consts::TAU, time::Duration};

    use iced::{
        Alignment, Border, Color, Element, Length, Subscription, Task,
        alignment::Vertical,
        futures::{SinkExt, channel::mpsc},
        widget::{column, container, pin, progress_bar, row, space, stack, text},
    };
    use iced_layershell::{
        reexport::{Anchor, KeyboardInteractivity, Layer},
        settings::{LayerShellSettings, Settings, StartMode},
        to_layer_message,
    };
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::{KeyboardView, OverlayView};
    use crate::{
        config::MenuKind,
        ipc::{self, Request},
        keyboard,
        menu::MenuView,
    };

    const UNIT: f32 = 46.0;
    const GAP: f32 = 4.0;
    const TEXT: Color = Color::WHITE;
    const MUTED: Color = Color { r: 0.75, g: 0.78, b: 0.82, a: 1.0 };
    const PANEL: Color = Color { r: 0.086, g: 0.094, b: 0.11, a: 0.92 };
    const CELL: Color = Color { r: 0.19, g: 0.2, b: 0.235, a: 0.95 };
    const SELECTED: Color = Color { r: 0.18, g: 0.36, b: 0.69, a: 1.0 };
    const ACCENT: Color = Color { r: 0.31, g: 0.63, b: 1.0, a: 1.0 };

    #[to_layer_message]
    #[derive(Debug, Clone)]
    enum Message {
        State(Option<OverlayView>),
        /// The daemon went away: nothing left to draw for.
        Disconnected,
    }

    struct Overlay {
        view: Option<OverlayView>,
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
        iced_layershell::application(boot, namespace, update, view)
            .style(|_, _| iced::theme::Style { background_color: Color::TRANSPARENT, text_color: TEXT })
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
        (Overlay { view: None }, Task::none())
    }

    fn namespace() -> String {
        "controller_app_overlay".into()
    }

    fn update(state: &mut Overlay, message: Message) -> Task<Message> {
        match message {
            Message::State(view) => {
                let was_shown = state.view.is_some();
                state.view = view;
                let (anchor, size) = match (was_shown, state.view.is_some()) {
                    (false, true) => full_screen(),
                    (true, false) => idle(),
                    _ => return Task::none(),
                };
                return Task::done(Message::AnchorSizeChange(anchor, size));
            }
            Message::Disconnected => return iced::exit(),
            _ => {}
        }
        Task::none()
    }

    fn subscription(_: &Overlay) -> Subscription<Message> {
        Subscription::run(watch)
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
                    if let Ok(state) = serde_json::from_str::<Option<OverlayView>>(&line) {
                        let _ = output.send(Message::State(state)).await;
                    }
                }
            }
            let _ = output.send(Message::Disconnected).await;
            // Keep the stream alive until iced exits.
            tokio::time::sleep(Duration::from_secs(3600)).await;
        })
    }


    fn panel(bg: Color) -> impl Fn(&iced::Theme) -> container::Style {
        move |_| container::Style {
            background: Some(bg.into()),
            border: Border { width: 1.0, radius: 14.0.into(), color: Color::from_rgba(1.0, 1.0, 1.0, 0.15) },
            ..container::Style::default()
        }
    }

    fn cell(selected: bool) -> impl Fn(&iced::Theme) -> container::Style {
        move |_| container::Style {
            background: Some(if selected { SELECTED } else { CELL }.into()),
            border: Border {
                width: if selected { 2.0 } else { 1.0 },
                radius: 9.0.into(),
                color: if selected { Color::WHITE } else { Color::from_rgba(1.0, 1.0, 1.0, 0.12) },
            },
            ..container::Style::default()
        }
    }

    fn view(state: &Overlay) -> Element<'_, Message> {
        match &state.view {
            None => space().into(),
            Some(OverlayView::Keyboard(k)) => container(keyboard_panel(k))
                .center_x(Length::Fill)
                .height(Length::Fill)
                .align_y(Vertical::Bottom)
                .padding(40)
                .into(),
            Some(OverlayView::Menu(m)) => container(menu_panel(m)).center(Length::Fill).into(),
        }
    }

    fn keyboard_panel(v: &KeyboardView) -> Element<'_, Message> {
        let cursor_code = keyboard::key_at(v.cursor).code;
        let mut rows = column![].spacing(GAP);
        for keys in keyboard::MAIN.iter() {
            let mut line = row![].spacing(GAP);
            for key in keys.iter() {
                let width = key.width * (UNIT + GAP) - GAP;
                if key.code.is_empty() {
                    line = line.push(space().width(width).height(UNIT));
                    continue;
                }
                let selected = key.code == cursor_code;
                let latched = v.latched.iter().any(|l| l == key.code);
                let held = v.pressed.as_deref() == Some(key.code);
                let (bg, border) = if held {
                    (ACCENT, Color::WHITE)
                } else if selected {
                    (SELECTED, Color::WHITE)
                } else if latched {
                    (Color::from_rgb8(0x2e, 0x7d, 0x46), Color::from_rgb8(0x3f, 0xb9, 0x50))
                } else {
                    (CELL, Color::from_rgba(1.0, 1.0, 1.0, 0.12))
                };
                let cap = container(text(key.label).size(if selected { 16 } else { 14 }).color(TEXT))
                    .center_x(width)
                    .center_y(UNIT)
                    .style(move |_| container::Style {
                        background: Some(bg.into()),
                        border: Border { width: if selected { 2.0 } else { 1.0 }, radius: 7.0.into(), color: border },
                        ..container::Style::default()
                    });
                line = line.push(cap);
            }
            rows = rows.push(line);
        }
        let legend = row![
            text("A  press").size(14),
            text("X  backspace").size(14),
            text("Y  space").size(14),
            text("Start  enter").size(14),
            text("Shift/Ctrl/Alt latch for the next key").size(14),
            space::horizontal(),
            text("hold B to close").size(14),
        ]
        .spacing(18)
        .align_y(Alignment::Center);
        let mut body = column![rows, legend].spacing(12).width(15.0 * (UNIT + GAP));
        if v.closing > 0.0 {
            body = body.push(progress_bar(0.0..=1.0, v.closing).girth(4));
        }
        container(body).padding(16).style(panel(PANEL)).into()
    }

    /// An item's label with its button badge and a ▸ for submenus.
    fn item_text<'a>(label: &str, button: Option<&str>, submenu: bool, size: u32) -> Element<'a, Message> {
        let mut line = row![].spacing(8).align_y(Alignment::Center);
        if let Some(b) = button.filter(|b| !b.is_empty()) {
            line = line.push(
                container(text(b.to_string()).size(size - 2).color(Color::BLACK))
                    .padding([1, 6])
                    .style(|_| container::Style {
                        background: Some(Color::from_rgb(0.85, 0.87, 0.9).into()),
                        border: Border { radius: 5.0.into(), ..Border::default() },
                        ..container::Style::default()
                    }),
            );
        }
        line = line.push(text(label.to_string()).size(size).color(TEXT));
        if submenu {
            line = line.push(text("▸").size(size).color(MUTED));
        }
        line.into()
    }

    fn menu_panel(m: &MenuView) -> Element<'_, Message> {
        let body: Element<'_, Message> = match m.kind {
            MenuKind::Radial { .. } => radial(m),
            MenuKind::Cascade { .. } => cascade(m),
            MenuKind::List | MenuKind::Buttons => list(m),
            MenuKind::Carousel { .. } => carousel(m),
        };
        let depth = if m.depth > 0 { format!("  ({} deep)", m.depth + 1) } else { String::new() };
        container(
            column![
                text(format!("{}{depth}", m.title)).size(20).color(TEXT),
                body,
                text(m.hint.clone()).size(13).color(MUTED),
            ]
            .spacing(14)
            .align_x(Alignment::Center),
        )
        .padding(22)
        .style(panel(PANEL))
        .into()
    }

    fn radial(m: &MenuView) -> Element<'_, Message> {
        const SIZE: f32 = 420.0;
        const RADIUS: f32 = 150.0;
        const CELL_W: f32 = 120.0;
        const CELL_H: f32 = 44.0;
        let n = m.items.len().max(1) as f32;
        let mut layers: Vec<Element<'_, Message>> = vec![space().width(SIZE).height(SIZE).into()];
        for (i, item) in m.items.iter().enumerate() {
            // First item at the top, then clockwise.
            let angle = i as f32 / n * TAU;
            let (x, y) = (SIZE / 2.0 + RADIUS * angle.sin(), SIZE / 2.0 - RADIUS * angle.cos());
            let selected = m.selected == Some(i);
            let content = container(item_text(&item.label, None, item.submenu, 15))
                .center_x(CELL_W)
                .center_y(CELL_H)
                .style(cell(selected));
            layers.push(pin(content).x(x - CELL_W / 2.0).y(y - CELL_H / 2.0).into());
        }
        let center = container(text("●").size(18).color(MUTED)).center_x(30).center_y(30);
        layers.push(pin(center).x(SIZE / 2.0 - 15.0).y(SIZE / 2.0 - 15.0).into());
        stack(layers).width(SIZE).height(SIZE).into()
    }

    fn cascade(m: &MenuView) -> Element<'_, Message> {
        let slot = |i: usize| -> Element<'_, Message> {
            match m.items.get(i).filter(|item| !item.label.is_empty()) {
                Some(item) => container(item_text(&item.label, item.button.as_deref(), item.submenu, 16))
                    .center_x(190)
                    .center_y(52)
                    .style(cell(false))
                    .into(),
                None => space().width(190).height(52).into(),
            }
        };
        column![
            slot(0),
            row![slot(3), space().width(40), slot(1)].align_y(Alignment::Center),
            slot(2),
        ]
        .spacing(10)
        .align_x(Alignment::Center)
        .into()
    }

    fn list(m: &MenuView) -> Element<'_, Message> {
        let mut col = column![].spacing(6).width(360);
        for (i, item) in m.items.iter().enumerate() {
            let selected = m.selected == Some(i);
            col = col.push(
                container(item_text(&item.label, item.button.as_deref(), item.submenu, 17))
                    .padding([10, 14])
                    .width(Length::Fill)
                    .style(cell(selected)),
            );
        }
        col.into()
    }

    fn carousel(m: &MenuView) -> Element<'_, Message> {
        let n = m.items.len();
        let selected = m.selected.unwrap_or(0);
        let mut line = row![text("◀").size(22).color(MUTED)].spacing(12).align_y(Alignment::Center);
        // The selected item in the middle, with up to two neighbors on each side.
        let shown = n.min(5) as i32;
        for offset in -(shown / 2)..=(shown - 1 - shown / 2) {
            let i = (selected as i32 + offset).rem_euclid(n as i32) as usize;
            let item = &m.items[i];
            let big = offset == 0;
            line = line.push(
                container(item_text(&item.label, None, item.submenu, if big { 19 } else { 14 }))
                    .center_x(if big { 170 } else { 120 })
                    .center_y(if big { 80 } else { 60 })
                    .style(cell(big)),
            );
        }
        line.push(text("▶").size(22).color(MUTED)).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(c: &OverlayController) -> &'static str {
        keyboard::key_at(c.cursor()).code
    }

    fn ms(t: Instant, n: u64) -> Instant {
        t + Duration::from_millis(n)
    }

    fn start() -> OverlayController {
        OverlayController::new(keyboard::find("KEY_Q").unwrap())
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
        let mut c = OverlayController::new(keyboard::find("KEY_LEFTSHIFT").unwrap());
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
    fn face_button_shortcuts() {
        let mut c = start();
        let t0 = Instant::now();
        let tap = |k| vec![OverlayAction::Key(k, true), OverlayAction::Key(k, false)];
        assert_eq!(c.handle(InputEvent::Button(Button::West, true), t0), tap(KeyCode::KEY_BACKSPACE));
        assert_eq!(c.handle(InputEvent::Button(Button::North, true), t0), tap(KeyCode::KEY_SPACE));
        assert_eq!(c.handle(InputEvent::Button(Button::Start, true), t0), tap(KeyCode::KEY_ENTER));
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
        let mut c = OverlayController::new(keyboard::find("KEY_LEFTCTRL").unwrap());
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

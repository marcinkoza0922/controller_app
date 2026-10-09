//! On-screen action menus: how controller input moves through them. The daemon routes all
//! controller input here while a menu is open; the overlay process draws `MenuView`.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{
    config::{Button, ButtonAction, CarouselControls, Keyword, Menu, MenuKind, OverlayStyle, Stick, Trigger},
    engine::Opener,
    input::{Axis, InputEvent},
};

/// Stick deflection that aims a radial menu or steps a list.
const STICK_PRESS: f32 = 0.6;
const STICK_RELEASE: f32 = 0.45;
/// A trigger opener counts as let go below this.
const TRIGGER_RELEASE: f32 = 0.3;
/// Trigger pull that steps a trigger-driven carousel.
const TRIGGER_PRESS: f32 = 0.6;
const REPEAT_DELAY: Duration = Duration::from_millis(350);
const REPEAT_EVERY: Duration = Duration::from_millis(110);

/// What the overlay draws for an open menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MenuView {
    pub title: String,
    pub kind: MenuKind,
    pub items: Vec<ItemView>,
    pub selected: Option<usize>,
    /// How many items have been chosen in this menu's session; changes on each pick.
    #[serde(default)]
    pub picks: u32,
    /// The menus this one was opened from, outermost first. Shown above the title.
    #[serde(default)]
    pub crumbs: Vec<String>,
    /// Short reminder of the controls.
    pub hint: String,
    #[serde(default)]
    pub style: OverlayStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemView {
    pub label: String,
    /// Button that picks it directly (quick-select, or the cascade slot).
    pub button: Option<String>,
    /// Opens another menu rather than acting.
    pub submenu: bool,
    /// Buttons drawn as their controller glyphs, in front of the label.
    #[serde(default)]
    pub buttons: Vec<Button>,
    /// What kind of row it is, for its background.
    #[serde(default)]
    pub tone: Tone,
    /// The keyword the row's action starts with (Macro, Turbo…), drawn as its icon in front of the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<Keyword>,
    /// The share of a radial menu's circle its arc takes.
    #[serde(default = "crate::config::default_weight")]
    pub weight: f32,
}

/// What kind of row an item is. The overlay tints each kind differently, so a row that adds to
/// something or removes it stands out from the things it acts on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tone {
    #[default]
    Normal,
    /// Adds something: a new item, a new step.
    Add,
    /// Removes something, or deletes it.
    Remove,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuOutcome {
    /// Run this item's action; `close` says whether the menu goes away too.
    Choose { menu: String, item: usize, action: ButtonAction, close: bool },
    Close,
}

/// What keeps a menu on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Up while the opener is held; letting go closes it (a radial menu chooses first).
    Held,
    /// Opened by a Toggle: up until the opener is pressed again.
    Toggled,
    /// Opened by something that can't be held (the command line, an analog zone): up until
    /// an item is chosen or the menu's back button closes it.
    Unwatched,
}

/// A stick-direction opener counts as let go once the stick is back within this.
const DIRECTION_RELEASE: f32 = 0.25;

/// One step through a list, grid or carousel: (x, y), each -1, 0 or 1; y is positive down.
type Step = (i32, i32);

struct Frame {
    menu: usize,
    cursor: Option<usize>,
}

/// One open menu (and any submenus opened from it, which close with it).
pub struct MenuSession {
    stack: Vec<Frame>,
    opener: Opener,
    mode: Mode,
    /// Whether the opener was down after the last event.
    opener_down: bool,
    /// A toggled menu closes on the opener's next press, once it has been let go.
    armed: bool,
    axes: HashMap<Axis, f32>,
    held: HashSet<Button>,
    /// Direction (x, y) stepping through a list, grid or carousel, and when it repeats.
    stepping: Option<(Step, Instant)>,
    /// Which step direction the stick currently holds.
    stick_step: Option<Step>,
    /// Which carousel trigger is currently pulled past the step point.
    trigger_step: Option<Trigger>,
    /// How many items have been chosen so far, shown to the overlay so it can animate the pick.
    picks: u32,
}

fn find(menus: &[Menu], name: &str) -> Option<usize> {
    menus.iter().position(|m| m.name == name)
}

/// A label for a button in hints and quick-select badges.
pub fn button_badge(b: Button) -> &'static str {
    match b {
        Button::South => "A",
        Button::East => "B",
        Button::North => "Y",
        Button::West => "X",
        Button::LeftBumper => "LB",
        Button::RightBumper => "RB",
        Button::Select => "Select",
        Button::Start => "Start",
        Button::Guide => "Guide",
        Button::LeftStick => "L3",
        Button::RightStick => "R3",
        Button::DpadUp => "D↑",
        Button::DpadDown => "D↓",
        Button::DpadLeft => "D←",
        Button::DpadRight => "D→",
        Button::LeftStickUp => "LS↑",
        Button::LeftStickDown => "LS↓",
        Button::LeftStickLeft => "LS←",
        Button::LeftStickRight => "LS→",
        Button::RightStickUp => "RS↑",
        Button::RightStickDown => "RS↓",
        Button::RightStickLeft => "RS←",
        Button::RightStickRight => "RS→",
        Button::LeftPaddle => "LP",
        Button::RightPaddle => "RP",
        Button::LeftPaddle2 => "LP2",
        Button::RightPaddle2 => "RP2",
    }
}

impl MenuSession {
    /// Opens the menu named `name`, or `None` if there is no such menu or it is empty.
    pub fn open(menus: &[Menu], name: &str, opener: Opener) -> Option<Self> {
        let menu = find(menus, name)?;
        if menus[menu].items.is_empty() {
            return None;
        }
        let watchable = !opener.buttons.is_empty() || opener.trigger.is_some();
        let mode = match (opener.toggled, watchable) {
            (_, false) => Mode::Unwatched,
            (true, true) => Mode::Toggled,
            (false, true) => Mode::Held,
        };
        let mut session = MenuSession {
            stack: Vec::new(),
            opener,
            mode,
            // Assumed held, as it just opened the menu; `prime` checks.
            opener_down: true,
            armed: false,
            axes: HashMap::new(),
            held: HashSet::new(),
            stepping: None,
            stick_step: None,
            trigger_step: None,
            picks: 0,
        };
        // Until `prime` says otherwise, the opener is taken to be held (it just fired).
        for b in session.opener.buttons.clone() {
            match b.stick_direction() {
                Some((stick, (dx, dy))) => {
                    let (ax, ay) = stick_axes(stick);
                    session.axes.insert(ax, dx);
                    session.axes.insert(ay, dy);
                }
                None => {
                    session.held.insert(b);
                }
            }
        }
        if let Some(t) = session.opener.trigger {
            session.axes.insert(trigger_axis(t), 1.0);
        }
        session.push(menus, menu);
        Some(session)
    }

    /// Tells the session what the controller is holding as the menu opens. Returns false if
    /// the opener is already let go (e.g. a quick tap), so the menu shouldn't show at all;
    /// a tap opens a menu only through a Toggle.
    #[must_use]
    pub fn prime(&mut self, menus: &[Menu], buttons: impl IntoIterator<Item = Button>, axes: impl IntoIterator<Item = (Axis, f32)>) -> bool {
        self.held = buttons.into_iter().collect();
        self.axes = axes.into_iter().collect();
        self.opener_down = self.opener_is_down();
        match self.mode {
            Mode::Held if !self.opener_down => return false,
            Mode::Toggled if !self.opener_down => self.armed = true,
            _ => {}
        }
        if let Some(menu) = self.top(menus).cloned()
            && let MenuKind::Radial { stick, .. } = menu.kind
        {
            self.aim(&menu, stick);
        }
        true
    }

    fn opener_is_down(&self) -> bool {
        let buttons = self.opener.buttons.iter().all(|b| match b.stick_direction() {
            Some((stick, (dx, dy))) => {
                let (ax, ay) = stick_axes(stick);
                let (x, y) = (self.axis(ax), self.axis(ay));
                x * dx + y * dy >= DIRECTION_RELEASE
            }
            None => self.held.contains(b),
        });
        let trigger = self.opener.trigger.is_none_or(|t| self.axis(trigger_axis(t)) >= TRIGGER_RELEASE);
        buttons && trigger
    }

    fn axis(&self, axis: Axis) -> f32 {
        self.axes.get(&axis).copied().unwrap_or(0.0)
    }

    /// The input that opened the menu, for hints.
    fn opener_label(&self) -> String {
        let mut parts: Vec<&str> = self.opener.buttons.iter().map(|b| button_badge(*b)).collect();
        if let Some(t) = self.opener.trigger {
            parts.push(if t == Trigger::Left { "LT" } else { "RT" });
        }
        parts.join("+")
    }

    fn push(&mut self, menus: &[Menu], menu: usize) {
        let cursor = match menus[menu].kind {
            MenuKind::Radial { .. } | MenuKind::Directional { .. } => None,
            MenuKind::List | MenuKind::Buttons | MenuKind::Carousel { .. } | MenuKind::Grid { .. } => Some(0),
        };
        self.stack.push(Frame { menu, cursor });
        self.stepping = None;
    }

    fn top<'m>(&self, menus: &'m [Menu]) -> Option<&'m Menu> {
        self.stack.last().and_then(|f| menus.get(f.menu))
    }

    pub fn view(&self, menus: &[Menu]) -> Option<MenuView> {
        let frame = self.stack.last()?;
        let menu = menus.get(frame.menu)?;
        let slots = match menu.kind {
            MenuKind::Directional { cluster } => Some(cluster.slots()),
            _ => None,
        };
        let items = menu
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| ItemView {
                label: item.label.clone(),
                button: match (slots, item.button) {
                    (Some(slots), _) => slots.get(i).map(|b| button_badge(*b).to_string()),
                    (None, Some(b)) => Some(button_badge(b).to_string()),
                    (None, None) => None,
                },
                submenu: matches!(item.action, ButtonAction::OpenMenu(_)),
                weight: item.weight,
                buttons: Vec::new(),
                tone: Tone::Normal,
                keyword: None,
            })
            .collect();
        let radial = matches!(menu.kind, MenuKind::Radial { .. });
        // How the menu goes away (and, for a radial menu, chooses).
        let close = match (self.mode, radial) {
            (Mode::Held, true) => "let go to choose".to_string(),
            (Mode::Held, false) => "let go to close".to_string(),
            (Mode::Toggled, true) => format!("{} again to choose", self.opener_label()),
            (Mode::Toggled, false) => format!("{} again to close", self.opener_label()),
            (Mode::Unwatched, true) => format!("A choose · {} close", button_badge(menu.cancel_button())),
            (Mode::Unwatched, false) => {
                let back = if self.stack.len() > 1 { "back" } else { "close" };
                format!("{} {back}", button_badge(menu.cancel_button()))
            }
        };
        let hint = match menu.kind {
            MenuKind::Radial { stick, .. } => {
                let stick = if stick == Stick::Left { "left stick" } else { "right stick" };
                format!("Aim the {stick} · {close}")
            }
            MenuKind::Directional { cluster } => format!("Press a {} direction · {close}", cluster.to_string().to_lowercase()),
            MenuKind::List => format!("↑↓ move · A choose · {close}"),
            MenuKind::Buttons => format!("Press an item's button, or ↑↓ and A · {close}"),
            MenuKind::Carousel { controls } => format!("{controls} to cycle · A choose · {close}"),
            MenuKind::Grid { .. } => format!("↑↓←→ move · A choose · {close}"),
        };
        Some(MenuView {
            title: menu.name.clone(),
            kind: menu.kind,
            items,
            selected: frame.cursor,
            picks: self.picks,
            crumbs: self.stack[..self.stack.len() - 1].iter().filter_map(|f| menus.get(f.menu)).map(|m| m.name.clone()).collect(),
            hint,
            style: menu.style.clone(),
        })
    }

    /// The item the cursor is on in the menu on top, if it has one.
    pub fn cursor(&self) -> Option<usize> {
        self.stack.last().and_then(|f| f.cursor)
    }

    /// Handles controller input; returns what to do, if anything.
    pub fn handle(&mut self, menus: &[Menu], ev: InputEvent, now: Instant) -> Option<MenuOutcome> {
        let outcome = self.act(menus, ev, now);
        if matches!(outcome, Some(MenuOutcome::Choose { .. })) {
            self.picks += 1;
        }
        outcome
    }

    fn act(&mut self, menus: &[Menu], ev: InputEvent, now: Instant) -> Option<MenuOutcome> {
        let menu = self.top(menus)?.clone();
        let mut fresh_press = false;
        match ev {
            InputEvent::Button(b, true) => fresh_press = self.held.insert(b),
            InputEvent::Button(b, false) => {
                self.held.remove(&b);
                if self.stepping.is_some() && matches!(b, Button::DpadUp | Button::DpadDown | Button::DpadLeft | Button::DpadRight) {
                    self.stepping = None;
                }
            }
            InputEvent::Axis(axis, value) => {
                self.axes.insert(axis, value);
            }
            InputEvent::Touchpad(_) => {}
        }

        let down = self.opener_is_down();
        let was_down = std::mem::replace(&mut self.opener_down, down);
        match self.mode {
            Mode::Held if was_down && !down => return Some(self.finish(&menu)),
            Mode::Toggled if !down => self.armed = true,
            Mode::Toggled if self.armed && !was_down => return Some(self.finish(&menu)),
            _ => {}
        }

        match ev {
            // The opener's own buttons only hold the menu up.
            InputEvent::Button(b, true) if fresh_press && !self.opener.buttons.contains(&b) => {
                self.button_pressed(menus, &menu, b, now)
            }
            InputEvent::Button(..) => None,
            InputEvent::Axis(axis, value) => self.axis_moved(&menu, axis, value, now),
            InputEvent::Touchpad(_) => None,
        }
    }

    /// The opener was let go (or pressed again, for a toggled menu): a radial menu chooses
    /// what it's aimed at, and the menu closes with all its submenus.
    fn finish(&mut self, menu: &Menu) -> MenuOutcome {
        let aimed = self.stack.last().and_then(|f| f.cursor);
        if let MenuKind::Radial { .. } = menu.kind
            && let Some(i) = aimed
            && let Some(item) = menu.items.get(i)
            && !matches!(item.action, ButtonAction::Disabled | ButtonAction::OpenMenu(_))
        {
            return MenuOutcome::Choose { menu: menu.name.clone(), item: i, action: item.action.clone(), close: true };
        }
        MenuOutcome::Close
    }

    fn button_pressed(&mut self, menus: &[Menu], menu: &Menu, b: Button, now: Instant) -> Option<MenuOutcome> {
        // A menu that can't be let go of needs a button to back out with.
        if self.mode == Mode::Unwatched && b == menu.cancel_button() {
            return self.back();
        }
        match menu.kind {
            MenuKind::Directional { cluster } => {
                let slot = cluster.slots().iter().position(|s| *s == b)?;
                self.choose(menus, slot)
            }
            MenuKind::Radial { .. } => {
                // With nothing to let go of, A chooses.
                (self.mode == Mode::Unwatched && b == Button::South).then(|| self.choose_cursor(menus)).flatten()
            }
            MenuKind::Buttons => {
                if let Some(i) = menu.items.iter().position(|item| item.button == Some(b)) {
                    return self.choose(menus, i);
                }
                self.list_button(menus, menu, b, now)
            }
            MenuKind::List | MenuKind::Grid { .. } => self.list_button(menus, menu, b, now),
            MenuKind::Carousel { controls } => {
                let step = match (controls, b) {
                    (CarouselControls::Bumpers, Button::LeftBumper) | (CarouselControls::DPad, Button::DpadLeft) => Some((-1, 0)),
                    (CarouselControls::Bumpers, Button::RightBumper) | (CarouselControls::DPad, Button::DpadRight) => Some((1, 0)),
                    _ => None,
                };
                if let Some(step) = step {
                    self.start_stepping(menu, step, now);
                    return None;
                }
                (b == Button::South).then(|| self.choose_cursor(menus)).flatten()
            }
        }
    }

    /// The D-pad moves a list up and down, and a grid in all four directions; A chooses.
    fn list_button(&mut self, menus: &[Menu], menu: &Menu, b: Button, now: Instant) -> Option<MenuOutcome> {
        let grid = matches!(menu.kind, MenuKind::Grid { .. });
        match b {
            Button::DpadUp => self.start_stepping(menu, (0, -1), now),
            Button::DpadDown => self.start_stepping(menu, (0, 1), now),
            Button::DpadLeft if grid => self.start_stepping(menu, (-1, 0), now),
            Button::DpadRight if grid => self.start_stepping(menu, (1, 0), now),
            Button::South => return self.choose_cursor(menus),
            _ => {}
        }
        None
    }

    fn axis_moved(&mut self, menu: &Menu, axis: Axis, value: f32, now: Instant) -> Option<MenuOutcome> {
        // A trigger opener only holds the menu up.
        if self.opener.trigger.is_some_and(|t| axis == trigger_axis(t)) {
            return None;
        }
        match menu.kind {
            MenuKind::Radial { stick, .. } => {
                let (ax, ay) = stick_axes(stick);
                if axis == ax || axis == ay {
                    self.aim(menu, stick);
                }
                None
            }
            MenuKind::List | MenuKind::Buttons => {
                if axis == Axis::LeftY {
                    self.stick_step(menu, (0.0, value), now);
                }
                None
            }
            MenuKind::Grid { .. } => {
                if matches!(axis, Axis::LeftX | Axis::LeftY) {
                    self.stick_step(menu, (self.axis(Axis::LeftX), self.axis(Axis::LeftY)), now);
                }
                None
            }
            MenuKind::Carousel { controls } => {
                match (controls, axis) {
                    (CarouselControls::LeftStick, Axis::LeftX) | (CarouselControls::RightStick, Axis::RightX) => {
                        self.stick_step(menu, (value, 0.0), now)
                    }
                    (CarouselControls::Triggers, Axis::LeftTrigger | Axis::RightTrigger) => {
                        let t = if axis == Axis::LeftTrigger { Trigger::Left } else { Trigger::Right };
                        if value >= TRIGGER_PRESS && self.trigger_step.is_none() {
                            self.trigger_step = Some(t);
                            self.start_stepping(menu, (if t == Trigger::Left { -1 } else { 1 }, 0), now);
                        } else if value < TRIGGER_RELEASE && self.trigger_step == Some(t) {
                            self.trigger_step = None;
                            self.stepping = None;
                        }
                    }
                    _ => {}
                }
                None
            }
            MenuKind::Directional { .. } => None,
        }
    }

    /// A stick used like a D-pad: up/down for lists, left/right for carousels, all four for
    /// grids (whichever way it leans most).
    fn stick_step(&mut self, menu: &Menu, (x, y): (f32, f32), now: Instant) {
        let threshold = if self.stick_step.is_some() { STICK_RELEASE } else { STICK_PRESS };
        let dir = if x.abs().max(y.abs()) < threshold {
            None
        } else if x.abs() > y.abs() {
            Some((x.signum() as i32, 0))
        } else {
            Some((0, y.signum() as i32))
        };
        if dir == self.stick_step {
            return;
        }
        self.stick_step = dir;
        match dir {
            Some(d) => self.start_stepping(menu, d, now),
            None => self.stepping = None,
        }
    }

    /// Points a radial menu's selection where the stick is aimed (first item at the top,
    /// going clockwise). The last aim sticks if the stick springs back to center.
    fn aim(&mut self, menu: &Menu, stick: Stick) {
        let (ax, ay) = stick_axes(stick);
        let x = self.axes.get(&ax).copied().unwrap_or(0.0);
        let y = self.axes.get(&ay).copied().unwrap_or(0.0);
        if x.hypot(y) < STICK_PRESS {
            return;
        }
        // Clockwise from straight up; y is positive down.
        let angle = x.atan2(-y).to_degrees().rem_euclid(360.0);
        let weights: Vec<f32> = menu.items.iter().map(|item| item.weight).collect();
        let boxes = matches!(menu.kind, MenuKind::Radial { boxes: true, .. });
        if let Some(i) = crate::radial::index_at(&crate::radial::for_menu(boxes, &weights), angle)
            && let Some(frame) = self.stack.last_mut()
        {
            frame.cursor = Some(i);
        }
    }

    fn start_stepping(&mut self, menu: &Menu, dir: Step, now: Instant) {
        self.step(menu, dir);
        self.stepping = Some((dir, now + REPEAT_DELAY));
    }

    fn step(&mut self, menu: &Menu, (dx, dy): Step) {
        let n = menu.items.len();
        if let Some(frame) = self.stack.last_mut()
            && n > 0
        {
            let cursor = frame.cursor.unwrap_or(0).min(n - 1);
            frame.cursor = Some(match menu.kind.grid_columns() {
                Some(columns) => grid_step(cursor, n, columns, (dx, dy)),
                None if matches!(menu.kind, MenuKind::List | MenuKind::Buttons) => list_step(cursor, n, (dx, dy)),
                // Carousels are one line, whichever way it runs.
                None => (cursor as i32 + dx + dy).rem_euclid(n as i32) as usize,
            });
        }
    }

    fn choose_cursor(&mut self, menus: &[Menu]) -> Option<MenuOutcome> {
        let i = self.stack.last()?.cursor?;
        self.choose(menus, i)
    }

    /// Chooses item `i`: opens its submenu, or hands back its action to run. A held or
    /// toggled menu stays up after an action, so several can be picked in one go.
    fn choose(&mut self, menus: &[Menu], i: usize) -> Option<MenuOutcome> {
        let menu = self.top(menus)?;
        let item = menu.items.get(i)?;
        match &item.action {
            ButtonAction::Disabled => None,
            ButtonAction::OpenMenu(name) => {
                let sub = find(menus, name).filter(|m| can_open(menu, &menus[*m]))?;
                self.push(menus, sub);
                None
            }
            action => Some(MenuOutcome::Choose {
                menu: menu.name.clone(),
                item: i,
                action: action.clone(),
                close: self.mode == Mode::Unwatched,
            }),
        }
    }

    /// Cancel: back out of a submenu, or close.
    fn back(&mut self) -> Option<MenuOutcome> {
        self.stack.pop();
        self.stepping = None;
        if self.stack.is_empty() { Some(MenuOutcome::Close) } else { None }
    }

    /// List/carousel repeat while a direction is held. Returns whether the view changed.
    pub fn tick(&mut self, menus: &[Menu], now: Instant) -> bool {
        let Some((dir, mut next)) = self.stepping else { return false };
        let Some(menu) = self.top(menus).cloned() else { return false };
        let mut changed = false;
        while now >= next {
            self.step(&menu, dir);
            next += REPEAT_EVERY;
            changed = true;
        }
        self.stepping = Some((dir, next));
        changed
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.stepping.map(|(_, t)| t)
    }
}

/// A list longer than this many rows is shown in two columns.
pub const ONE_COLUMN_ROWS: usize = 8;
/// Rows each column shows at once; a longer column scrolls with the cursor.
pub const VISIBLE_ROWS: usize = 8;

/// How many columns a list of `n` rows is shown in.
pub fn list_columns(n: usize) -> usize {
    if n > ONE_COLUMN_ROWS { 2 } else { 1 }
}

/// Moves the cursor through a list of `n` rows, as the overlay shows it. A two-column list
/// fills the left column first, then the right. Up and down follow that order, wrapping round the
/// whole list. Left and right move to the other column, on the same row or its last one.
pub fn list_step(cursor: usize, n: usize, (dx, dy): Step) -> usize {
    if n == 0 {
        return 0;
    }
    if list_columns(n) == 1 || dx == 0 {
        // Up and down go through the list in order, from the bottom of one column to the top of
        // the next and round again.
        return (cursor as i32 + dx + dy).rem_euclid(n as i32) as usize;
    }
    let half = n.div_ceil(2);
    let col = usize::from(cursor >= half);
    let row = cursor - col * half;
    let other_len = if col == 0 { n - half } else { half };
    (1 - col) * half + row.min(other_len - 1)
}

/// The first row a two-column list shows, so the cursor's row is on screen. Both columns share
/// it, so their rows line up. A one-column list always starts at the top.
pub fn list_start(cursor: usize, n: usize) -> usize {
    if list_columns(n) == 1 {
        return 0;
    }
    let half = n.div_ceil(2);
    let row = cursor.min(n - 1) - usize::from(cursor >= half) * half;
    if row >= VISIBLE_ROWS { row + 1 - VISIBLE_ROWS } else { 0 }
}

/// Moves through `n` items laid out `columns` wide, wrapping within the row or column (the
/// last row may be short, so its columns are too).
fn grid_step(cursor: usize, n: usize, columns: usize, (dx, dy): Step) -> usize {
    let (col, row) = (cursor % columns, cursor / columns);
    let row_len = (n - row * columns).min(columns);
    let col_len = (n - col).div_ceil(columns);
    let col = (col as i32 + dx).rem_euclid(row_len as i32) as usize;
    let row = (row as i32 + dy).rem_euclid(col_len as i32) as usize;
    row * columns + col
}

/// Whether `parent` may open `child` as a submenu: only menus of the same kind, and never
/// from a radial menu (aiming a second wheel with the same release is awkward).
pub fn can_open(parent: &Menu, child: &Menu) -> bool {
    !matches!(parent.kind, MenuKind::Radial { .. }) && parent.kind.tag() == child.kind.tag() && !child.items.is_empty()
}

fn stick_axes(s: Stick) -> (Axis, Axis) {
    match s {
        Stick::Left => (Axis::LeftX, Axis::LeftY),
        Stick::Right => (Axis::RightX, Axis::RightY),
    }
}

fn trigger_axis(t: Trigger) -> Axis {
    match t {
        Trigger::Left => Axis::LeftTrigger,
        Trigger::Right => Axis::RightTrigger,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_two_column_list_moves_down_and_up_through_both_columns_and_left_and_right_switch() {
        // Ten rows: left column 0..5, right column 5..10.
        assert_eq!(list_step(4, 10, (0, 1)), 5, "down from the bottom of the left goes to the top of the right");
        assert_eq!(list_step(5, 10, (0, -1)), 4, "up from the top of the right goes to the bottom of the left");
        assert_eq!(list_step(9, 10, (0, 1)), 0, "down from the bottom of the right goes to the first option");
        assert_eq!(list_step(0, 10, (0, -1)), 9, "up from the first option goes to the bottom of the right");
        assert_eq!(list_step(2, 10, (1, 0)), 7, "right moves to the same row of the right column");
        assert_eq!(list_step(9, 10, (-1, 0)), 4, "left from the bottom of the right lands on the left column's last row");
        // A one-column list keeps its linear wrap.
        assert_eq!(list_step(7, 8, (0, 1)), 0);
        assert_eq!(list_step(0, 8, (0, -1)), 7);
    }

    #[test]
    fn a_two_column_list_scrolls_only_once_the_cursor_leaves_the_visible_rows() {
        // Twenty rows: 10 per column, 8 visible per column.
        assert_eq!(list_start(7, 20), 0);
        assert_eq!(list_start(8, 20), 1);
        assert_eq!(list_start(9, 20), 2);
        // The right column's rows scroll with the left's: its row 7 is on screen, row 9 scrolls.
        assert_eq!(list_start(17, 20), 0);
        assert_eq!(list_start(19, 20), 2);
        assert_eq!(list_start(0, 8), 0);
    }

    use super::*;
    use crate::config::{Cluster, GRID_MAX, MenuItem};

    fn key(k: &str) -> ButtonAction {
        ButtonAction::Keys(vec![k.into()])
    }

    fn item(label: &str, action: ButtonAction) -> MenuItem {
        MenuItem { label: label.into(), action, button: None, weight: 1.0 }
    }

    fn menu(name: &str, kind: MenuKind, items: Vec<MenuItem>) -> Menu {
        Menu { name: name.into(), kind, items, cancel: None, style: OverlayStyle::default() }
    }

    fn numbers(n: usize) -> Vec<MenuItem> {
        (1..=n).map(|i| item(&i.to_string(), key(&format!("KEY_{i}")))).collect()
    }

    fn press(s: &mut MenuSession, menus: &[Menu], b: Button) -> Option<MenuOutcome> {
        let now = Instant::now();
        let first = s.handle(menus, InputEvent::Button(b, true), now);
        let second = s.handle(menus, InputEvent::Button(b, false), now);
        first.or(second)
    }

    fn chose(out: Option<MenuOutcome>) -> Option<String> {
        match out {
            Some(MenuOutcome::Choose { action: ButtonAction::Keys(k), .. }) => Some(k[0].clone()),
            _ => None,
        }
    }

    fn held_by(b: Button) -> Opener {
        Opener { buttons: vec![b], ..Opener::default() }
    }

    #[test]
    fn radial_aims_with_the_stick_and_chooses_on_release() {
        let menus = [menu("Weapons", MenuKind::Radial { stick: Stick::Right, boxes: false }, numbers(4))];
        let mut s = MenuSession::open(&menus, "Weapons", held_by(Button::LeftBumper)).unwrap();
        let now = Instant::now();
        // Right on the stick is the second of four items (1 at the top, clockwise).
        s.handle(&menus, InputEvent::Axis(Axis::RightX, 0.9), now);
        assert_eq!(s.view(&menus).unwrap().selected, Some(1));
        // Down is the third; the aim sticks when the stick springs back.
        s.handle(&menus, InputEvent::Axis(Axis::RightX, 0.0), now);
        s.handle(&menus, InputEvent::Axis(Axis::RightY, 0.9), now);
        s.handle(&menus, InputEvent::Axis(Axis::RightY, 0.0), now);
        assert_eq!(s.view(&menus).unwrap().selected, Some(2));
        // Other buttons don't close it; letting go of the opener chooses.
        assert_eq!(s.handle(&menus, InputEvent::Button(Button::South, false), now), None);
        assert_eq!(chose(s.handle(&menus, InputEvent::Button(Button::LeftBumper, false), now)).as_deref(), Some("KEY_3"));
    }

    #[test]
    fn radial_released_without_aiming_just_closes_and_trigger_openers_work() {
        let menus = [menu("W", MenuKind::Radial { stick: Stick::Right, boxes: false }, numbers(8))];
        let mut s = MenuSession::open(&menus, "W", held_by(Button::LeftBumper)).unwrap();
        assert_eq!(s.handle(&menus, InputEvent::Button(Button::LeftBumper, false), Instant::now()), Some(MenuOutcome::Close));

        let opener = Opener { trigger: Some(Trigger::Left), ..Opener::default() };
        let mut s = MenuSession::open(&menus, "W", opener).unwrap();
        let now = Instant::now();
        s.handle(&menus, InputEvent::Axis(Axis::LeftTrigger, 1.0), now);
        // Up-left is the last of eight.
        s.handle(&menus, InputEvent::Axis(Axis::RightX, -0.7), now);
        s.handle(&menus, InputEvent::Axis(Axis::RightY, -0.7), now);
        assert_eq!(chose(s.handle(&menus, InputEvent::Axis(Axis::LeftTrigger, 0.1), now)).as_deref(), Some("KEY_8"));
    }

    #[test]
    fn cascade_fires_slots_and_opens_submenus_with_back() {
        let menus = [
            menu(
                "Root",
                MenuKind::Directional { cluster: Cluster::DPad },
                vec![item("Up", key("KEY_U")), item("More", ButtonAction::OpenMenu("Sub".into()))],
            ),
            menu("Sub", MenuKind::Directional { cluster: Cluster::DPad }, vec![item("Deep", key("KEY_D"))]),
        ];
        let mut s = MenuSession::open(&menus, "Root", Opener::default()).unwrap();
        assert_eq!(press(&mut s, &menus, Button::DpadDown), None, "an empty slot does nothing");
        assert_eq!(press(&mut s, &menus, Button::DpadRight), None, "opens the submenu");
        assert_eq!(s.view(&menus).unwrap().title, "Sub");
        assert_eq!(press(&mut s, &menus, Button::East), None, "B goes back one level");
        assert_eq!(s.view(&menus).unwrap().title, "Root");
        assert_eq!(press(&mut s, &menus, Button::DpadRight), None);
        assert_eq!(chose(press(&mut s, &menus, Button::DpadUp)).as_deref(), Some("KEY_D"));
    }

    #[test]
    fn face_button_cascade_cancels_with_select() {
        let menus = [menu("F", MenuKind::Directional { cluster: Cluster::FaceButtons }, numbers(4))];
        let mut s = MenuSession::open(&menus, "F", Opener::default()).unwrap();
        assert_eq!(chose(press(&mut s, &menus, Button::East)).as_deref(), Some("KEY_2"), "East is slot 2 (right)");
        let mut s = MenuSession::open(&menus, "F", Opener::default()).unwrap();
        assert_eq!(press(&mut s, &menus, Button::Select), Some(MenuOutcome::Close));
    }

    #[test]
    fn list_moves_with_dpad_and_stick_and_wraps() {
        let menus = [menu("Pause", MenuKind::List, numbers(3))];
        let mut s = MenuSession::open(&menus, "Pause", Opener::default()).unwrap();
        press(&mut s, &menus, Button::DpadUp);
        assert_eq!(s.view(&menus).unwrap().selected, Some(2), "wraps to the bottom");
        let now = Instant::now();
        s.handle(&menus, InputEvent::Axis(Axis::LeftY, 0.9), now);
        assert_eq!(s.view(&menus).unwrap().selected, Some(0));
        // Holding repeats.
        assert!(s.tick(&menus, now + REPEAT_DELAY));
        assert_eq!(s.view(&menus).unwrap().selected, Some(1));
        s.handle(&menus, InputEvent::Axis(Axis::LeftY, 0.0), now);
        assert_eq!(s.next_deadline(), None);
        assert_eq!(chose(press(&mut s, &menus, Button::South)).as_deref(), Some("KEY_2"));
        let mut s = MenuSession::open(&menus, "Pause", Opener::default()).unwrap();
        assert_eq!(press(&mut s, &menus, Button::East), Some(MenuOutcome::Close));
    }

    #[test]
    fn grid_moves_in_four_directions_and_wraps_within_rows_and_columns() {
        // 1 2 3
        // 4 5 6
        // 7
        let menus = [menu("G", MenuKind::Grid { columns: 3 }, numbers(7))];
        let mut s = MenuSession::open(&menus, "G", Opener::default()).unwrap();
        let at = |s: &MenuSession| s.view(&menus).unwrap().selected;
        press(&mut s, &menus, Button::DpadRight);
        press(&mut s, &menus, Button::DpadDown);
        assert_eq!(at(&s), Some(4), "item 5");
        press(&mut s, &menus, Button::DpadDown);
        assert_eq!(at(&s), Some(1), "column 2 has no third row, so it wraps to the top");
        press(&mut s, &menus, Button::DpadLeft);
        press(&mut s, &menus, Button::DpadLeft);
        assert_eq!(at(&s), Some(2), "wraps to the end of the row");
        press(&mut s, &menus, Button::DpadRight);
        press(&mut s, &menus, Button::DpadUp);
        assert_eq!(at(&s), Some(6), "column 1 wraps to the short last row");
        press(&mut s, &menus, Button::DpadRight);
        assert_eq!(at(&s), Some(6), "the short row wraps on itself");
        // The left stick moves the way it leans most, and holding repeats.
        let now = Instant::now();
        s.handle(&menus, InputEvent::Axis(Axis::LeftY, -0.9), now);
        assert_eq!(at(&s), Some(3));
        s.handle(&menus, InputEvent::Axis(Axis::LeftX, 0.95), now);
        assert_eq!(at(&s), Some(4), "leaning further right turns it into a step right");
        assert!(s.tick(&menus, now + REPEAT_DELAY));
        assert_eq!(at(&s), Some(5));
        s.handle(&menus, InputEvent::Axis(Axis::LeftY, 0.0), now);
        s.handle(&menus, InputEvent::Axis(Axis::LeftX, 0.0), now);
        assert_eq!(s.next_deadline(), None);
        assert_eq!(chose(press(&mut s, &menus, Button::South)).as_deref(), Some("KEY_6"));
        assert!(s.view(&menus).unwrap().hint.contains("↑↓←→"));
    }

    #[test]
    fn grid_steps_cover_every_cell() {
        for columns in 1..=GRID_MAX {
            for n in 1..=GRID_MAX * GRID_MAX {
                for cursor in 0..n {
                    for dir in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let next = grid_step(cursor, n, columns, dir);
                        assert!(next < n, "{columns} columns, {n} items, {cursor} {dir:?}");
                        assert_eq!(grid_step(next, n, columns, (-dir.0, -dir.1)), cursor, "steps undo");
                    }
                }
            }
        }
    }

    #[test]
    fn button_menu_quick_select_takes_priority() {
        let mut items = numbers(3);
        items[0].button = Some(Button::LeftBumper);
        items[2].button = Some(Button::South);
        let menus = [menu("Q", MenuKind::Buttons, items)];
        let mut s = MenuSession::open(&menus, "Q", Opener::default()).unwrap();
        assert_eq!(s.view(&menus).unwrap().items[0].button.as_deref(), Some("LB"));
        assert_eq!(chose(press(&mut s, &menus, Button::LeftBumper)).as_deref(), Some("KEY_1"));
        let mut s = MenuSession::open(&menus, "Q", Opener::default()).unwrap();
        press(&mut s, &menus, Button::DpadDown);
        assert_eq!(chose(press(&mut s, &menus, Button::South)).as_deref(), Some("KEY_3"), "A is item 3's quick button");
    }

    #[test]
    fn carousel_controls() {
        for (controls, next) in [
            (CarouselControls::Bumpers, InputEvent::Button(Button::RightBumper, true)),
            (CarouselControls::DPad, InputEvent::Button(Button::DpadRight, true)),
            (CarouselControls::RightStick, InputEvent::Axis(Axis::RightX, 0.9)),
            (CarouselControls::Triggers, InputEvent::Axis(Axis::RightTrigger, 1.0)),
        ] {
            let menus = [menu("C", MenuKind::Carousel { controls }, numbers(3))];
            let mut s = MenuSession::open(&menus, "C", Opener::default()).unwrap();
            s.handle(&menus, next, Instant::now());
            assert_eq!(s.view(&menus).unwrap().selected, Some(1), "{controls:?}");
            assert_eq!(chose(press(&mut s, &menus, Button::South)).as_deref(), Some("KEY_2"), "{controls:?}");
        }
    }

    #[test]
    fn held_menu_stays_up_for_picks_and_closes_with_its_submenu_on_release() {
        let menus = [
            menu(
                "Root",
                MenuKind::Directional { cluster: Cluster::DPad },
                vec![item("Up", key("KEY_U")), item("More", ButtonAction::OpenMenu("Sub".into()))],
            ),
            menu("Sub", MenuKind::Directional { cluster: Cluster::DPad }, vec![item("Deep", key("KEY_D"))]),
        ];
        let mut s = MenuSession::open(&menus, "Root", held_by(Button::LeftBumper)).unwrap();
        let picked = press(&mut s, &menus, Button::DpadUp);
        assert!(matches!(picked, Some(MenuOutcome::Choose { close: false, .. })), "{picked:?}");
        assert_eq!(press(&mut s, &menus, Button::East), None, "B doesn't close a held menu");
        press(&mut s, &menus, Button::DpadRight);
        assert_eq!(s.view(&menus).unwrap().title, "Sub");
        assert_eq!(chose(press(&mut s, &menus, Button::DpadUp)).as_deref(), Some("KEY_D"));
        assert_eq!(s.view(&menus).unwrap().title, "Sub", "still up after a pick");
        // Letting go closes the submenu and its parent together.
        assert_eq!(s.handle(&menus, InputEvent::Button(Button::LeftBumper, false), Instant::now()), Some(MenuOutcome::Close));
    }

    #[test]
    fn submenus_must_match_and_radial_menus_open_none() {
        let menus = [
            menu("List", MenuKind::List, vec![item("Wheel", ButtonAction::OpenMenu("Wheel".into()))]),
            menu("Wheel", MenuKind::Radial { stick: Stick::Right, boxes: false }, vec![item("Back", ButtonAction::OpenMenu("List".into()))]),
        ];
        let mut s = MenuSession::open(&menus, "List", held_by(Button::LeftBumper)).unwrap();
        assert_eq!(press(&mut s, &menus, Button::South), None);
        assert_eq!(s.view(&menus).unwrap().title, "List", "a list doesn't open a radial menu");
        let mut s = MenuSession::open(&menus, "Wheel", held_by(Button::LeftBumper)).unwrap();
        s.handle(&menus, InputEvent::Axis(Axis::RightY, -0.9), Instant::now());
        assert_eq!(s.handle(&menus, InputEvent::Button(Button::LeftBumper, false), Instant::now()), Some(MenuOutcome::Close));
        assert!(!can_open(&menus[1], &menus[0]) && !can_open(&menus[0], &menus[1]));
        assert!(can_open(&menus[0], &menus[0]));
    }

    #[test]
    fn toggled_menu_closes_on_the_next_press() {
        let menus = [menu("W", MenuKind::Radial { stick: Stick::Right, boxes: false }, numbers(4))];
        let opener = Opener { buttons: vec![Button::LeftBumper], toggled: true, ..Opener::default() };
        let mut s = MenuSession::open(&menus, "W", opener).unwrap();
        let now = Instant::now();
        assert_eq!(s.handle(&menus, InputEvent::Button(Button::LeftBumper, false), now), None, "stays up when let go");
        s.handle(&menus, InputEvent::Axis(Axis::RightX, 0.9), now);
        assert!(s.view(&menus).unwrap().hint.contains("LB again"));
        assert_eq!(chose(s.handle(&menus, InputEvent::Button(Button::LeftBumper, true), now)).as_deref(), Some("KEY_2"));
    }

    #[test]
    fn openers_already_let_go_open_nothing_unless_toggled() {
        let menus = [menu("L", MenuKind::List, numbers(2))];
        let mut s = MenuSession::open(&menus, "L", held_by(Button::LeftBumper)).unwrap();
        assert!(!s.prime(&menus, [], []), "a tap can't hold a menu up");
        let toggled = Opener { buttons: vec![Button::LeftBumper], toggled: true, ..Opener::default() };
        let mut s = MenuSession::open(&menus, "L", toggled).unwrap();
        assert!(s.prime(&menus, [], []));
        assert_eq!(s.handle(&menus, InputEvent::Button(Button::LeftBumper, true), Instant::now()), Some(MenuOutcome::Close));
        // With nothing to hold (the command line), B closes it.
        let mut s = MenuSession::open(&menus, "L", Opener::default()).unwrap();
        assert!(s.prime(&menus, [], []));
        assert!(s.view(&menus).unwrap().hint.contains("B close"));
        assert_eq!(press(&mut s, &menus, Button::East), Some(MenuOutcome::Close));
    }

    #[test]
    fn stick_direction_openers_hold_the_menu() {
        let menus = [menu("L", MenuKind::List, numbers(2))];
        let mut s = MenuSession::open(&menus, "L", held_by(Button::LeftStickUp)).unwrap();
        assert!(s.prime(&menus, [], [(Axis::LeftY, -0.9)]));
        let now = Instant::now();
        assert_eq!(s.handle(&menus, InputEvent::Axis(Axis::LeftY, -0.5), now), None);
        assert_eq!(s.handle(&menus, InputEvent::Axis(Axis::LeftY, -0.1), now), Some(MenuOutcome::Close));
    }

    #[test]
    fn missing_or_empty_menus_do_not_open() {
        let menus = [menu("Empty", MenuKind::List, vec![])];
        assert!(MenuSession::open(&menus, "Empty", Opener::default()).is_none());
        assert!(MenuSession::open(&menus, "Nope", Opener::default()).is_none());
    }
}

//! On-screen action menus: how controller input moves through them. The daemon routes all
//! controller input here while a menu is open; the overlay process draws `MenuView`.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{
    config::{Button, ButtonAction, CarouselControls, Menu, MenuKind, Stick, Trigger},
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
    /// How many menus deep (submenus opened from items).
    pub depth: usize,
    /// Short reminder of the controls.
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemView {
    pub label: String,
    /// Button that picks it directly (quick-select, or the cascade slot).
    pub button: Option<String>,
    /// Opens another menu rather than acting.
    pub submenu: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuOutcome {
    /// Run this item's action (the menu has closed).
    Choose { menu: String, item: usize, action: ButtonAction },
    Close,
}

struct Frame {
    menu: usize,
    cursor: Option<usize>,
}

/// One open menu (and any submenus opened from it).
pub struct MenuSession {
    stack: Vec<Frame>,
    opener: Opener,
    axes: HashMap<Axis, f32>,
    held: HashSet<Button>,
    /// Direction stepping through a list or carousel, and when it repeats.
    stepping: Option<(i32, Instant)>,
    /// Which list-step direction the stick currently holds.
    stick_step: Option<i32>,
    /// Which carousel trigger is currently pulled past the step point.
    trigger_step: Option<Trigger>,
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
        Button::DpadUp => "↑",
        Button::DpadDown => "↓",
        Button::DpadLeft => "←",
        Button::DpadRight => "→",
        _ => "",
    }
}

impl MenuSession {
    /// Opens the menu named `name`, or `None` if there is no such menu or it is empty.
    pub fn open(menus: &[Menu], name: &str, opener: Opener) -> Option<Self> {
        let menu = find(menus, name)?;
        if menus[menu].items.is_empty() {
            return None;
        }
        let mut session = MenuSession {
            stack: Vec::new(),
            opener,
            axes: HashMap::new(),
            held: HashSet::new(),
            stepping: None,
            stick_step: None,
            trigger_step: None,
        };
        session.push(menus, menu);
        Some(session)
    }

    fn push(&mut self, menus: &[Menu], menu: usize) {
        let cursor = match menus[menu].kind {
            MenuKind::Radial { .. } | MenuKind::Cascade { .. } => None,
            MenuKind::List | MenuKind::Buttons | MenuKind::Carousel { .. } => Some(0),
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
            MenuKind::Cascade { cluster } => Some(cluster.slots()),
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
            })
            .collect();
        let cancel = button_badge(menu.cancel_button());
        let back = if self.stack.len() > 1 { "back" } else { "close" };
        let hint = match menu.kind {
            MenuKind::Radial { stick } => {
                let stick = if stick == Stick::Left { "left stick" } else { "right stick" };
                format!("Aim the {stick}, release to choose · {cancel} {back}")
            }
            MenuKind::Cascade { cluster } => format!("Press a {} direction · {cancel} {back}", cluster.to_string().to_lowercase()),
            MenuKind::List => format!("↑↓ move · A choose · {cancel} {back}"),
            MenuKind::Buttons => format!("Press an item's button, or ↑↓ and A · {cancel} {back}"),
            MenuKind::Carousel { controls } => format!("{controls} to cycle · A choose · {cancel} {back}"),
        };
        Some(MenuView {
            title: menu.name.clone(),
            kind: menu.kind,
            items,
            selected: frame.cursor,
            depth: self.stack.len() - 1,
            hint,
        })
    }

    /// Handles controller input; returns what to do, if anything.
    pub fn handle(&mut self, menus: &[Menu], ev: InputEvent, now: Instant) -> Option<MenuOutcome> {
        let menu = self.top(menus)?.clone();
        match ev {
            InputEvent::Button(b, true) => {
                if !self.held.insert(b) {
                    return None;
                }
                self.button_pressed(menus, &menu, b, now)
            }
            InputEvent::Button(b, false) => {
                self.held.remove(&b);
                if self.stepping.is_some() && matches!(b, Button::DpadUp | Button::DpadDown | Button::DpadLeft | Button::DpadRight) {
                    self.stepping = None;
                }
                if self.opener.buttons.contains(&b) {
                    return self.opener_released(menus, &menu);
                }
                None
            }
            InputEvent::Axis(axis, value) => {
                let before = self.axes.insert(axis, value).unwrap_or(0.0);
                self.axis_moved(menus, &menu, axis, before, value, now)
            }
        }
    }

    fn button_pressed(&mut self, menus: &[Menu], menu: &Menu, b: Button, now: Instant) -> Option<MenuOutcome> {
        if b == menu.cancel_button() {
            return self.back();
        }
        match menu.kind {
            MenuKind::Cascade { cluster } => {
                let slot = cluster.slots().iter().position(|s| *s == b)?;
                self.choose(menus, slot)
            }
            MenuKind::Radial { .. } => {
                // A works too, e.g. when the menu was opened by something we can't watch.
                (b == Button::South).then(|| self.choose_cursor(menus)).flatten()
            }
            MenuKind::Buttons => {
                if let Some(i) = menu.items.iter().position(|item| item.button == Some(b)) {
                    return self.choose(menus, i);
                }
                self.list_button(menus, menu, b, now)
            }
            MenuKind::List => self.list_button(menus, menu, b, now),
            MenuKind::Carousel { controls } => {
                let step = match (controls, b) {
                    (CarouselControls::Bumpers, Button::LeftBumper) | (CarouselControls::DPad, Button::DpadLeft) => Some(-1),
                    (CarouselControls::Bumpers, Button::RightBumper) | (CarouselControls::DPad, Button::DpadRight) => Some(1),
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

    fn list_button(&mut self, menus: &[Menu], menu: &Menu, b: Button, now: Instant) -> Option<MenuOutcome> {
        match b {
            Button::DpadUp => self.start_stepping(menu, -1, now),
            Button::DpadDown => self.start_stepping(menu, 1, now),
            Button::South => return self.choose_cursor(menus),
            _ => {}
        }
        None
    }

    fn axis_moved(&mut self, menus: &[Menu], menu: &Menu, axis: Axis, before: f32, value: f32, now: Instant) -> Option<MenuOutcome> {
        // A trigger opener being let go chooses (radial) like a released button.
        if let Some(t) = self.opener.trigger
            && axis == trigger_axis(t)
            && before >= TRIGGER_RELEASE
            && value < TRIGGER_RELEASE
        {
            return self.opener_released(menus, menu);
        }
        match menu.kind {
            MenuKind::Radial { stick } => {
                let (ax, ay) = stick_axes(stick);
                if axis == ax || axis == ay {
                    self.aim(menu, stick);
                }
                None
            }
            MenuKind::List | MenuKind::Buttons => {
                if axis == Axis::LeftY {
                    self.stick_step(menu, value, now);
                }
                None
            }
            MenuKind::Carousel { controls } => {
                match (controls, axis) {
                    (CarouselControls::LeftStick, Axis::LeftX) | (CarouselControls::RightStick, Axis::RightX) => {
                        self.stick_step(menu, value, now)
                    }
                    (CarouselControls::Triggers, Axis::LeftTrigger | Axis::RightTrigger) => {
                        let t = if axis == Axis::LeftTrigger { Trigger::Left } else { Trigger::Right };
                        if value >= TRIGGER_PRESS && self.trigger_step.is_none() {
                            self.trigger_step = Some(t);
                            self.start_stepping(menu, if t == Trigger::Left { -1 } else { 1 }, now);
                        } else if value < TRIGGER_RELEASE && self.trigger_step == Some(t) {
                            self.trigger_step = None;
                            self.stepping = None;
                        }
                    }
                    _ => {}
                }
                None
            }
            MenuKind::Cascade { .. } => None,
        }
    }

    /// A stick used like a two-way D-pad (up/down for lists, left/right for carousels).
    fn stick_step(&mut self, menu: &Menu, value: f32, now: Instant) {
        let threshold = if self.stick_step.is_some() { STICK_RELEASE } else { STICK_PRESS };
        let dir = if value.abs() < threshold { None } else { Some(value.signum() as i32) };
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
        let n = menu.items.len().max(1);
        // Clockwise from straight up; y is positive down.
        let angle = x.atan2(-y).to_degrees().rem_euclid(360.0);
        let slice = 360.0 / n as f32;
        let i = ((angle + slice / 2.0) / slice) as usize % n;
        if let Some(frame) = self.stack.last_mut() {
            frame.cursor = Some(i);
        }
    }

    fn start_stepping(&mut self, menu: &Menu, dir: i32, now: Instant) {
        self.step(menu, dir);
        self.stepping = Some((dir, now + REPEAT_DELAY));
    }

    fn step(&mut self, menu: &Menu, dir: i32) {
        let n = menu.items.len() as i32;
        if let Some(frame) = self.stack.last_mut()
            && n > 0
        {
            let cursor = frame.cursor.unwrap_or(0) as i32;
            frame.cursor = Some((cursor + dir).rem_euclid(n) as usize);
        }
    }

    fn opener_released(&mut self, menus: &[Menu], menu: &Menu) -> Option<MenuOutcome> {
        // Only a radial menu acts on release; the others stay open for navigation.
        if !matches!(menu.kind, MenuKind::Radial { .. }) {
            return None;
        }
        self.choose_cursor(menus).or(Some(MenuOutcome::Close))
    }

    fn choose_cursor(&mut self, menus: &[Menu]) -> Option<MenuOutcome> {
        let i = self.stack.last()?.cursor?;
        self.choose(menus, i)
    }

    /// Chooses item `i`: opens its submenu, or hands back its action to run.
    fn choose(&mut self, menus: &[Menu], i: usize) -> Option<MenuOutcome> {
        let menu = self.top(menus)?;
        let item = menu.items.get(i)?;
        match &item.action {
            ButtonAction::Disabled => None,
            ButtonAction::OpenMenu(name) => {
                let sub = find(menus, name).filter(|m| !menus[*m].items.is_empty())?;
                self.push(menus, sub);
                None
            }
            action => Some(MenuOutcome::Choose { menu: menu.name.clone(), item: i, action: action.clone() }),
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
    use super::*;
    use crate::config::{Cluster, MenuItem};

    fn key(k: &str) -> ButtonAction {
        ButtonAction::Keys(vec![k.into()])
    }

    fn item(label: &str, action: ButtonAction) -> MenuItem {
        MenuItem { label: label.into(), action, button: None }
    }

    fn menu(name: &str, kind: MenuKind, items: Vec<MenuItem>) -> Menu {
        Menu { name: name.into(), kind, items, cancel: None }
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
        Opener { buttons: vec![b], trigger: None }
    }

    #[test]
    fn radial_aims_with_the_stick_and_chooses_on_release() {
        let menus = [menu("Weapons", MenuKind::Radial { stick: Stick::Right }, numbers(4))];
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
        let menus = [menu("W", MenuKind::Radial { stick: Stick::Right }, numbers(8))];
        let mut s = MenuSession::open(&menus, "W", held_by(Button::LeftBumper)).unwrap();
        assert_eq!(s.handle(&menus, InputEvent::Button(Button::LeftBumper, false), Instant::now()), Some(MenuOutcome::Close));

        let opener = Opener { buttons: vec![], trigger: Some(Trigger::Left) };
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
                MenuKind::Cascade { cluster: Cluster::DPad },
                vec![item("Up", key("KEY_U")), item("More", ButtonAction::OpenMenu("Sub".into()))],
            ),
            menu("Sub", MenuKind::Cascade { cluster: Cluster::DPad }, vec![item("Deep", key("KEY_D"))]),
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
        let menus = [menu("F", MenuKind::Cascade { cluster: Cluster::FaceButtons }, numbers(4))];
        let mut s = MenuSession::open(&menus, "F", Opener::default()).unwrap();
        assert_eq!(chose(press(&mut s, &menus, Button::East)).as_deref(), Some("KEY_2"), "East is slot 2 (right)");
        let mut s = MenuSession::open(&menus, "F", Opener::default()).unwrap();
        assert_eq!(press(&mut s, &menus, Button::Select), Some(MenuOutcome::Close));
    }

    #[test]
    fn list_moves_with_dpad_and_stick_and_wraps() {
        let menus = [menu("Pause", MenuKind::List, numbers(3))];
        let mut s = MenuSession::open(&menus, "Pause", held_by(Button::Select)).unwrap();
        // Letting go of the button that opened a list leaves it open.
        assert_eq!(s.handle(&menus, InputEvent::Button(Button::Select, false), Instant::now()), None);
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
    fn missing_or_empty_menus_do_not_open() {
        let menus = [menu("Empty", MenuKind::List, vec![])];
        assert!(MenuSession::open(&menus, "Empty", Opener::default()).is_none());
        assert!(MenuSession::open(&menus, "Nope", Opener::default()).is_none());
    }
}

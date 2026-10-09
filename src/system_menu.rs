//! The menu Guide + Start opens over the game. It isn't in any profile: Quick Settings (a few
//! values of the active profile) comes first, and Edit Controls last. The daemon routes
//! controller input here while it is open, and the overlay draws its [`MenuView`].

use std::collections::HashSet;

use crate::{
    layout_editor::{EditStep, LayoutEditor},
    config::{Button, Config, GyroMode, OverlayStyle, Profile, ProfileRef, Stick, StickAction},
    input::{Axis, InputEvent},
    menu::{ItemView, MenuView, Tone},
};

/// Stick deflection that moves the cursor or changes a value, and the point it lets go below.
const STICK_STEP: f32 = 0.6;
const STICK_RELEASE: f32 = 0.45;
/// One Quick Settings step multiplies or divides a value by this.
const STEP: f32 = 1.1;
/// Stick speed in pixels per second, and gyro sensitivity in pixels per degree.
const SPEED_RANGE: (f32, f32) = (10.0, 10_000.0);
const SENSITIVITY_RANGE: (f32, f32) = (0.1, 1_000.0);

/// What the menu asks the daemon to do after a choice.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Put the overlay away.
    Close,
    /// A value changed in the config: save it and apply it.
    Changed,
    /// Switch to this profile (the menu stays up).
    Switch(ProfileRef),
    /// Edit Controls was chosen: the daemon makes a pack for the game if there isn't one.
    EditControls,
    /// Save the changes to the config file, then close.
    Save,
    /// Put the config back as it was when the menu opened, then close.
    Discard(Box<Config>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Screen {
    Main,
    Quick,
    Editor(LayoutEditor),
    /// Closing with changes not saved: save, discard, or keep editing.
    Confirm,
}

/// One line of Quick Settings. Which ones show depends on the active profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    /// Speed of a stick that moves the mouse.
    StickSpeed(Stick),
    GyroSensitivity,
    /// Flips every Y inversion in the profile at once.
    InvertY,
    /// Next or previous profile of the active game.
    Profile,
    /// Button labels swapped to the Nintendo layout, for the active game.
    NintendoLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Up,
    Down,
    Left,
    Right,
    Pick,
    Back,
}

/// The open menu: which screen, and where the cursor is on it.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemMenu {
    screen: Screen,
    cursor: usize,
    /// Stick pushes already counted, so holding a stick moves one step, not a run of them.
    latch_x: bool,
    latch_y: bool,
    /// The config as it was when the menu opened: what Discard goes back to, and what counts as
    /// unsaved.
    saved: Box<Config>,
}

impl SystemMenu {
    /// A menu over a config that is currently saved as `saved`.
    pub fn new(saved: &Config) -> Self {
        SystemMenu { screen: Screen::Main, cursor: 0, latch_x: false, latch_y: false, saved: Box::new(saved.clone()) }
    }

    /// Handles one controller event. `None` when nothing needs the daemon.
    pub fn handle(&mut self, ev: InputEvent, config: &mut Config) -> Option<Outcome> {
        let step = match ev {
            InputEvent::Button(b, true) => match b {
                Button::DpadUp => Step::Up,
                Button::DpadDown => Step::Down,
                Button::DpadLeft => Step::Left,
                Button::DpadRight => Step::Right,
                Button::South => Step::Pick,
                Button::East => Step::Back,
                _ => return None,
            },
            InputEvent::Axis(Axis::LeftY, v) => Self::stick_step(&mut self.latch_y, v, Step::Down, Step::Up)?,
            InputEvent::Axis(Axis::LeftX, v) => Self::stick_step(&mut self.latch_x, v, Step::Right, Step::Left)?,
            _ => return None,
        };
        match self.screen {
            Screen::Main => self.main_step(step, config),
            Screen::Confirm => self.confirm_step(step),
            Screen::Quick => self.quick_step(step, config),
            Screen::Editor(_) => self.editor_step(step, config),
        }
    }

    /// The step a stick push makes, once per push. `positive` is the step for a push past the
    /// stick's positive side.
    fn stick_step(latch: &mut bool, v: f32, positive: Step, negative: Step) -> Option<Step> {
        if v.abs() < STICK_RELEASE {
            *latch = false;
            return None;
        }
        if *latch || v.abs() < STICK_STEP {
            return None;
        }
        *latch = true;
        Some(if v > 0.0 { positive } else { negative })
    }

    fn main_step(&mut self, step: Step, config: &Config) -> Option<Outcome> {
        match step {
            Step::Up | Step::Down => {
                self.cursor = wrap(self.cursor, step, 2);
                None
            }
            Step::Pick => match self.cursor {
                0 => {
                    self.screen = Screen::Quick;
                    self.cursor = 0;
                    None
                }
                _ => {
                    self.screen = Screen::Editor(LayoutEditor::new());
                    Some(Outcome::EditControls)
                }
            },
            Step::Back if *self.saved == *config => Some(Outcome::Close),
            Step::Back => {
                self.screen = Screen::Confirm;
                self.cursor = 0;
                None
            }
            Step::Left | Step::Right => None,
        }
    }

    fn confirm_step(&mut self, step: Step) -> Option<Outcome> {
        match step {
            Step::Up | Step::Down => {
                self.cursor = wrap(self.cursor, step, 3);
                None
            }
            Step::Pick => match self.cursor {
                0 => Some(Outcome::Save),
                1 => Some(Outcome::Discard(self.saved.clone())),
                _ => {
                    self.back_to_main();
                    None
                }
            },
            Step::Back => {
                self.back_to_main();
                None
            }
            Step::Left | Step::Right => None,
        }
    }

    /// Back to the main page, from wherever the menu is.
    pub fn back_to_main(&mut self) {
        self.screen = Screen::Main;
        self.cursor = 0;
    }

    fn quick_step(&mut self, step: Step, config: &mut Config) -> Option<Outcome> {
        let rows = quick_rows(config);
        match step {
            Step::Up | Step::Down => {
                self.cursor = wrap(self.cursor, step, rows.len());
                None
            }
            Step::Back => {
                self.screen = Screen::Main;
                self.cursor = 0;
                None
            }
            Step::Left | Step::Right | Step::Pick => {
                let row = *rows.get(self.cursor)?;
                let dir = match step {
                    Step::Left => -1,
                    _ => 1,
                };
                adjust(row, dir, config)
            }
        }
    }

    fn editor_step(&mut self, step: Step, config: &mut Config) -> Option<Outcome> {
        let Screen::Editor(editor) = &mut self.screen else { return None };
        match step {
            Step::Up => editor.move_cursor((0, -1), config),
            Step::Down => editor.move_cursor((0, 1), config),
            Step::Pick => return (editor.choose(config) == EditStep::Changed).then_some(Outcome::Changed),
            Step::Back => {
                if editor.back() == EditStep::Leave {
                    self.screen = Screen::Main;
                    self.cursor = 1;
                }
            }
            Step::Left | Step::Right if editor.two_columns(config) => {
                editor.move_cursor((if step == Step::Left { -1 } else { 1 }, 0), config);
            }
            Step::Left => return (editor.adjust(-1, config) == EditStep::Changed).then_some(Outcome::Changed),
            Step::Right => return (editor.adjust(1, config) == EditStep::Changed).then_some(Outcome::Changed),
        }
        None
    }

    /// What the overlay draws now.
    pub fn view(&self, config: &Config) -> MenuView {
        let style = config.active_menu_style().clone();
        match &self.screen {
            Screen::Main => main_page(style, self.cursor),
            Screen::Editor(editor) => editor.view(config, &["Menu".to_string()]),
            Screen::Confirm => MenuView {
                title: "Unsaved changes".into(),
                kind: crate::config::MenuKind::List,
                items: vec![
                    ItemView { label: "Save and close".into(), button: None, submenu: false, buttons: Vec::new(), tone: Tone::Normal, keyword: None },
                    ItemView { label: "Discard and close".into(), button: None, submenu: false, buttons: Vec::new(), tone: Tone::Normal, keyword: None },
                    ItemView { label: "Keep editing".into(), button: None, submenu: false, buttons: Vec::new(), tone: Tone::Normal, keyword: None },
                ],
                selected: Some(self.cursor),
                crumbs: vec!["Menu".into()],
                hint: "A choose · B keep editing".into(),
                style,
            },
            Screen::Quick => MenuView {
                title: "Quick Settings".into(),
                kind: crate::config::MenuKind::List,
                items: quick_rows(config).into_iter().map(|row| ItemView { label: label(row, config), button: None, submenu: false, buttons: Vec::new(), tone: Tone::Normal, keyword: None }).collect(),
                selected: Some(self.cursor),
                crumbs: vec!["Menu".into()],
                hint: "◀ ▶ change · A toggle or next · B back".into(),
                style,
            },
        }
    }
}

/// True when these events press Guide and Start together: one of them goes down while the other
/// is already held. `held` is what was down before the events.
pub fn chord(held: &HashSet<Button>, events: &[InputEvent]) -> bool {
    let mut held = held.clone();
    for ev in events {
        let InputEvent::Button(b, down) = *ev else { continue };
        if down {
            let other = match b {
                Button::Start => Button::Guide,
                Button::Guide => Button::Start,
                _ => {
                    held.insert(b);
                    continue;
                }
            };
            if held.contains(&other) {
                return true;
            }
            held.insert(b);
        } else {
            held.remove(&b);
        }
    }
    false
}

/// The next index `step` moves to in a list of `len`, wrapping at both ends.
fn wrap(cursor: usize, step: Step, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    match step {
        Step::Up => (cursor + len - 1) % len,
        _ => (cursor + 1) % len,
    }
}

/// The rows Quick Settings shows for the active profile.
fn quick_rows(config: &Config) -> Vec<Row> {
    let mut rows = Vec::new();
    if let Some(profile) = config.active() {
        for (stick, cfg) in [(Stick::Left, &profile.left_stick), (Stick::Right, &profile.right_stick)] {
            if matches!(cfg.action, StickAction::Mouse { .. }) {
                rows.push(Row::StickSpeed(stick));
            }
        }
        if matches!(profile.gyro.mode, GyroMode::Mouse { .. }) {
            rows.push(Row::GyroSensitivity);
        }
        rows.push(Row::InvertY);
    }
    rows.push(Row::NintendoLayout);
    if config.active_game().profiles.len() > 1 {
        rows.push(Row::Profile);
    }
    rows
}

fn label(row: Row, config: &Config) -> String {
    let profile = config.active();
    match row {
        Row::StickSpeed(stick) => {
            let name = if stick == Stick::Left { "Left" } else { "Right" };
            let speed = profile.and_then(|p| match &stick_config(p, stick).action {
                StickAction::Mouse { speed, .. } => Some(*speed),
                _ => None,
            });
            format!("{name} stick speed: {:.0} px/s", speed.unwrap_or_default())
        }
        Row::GyroSensitivity => {
            let sensitivity = profile.and_then(|p| match p.gyro.mode {
                GyroMode::Mouse { sensitivity } => Some(sensitivity),
                _ => None,
            });
            format!("Gyro sensitivity: {:.1} px/°", sensitivity.unwrap_or_default())
        }
        Row::InvertY => {
            let on = profile.is_some_and(inverted_y);
            format!("Invert Y: {}", if on { "on" } else { "off" })
        }
        Row::Profile => format!("Profile: {}", config.active_ref().profile),
        Row::NintendoLayout => format!("Nintendo button layout: {}", if config.active_nintendo_layout() { "on" } else { "off" }),
    }
}

fn stick_config(profile: &Profile, stick: Stick) -> &crate::config::StickConfig {
    if stick == Stick::Left { &profile.left_stick } else { &profile.right_stick }
}

/// Whether any of the profile's Y inversions is on.
fn inverted_y(profile: &Profile) -> bool {
    [&profile.left_stick, &profile.right_stick].into_iter().any(|s| match s.action {
        StickAction::Gamepad { invert_y, .. } | StickAction::Mouse { invert_y, .. } | StickAction::Scroll { invert_y, .. } => invert_y,
        _ => false,
    }) || profile.gyro.invert_y
}

/// Every Y inversion in the profile, so one switch can set them all.
fn y_flags(profile: &mut Profile) -> Vec<&mut bool> {
    let mut flags = Vec::new();
    for stick in [&mut profile.left_stick, &mut profile.right_stick] {
        match &mut stick.action {
            StickAction::Gamepad { invert_y, .. } | StickAction::Mouse { invert_y, .. } | StickAction::Scroll { invert_y, .. } => {
                flags.push(invert_y);
            }
            _ => {}
        }
    }
    flags.push(&mut profile.gyro.invert_y);
    flags
}

/// Changes a row's value in the config by one step (`dir` is -1 or 1), or moves to the next
/// profile.
fn adjust(row: Row, dir: i32, config: &mut Config) -> Option<Outcome> {
    if let Row::Profile = row {
        return next_profile(dir, config).map(Outcome::Switch);
    }
    if let Row::NintendoLayout = row {
        // A game's own choice, so it's set on the game the menu is over, not on the profile.
        let on = !config.active_nintendo_layout();
        let at = config.active_ref();
        config.game_mut(at.game.as_deref())?.nintendo_layout = Some(on);
        return Some(Outcome::Changed);
    }
    let profile = active_profile_mut(config)?;
    match row {
        Row::StickSpeed(stick) => {
            let cfg = if stick == Stick::Left { &mut profile.left_stick } else { &mut profile.right_stick };
            let StickAction::Mouse { speed, .. } = &mut cfg.action else { return None };
            *speed = scale(*speed, dir, SPEED_RANGE);
        }
        Row::GyroSensitivity => {
            let GyroMode::Mouse { sensitivity } = &mut profile.gyro.mode else { return None };
            *sensitivity = scale(*sensitivity, dir, SENSITIVITY_RANGE);
        }
        Row::InvertY => {
            let flags = y_flags(profile);
            let on = flags.iter().any(|f| **f);
            for flag in flags {
                *flag = !on;
            }
        }
        Row::Profile | Row::NintendoLayout => {}
    }
    Some(Outcome::Changed)
}

fn scale(value: f32, dir: i32, (low, high): (f32, f32)) -> f32 {
    let factor = if dir < 0 { 1.0 / STEP } else { STEP };
    (value * factor).clamp(low, high)
}

/// The profile after (or before) the active one in its game, wrapping around.
fn next_profile(dir: i32, config: &Config) -> Option<ProfileRef> {
    let at = config.active_ref();
    let profiles = &config.active_game().profiles;
    let i = profiles.iter().position(|p| p.name == at.profile)?;
    let next = (i as i32 + dir).rem_euclid(profiles.len() as i32) as usize;
    Some(ProfileRef::new(at.game.as_deref(), &profiles[next].name))
}

/// The profile the menu edits: the active one, in its game (or General).
pub(crate) fn active_profile_mut(config: &mut Config) -> Option<&mut Profile> {
    let at = config.active_ref();
    config.game_mut(at.game.as_deref())?.profiles.iter_mut().find(|p| p.name == at.profile)
}

/// The menu's first page, drawn with `style`; the settings previews show it too.
pub fn main_page(style: OverlayStyle, cursor: usize) -> MenuView {
    MenuView {
        title: "Menu".into(),
        kind: crate::config::MenuKind::List,
        items: vec![
            ItemView { label: "Quick Settings".into(), button: None, submenu: true, buttons: Vec::new(), tone: Tone::Normal, keyword: None },
            ItemView { label: "Edit Controls".into(), button: None, submenu: false, buttons: Vec::new(), tone: Tone::Normal, keyword: None },
        ],
        selected: Some(cursor),
        crumbs: Vec::new(),
        hint: "A choose · B close".into(),
        style,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MouseResponse;

    fn press(b: Button) -> InputEvent {
        InputEvent::Button(b, true)
    }

    /// A config whose active profile has a mouse-moving left stick.
    fn mouse_config() -> Config {
        let mut config = Config::default();
        let profile = &mut config.general.profiles[0];
        profile.left_stick.action = StickAction::Mouse { speed: 1000.0, response: MouseResponse::default(), invert_y: false };
        config
    }

    fn speed(config: &Config) -> f32 {
        match config.active().unwrap().left_stick.action {
            StickAction::Mouse { speed, .. } => speed,
            _ => panic!("left stick is not a mouse"),
        }
    }

    #[test]
    fn chord_needs_both_buttons_together() {
        let none = HashSet::new();
        assert!(chord(&none, &[press(Button::Guide), press(Button::Start)]));
        assert!(chord(&HashSet::from([Button::Guide]), &[press(Button::Start)]));
        assert!(chord(&HashSet::from([Button::Start]), &[press(Button::Guide)]));
        assert!(!chord(&none, &[press(Button::Start)]));
        // Guide let go before Start goes down is not a chord.
        assert!(!chord(&none, &[press(Button::Guide), InputEvent::Button(Button::Guide, false), press(Button::Start)]));
        assert!(!chord(&HashSet::from([Button::Guide]), &[press(Button::South)]));
    }

    #[test]
    fn main_menu_opens_quick_settings_first_and_edit_controls_last() {
        let mut config = mouse_config();
        let mut menu = SystemMenu::new(&config);
        assert_eq!(menu.handle(press(Button::South), &mut config), None);
        assert_eq!(menu.screen, Screen::Quick);
        assert_eq!(menu.handle(press(Button::East), &mut config), None);
        assert_eq!(menu.screen, Screen::Main);
        assert_eq!(menu.handle(press(Button::DpadDown), &mut config), None);
        assert_eq!(menu.handle(press(Button::South), &mut config), Some(Outcome::EditControls));
        // B leaves the editor for the main page, and B again closes the menu.
        assert_eq!(menu.handle(press(Button::East), &mut config), None);
        assert_eq!(menu.handle(press(Button::East), &mut config), Some(Outcome::Close));
    }

    #[test]
    fn quick_settings_only_lists_what_the_profile_uses() {
        let mut config = mouse_config();
        config.general.profiles.truncate(1);
        assert_eq!(quick_rows(&config), [Row::StickSpeed(Stick::Left), Row::InvertY, Row::NintendoLayout]);
        // A second profile adds the profile switch.
        config.general.profiles.push(config.general.profiles[0].clone());
        config.general.profiles[1].name = "Other".into();
        assert_eq!(quick_rows(&config), [Row::StickSpeed(Stick::Left), Row::InvertY, Row::NintendoLayout, Row::Profile]);
    }

    #[test]
    fn quick_nintendo_layout_sets_the_active_game_only() {
        let mut config = mouse_config();
        assert!(!config.active_nintendo_layout());
        // Picking the row turns it on for the game the menu is over, and leaves Settings alone.
        assert_eq!(adjust(Row::NintendoLayout, 1, &mut config), Some(Outcome::Changed));
        assert!(config.active_nintendo_layout());
        assert_eq!(config.general.nintendo_layout, Some(true));
        assert!(!config.nintendo_layout);
        assert_eq!(label(Row::NintendoLayout, &config), "Nintendo button layout: on");
        assert_eq!(adjust(Row::NintendoLayout, 1, &mut config), Some(Outcome::Changed));
        assert_eq!(config.general.nintendo_layout, Some(false));
    }

    #[test]
    fn closing_with_unsaved_changes_asks_first() {
        let mut config = mouse_config();
        let saved = config.clone();
        let mut menu = SystemMenu::new(&saved);
        // Change the stick speed: now the config differs from what was saved.
        menu.handle(press(Button::South), &mut config);
        menu.handle(press(Button::DpadRight), &mut config);
        menu.handle(press(Button::East), &mut config);
        assert_eq!(menu.screen, Screen::Main);
        assert_eq!(menu.handle(press(Button::East), &mut config), None);
        assert_eq!(menu.screen, Screen::Confirm);
        // Keep editing goes back to the main page.
        menu.handle(press(Button::DpadDown), &mut config);
        menu.handle(press(Button::DpadDown), &mut config);
        assert_eq!(menu.handle(press(Button::South), &mut config), None);
        assert_eq!(menu.screen, Screen::Main);
        // Closing again asks again; discard hands back what was saved.
        menu.handle(press(Button::East), &mut config);
        menu.handle(press(Button::DpadDown), &mut config);
        assert_eq!(menu.handle(press(Button::South), &mut config), Some(Outcome::Discard(Box::new(saved.clone()))));
    }

    #[test]
    fn stick_speed_steps_up_and_down() {
        let mut config = mouse_config();
        let mut menu = SystemMenu::new(&config);
        menu.handle(press(Button::South), &mut config);
        assert_eq!(menu.handle(press(Button::DpadRight), &mut config), Some(Outcome::Changed));
        assert!((speed(&config) - 1100.0).abs() < 0.01);
        assert_eq!(menu.handle(press(Button::DpadLeft), &mut config), Some(Outcome::Changed));
        assert!((speed(&config) - 1000.0).abs() < 0.01);
    }

    #[test]
    fn invert_y_flips_every_flag_at_once() {
        let mut config = mouse_config();
        let mut menu = SystemMenu::new(&config);
        menu.handle(press(Button::South), &mut config);
        menu.handle(press(Button::DpadDown), &mut config);
        assert_eq!(menu.handle(press(Button::South), &mut config), Some(Outcome::Changed));
        let profile = config.active().unwrap();
        assert!(inverted_y(profile));
        assert!(matches!(profile.left_stick.action, StickAction::Mouse { invert_y: true, .. }));
        assert!(profile.gyro.invert_y);
        menu.handle(press(Button::South), &mut config);
        assert!(!inverted_y(config.active().unwrap()));
    }

    #[test]
    fn stick_push_steps_once_until_let_go() {
        let mut config = mouse_config();
        let mut menu = SystemMenu::new(&config);
        menu.handle(InputEvent::Axis(Axis::LeftY, 0.9), &mut config);
        assert_eq!(menu.cursor, 1);
        menu.handle(InputEvent::Axis(Axis::LeftY, 1.0), &mut config);
        assert_eq!(menu.cursor, 1);
        menu.handle(InputEvent::Axis(Axis::LeftY, 0.0), &mut config);
        menu.handle(InputEvent::Axis(Axis::LeftY, -0.9), &mut config);
        assert_eq!(menu.cursor, 0);
    }
}

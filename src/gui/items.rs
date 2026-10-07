//! Macros, menus and info overlays: their cards, editors and the messages that edit them.

use iced::widget::{column, row};

use super::*;

/// A token in an info cell's "Insert…" list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TokenChoice(pub(super) &'static str, pub(super) &'static str);

impl fmt::Display for TokenChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{}}}  {}", self.0, self.1)
    }
}

/// A menu item's quick-select button in a picker; `None` means none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct QuickChoice(pub(super) Option<Button>);

impl fmt::Display for QuickChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(b) => f.write_str(crate::menu::button_badge(b)),
            None => f.write_str("—"),
        }
    }
}

/// The "add an info overlay" card.
pub(super) fn view_new_info_card<'a>() -> Element<'a, Message> {
    container(
        row![
            button(text("+ New info overlay")).style(button::secondary).on_press(Message::NewInfo),
            text("Text and button glyphs on screen, e.g. a game's controls.").size(13).color(MUTED_COLOR),
            space::horizontal(),
            help(
                "An info overlay shows a grid of text on screen without taking the controller: a \
                 game's controls with glyphs that match the controller in use, the time, CPU load and \
                 more. Show it always while its game is active, or with the \"Show info \
                 overlay…\" action."
                    .into(),
            ),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(14)
    .width(Length::Fill)
    .style(style::card)
    .into()
}

/// The "add a menu" card: one button per kind, kept apart from the menus themselves.
pub(super) fn view_new_menu_card<'a>() -> Element<'a, Message> {
    let mut kinds = row![text("Add a menu:")].spacing(8).align_y(Alignment::Center);
    for kind in MenuKindTag::ALL {
        kinds = kinds.push(button(text(kind.short()).size(14)).style(button::secondary).on_press(Message::NewMenu(kind)));
    }
    container(row![kinds, space::horizontal(), help(MENUS_HELP.into())].align_y(Alignment::Center))
        .padding(14)
        .width(Length::Fill)
        .style(style::card)
        .into()
}

/// A directional menu always has exactly four slots (up, right, down, left), some maybe empty.
/// A grid holds at most `GRID_MAX` × `GRID_MAX` items, and gets enough columns that its rows
/// stay within `GRID_MAX`.
pub(super) fn fit_items(menu: &mut Menu) {
    match menu.kind {
        MenuKind::Directional { .. } => {
            menu.items.truncate(4);
            while menu.items.len() < 4 {
                menu.items.push(MenuItem { label: String::new(), action: ButtonAction::Disabled, button: None });
            }
        }
        MenuKind::Grid { columns } => {
            menu.items.truncate(GRID_MAX * GRID_MAX);
            let fewest = menu.items.len().div_ceil(GRID_MAX).max(1);
            menu.kind = MenuKind::Grid { columns: columns.clamp(fewest as u8, GRID_MAX as u8) };
        }
        _ => {}
    }
}

/// How many items a menu can take, if it has a limit.
fn item_limit(kind: MenuKind) -> Option<usize> {
    match kind {
        MenuKind::Directional { .. } => Some(4),
        _ => kind.grid_columns().map(|columns| columns * GRID_MAX),
    }
}

/// A menu's kind picker and that kind's settings.
fn menu_kind_row<'a>(mi: usize, menu: &Menu) -> Element<'a, Message> {
    let mut kind_row = row![dropdown(MenuKindTag::ALL, Some(menu.kind.tag()), move |t| Message::SetMenuKind(mi, MenuKind::default_for(t))).width(280)]
        .spacing(8)
        .align_y(Alignment::Center);
    match menu.kind {
        MenuKind::Radial { stick } => {
            kind_row = kind_row.push(text("aim with")).push(
                dropdown([Stick::Left, Stick::Right], Some(stick), move |s| Message::SetMenuKind(mi, MenuKind::Radial { stick: s })).width(150),
            );
        }
        MenuKind::Directional { cluster } => {
            kind_row = kind_row.push(text("on the")).push(
                dropdown([Cluster::DPad, Cluster::FaceButtons], Some(cluster), move |c| {
                    Message::SetMenuKind(mi, MenuKind::Directional { cluster: c })
                })
                .width(150),
            );
        }
        MenuKind::Carousel { controls } => {
            kind_row = kind_row.push(text("cycle with")).push(
                dropdown(CarouselControls::ALL, Some(controls), move |c| Message::SetMenuKind(mi, MenuKind::Carousel { controls: c }))
                    .width(200),
            );
        }
        MenuKind::Grid { columns } => {
            // Too few columns would need more than `GRID_MAX` rows.
            let fewest = menu.items.len().div_ceil(GRID_MAX).max(1) as u8;
            let options: Vec<u8> = (fewest..=GRID_MAX as u8).collect();
            kind_row = kind_row
                .push(dropdown(options, Some(columns), move |c| Message::SetMenuKind(mi, MenuKind::Grid { columns: c })).width(70))
                .push(text("columns"));
        }
        MenuKind::List | MenuKind::Buttons => {}
    }
kind_row.into()
}

/// "+ Add item", greyed out with a note once a grid is full.
fn add_menu_item_row<'a>(mi: usize, menu: &Menu) -> Element<'a, Message> {
    let full = item_limit(menu.kind).is_some_and(|limit| menu.items.len() >= limit);
    let mut add = row![button(text("+ Add item").size(13)).style(button::secondary).on_press_maybe((!full).then_some(Message::AddMenuItem(mi)))]
        .spacing(10)
        .align_y(Alignment::Center);
    if full {
        let more = if menu.kind.grid_columns() < Some(GRID_MAX) { "; add columns to fit more" } else { "" };
        add = add.push(text(format!("A grid holds up to {GRID_MAX} rows{more}.")).size(12).color(MUTED_COLOR));
    }
    add.into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StepKind {
    Tap,
    Press,
    Release,
    Wait,
    Stick,
}

impl fmt::Display for StepKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StepKind::Tap => "Tap",
            StepKind::Press => "Hold down",
            StepKind::Release => "Release",
            StepKind::Wait => "Wait",
            StepKind::Stick => "Move stick",
        })
    }
}

pub(super) const STEP_KINDS: [StepKind; 5] = [StepKind::Tap, StepKind::Press, StepKind::Release, StepKind::Wait, StepKind::Stick];

/// One frame at 60 fps: the usual gap between motion inputs in fighting games.
pub(super) const MOTION_FRAME_MS: u64 = 17;

pub(super) const DIAGONAL: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Stick positions offered for "Move stick" steps (y positive is down).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StickPreset {
    Center,
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
    Custom,
}

impl StickPreset {
    pub(super) const ALL: [StickPreset; 10] = [
        StickPreset::Center,
        StickPreset::Up,
        StickPreset::UpRight,
        StickPreset::Right,
        StickPreset::DownRight,
        StickPreset::Down,
        StickPreset::DownLeft,
        StickPreset::Left,
        StickPreset::UpLeft,
        StickPreset::Custom,
    ];

    pub(super) fn position(self) -> Option<(f32, f32)> {
        let d = DIAGONAL;
        Some(match self {
            StickPreset::Center => (0.0, 0.0),
            StickPreset::Up => (0.0, -1.0),
            StickPreset::UpRight => (d, -d),
            StickPreset::Right => (1.0, 0.0),
            StickPreset::DownRight => (d, d),
            StickPreset::Down => (0.0, 1.0),
            StickPreset::DownLeft => (-d, d),
            StickPreset::Left => (-1.0, 0.0),
            StickPreset::UpLeft => (-d, -d),
            StickPreset::Custom => return None,
        })
    }

    pub(super) fn of(x: f32, y: f32) -> Self {
        Self::ALL
            .into_iter()
            .find(|p| p.position().is_some_and(|(px, py)| (px - x).abs() < 0.01 && (py - y).abs() < 0.01))
            .unwrap_or(StickPreset::Custom)
    }
}

impl fmt::Display for StickPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StickPreset::Center => "● Center",
            StickPreset::Up => "↑ Up",
            StickPreset::UpRight => "↗ Up-right",
            StickPreset::Right => "→ Right",
            StickPreset::DownRight => "↘ Down-right",
            StickPreset::Down => "↓ Down",
            StickPreset::DownLeft => "↙ Down-left",
            StickPreset::Left => "← Left",
            StickPreset::UpLeft => "↖ Up-left",
            StickPreset::Custom => "Custom",
        })
    }
}

/// Fighting-game motions, written for a character facing right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Motion {
    QuarterCircleForward,
    QuarterCircleBack,
    DragonPunch,
    HalfCircleForward,
    HalfCircleBack,
}

impl Motion {
    pub(super) const ALL: [Motion; 5] = [
        Motion::QuarterCircleForward,
        Motion::QuarterCircleBack,
        Motion::DragonPunch,
        Motion::HalfCircleForward,
        Motion::HalfCircleBack,
    ];

    pub(super) fn directions(self) -> &'static [StickPreset] {
        use StickPreset::*;
        match self {
            Motion::QuarterCircleForward => &[Down, DownRight, Right],
            Motion::QuarterCircleBack => &[Down, DownLeft, Left],
            Motion::DragonPunch => &[Right, Down, DownRight],
            Motion::HalfCircleForward => &[Left, DownLeft, Down, DownRight, Right],
            Motion::HalfCircleBack => &[Right, DownRight, Down, DownLeft, Left],
        }
    }

    /// Steps moving the left stick through the motion, one frame per direction.
    pub(super) fn steps(self) -> Vec<MacroStep> {
        self.directions()
            .iter()
            .filter_map(|p| p.position())
            .flat_map(|(x, y)| [MacroStep::Stick { stick: Stick::Left, x, y }, MacroStep::Wait(MOTION_FRAME_MS)])
            .collect()
    }
}

impl fmt::Display for Motion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Motion::QuarterCircleForward => "Quarter circle forward (↓↘→)",
            Motion::QuarterCircleBack => "Quarter circle back (↓↙←)",
            Motion::DragonPunch => "Dragon punch (→↓↘)",
            Motion::HalfCircleForward => "Half circle forward (←↙↓↘→)",
            Motion::HalfCircleBack => "Half circle back (→↘↓↙←)",
        })
    }
}

pub(super) const DEFAULT_TAP_MS: u64 = 50;

/// Where "When the game starts, for" and "stays for" start, in seconds.
pub(super) const DEFAULT_START_SECONDS: f32 = 5.0;

pub(super) const DEFAULT_LINGER_SECONDS: f32 = 3.0;

pub(super) const DEFAULT_WAIT_MS: u64 = 100;

pub(super) fn step_kind(step: &MacroStep) -> StepKind {
    match step {
        MacroStep::Tap { .. } => StepKind::Tap,
        MacroStep::Press(_) => StepKind::Press,
        MacroStep::Release(_) => StepKind::Release,
        MacroStep::Wait(_) => StepKind::Wait,
        MacroStep::Stick { .. } => StepKind::Stick,
    }
}

/// A step of another kind, keeping the action (or timing) where it carries over.
pub(super) fn convert_step(step: &MacroStep, kind: StepKind) -> MacroStep {
    let action = step.action().cloned().unwrap_or(ButtonAction::Keys(Vec::new()));
    match kind {
        StepKind::Tap => MacroStep::Tap { action, hold_ms: DEFAULT_TAP_MS },
        StepKind::Press => MacroStep::Press(action),
        StepKind::Release => MacroStep::Release(action),
        StepKind::Wait => MacroStep::Wait(DEFAULT_WAIT_MS),
        StepKind::Stick => MacroStep::Stick { stick: Stick::Left, x: 1.0, y: 0.0 },
    }
}

/// Number of "Hold down" steps with no later matching "Release".
pub(super) fn unreleased_holds(m: &Macro) -> usize {
    let mut held: Vec<&ButtonAction> = Vec::new();
    for step in &m.steps {
        match step {
            MacroStep::Press(a) => held.push(a),
            MacroStep::Release(a) => {
                if let Some(i) = held.iter().position(|h| *h == a) {
                    held.remove(i);
                }
            }
            _ => {}
        }
    }
    held.len()
}

pub(super) const MENUS_HELP: &str = "On-screen menus you open with the \"Open menu…\" action from any button, \
    gesture, combo or trigger. A menu is up while that input is held and closes when you let go; wrap the \
    action in \"Toggle\" to keep it up until pressed again. Radial: aim a stick, let go to choose. \
    Directional: four slots on the D-pad or face buttons. List: move with the D-pad or left stick, A \
    chooses. Button menu: a list where items also have their own button. Carousel: cycle with the chosen \
    controls, A chooses. Grid: a list laid out in up to 6 columns and 6 rows, moved through in all four \
    directions. Items tap their action like a button press; an item can open another menu of the \
    same kind (not from radial menus), which closes along with it.";

impl App {
    #[expect(clippy::too_many_lines, clippy::cognitive_complexity, reason = "predates the size lints")]
    pub(super) fn update_items(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SetAction(Target::MenuItem(m, i), action) => {
                if let Some(item) = self.menus_mut().get_mut(m).and_then(|m| m.items.get_mut(i)) {
                    item.action = action;
                }
            }
            Message::SetAction(Target::MacroStep(m, s), action) => {
                if let Some(slot) = self.macros_mut().get_mut(m).and_then(|m| m.steps.get_mut(s)).and_then(|s| s.action_mut()) {
                    *slot = action;
                }
            }
            Message::NewInfo => {
                let name = self.new_item_name(ItemKind::Info, "Info");
                let cells = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
                let overlay = InfoOverlay {
                    name,
                    always: true, on_start: None, linger: None,
                    style: OverlayStyle::info(),
                    rows: vec![cells("{south}", "Jump"), cells("{west}", "Reload")],
                };
                // New ones go first, right under the button that made them, already open.
                self.infos_mut().insert(0, overlay);
                self.open_infos = self.open_infos.iter().map(|j| j + 1).collect();
                self.open_infos.insert(0);
                self.open_info_appearance = self.open_info_appearance.iter().map(|j| j + 1).collect();
            }
            Message::ToggleInfo(i) => {
                if !self.open_infos.remove(&i) {
                    self.open_infos.insert(i);
                }
            }
            Message::ToggleInfoAppearance(i) => {
                if !self.open_info_appearance.remove(&i) {
                    self.open_info_appearance.insert(i);
                }
            }
            Message::DeleteInfo(i) => {
                if i < self.infos().len() {
                    self.infos_mut().remove(i);
                    self.open_infos = shift_removed(&self.open_infos, i);
                    self.open_info_appearance = shift_removed(&self.open_info_appearance, i);
                }
            }
            Message::RenameInfo(i, name) => {
                if self.item_name_free(ItemKind::Info, Some(i), &name)
                    && let Some(o) = self.infos_mut().get_mut(i)
                {
                    let old = std::mem::replace(&mut o.name, name.clone());
                    // Keep every "Show info overlay" (in profiles and menus) pointing at it.
                    self.follow_item_rename(ItemKind::Info, &old, &name);
                }
            }
            Message::SetInfoOnStart(i, on) => {
                if let Some(o) = self.infos_mut().get_mut(i) {
                    o.on_start = on;
                }
            }
            Message::SetInfoLinger(i, seconds) => {
                if let Some(o) = self.infos_mut().get_mut(i) {
                    o.linger = seconds;
                }
            }
            Message::SetInfoAlways(i, always) => {
                if let Some(o) = self.infos_mut().get_mut(i) {
                    o.always = always;
                }
            }
            Message::SetInfoStyle(i, style) => {
                if let Some(o) = self.infos_mut().get_mut(i) {
                    o.style = style;
                }
            }
            Message::AddInfoRow(i) => {
                if let Some(o) = self.infos_mut().get_mut(i) {
                    o.rows.push(vec![String::new()]);
                }
            }
            Message::MoveInfoRow(i, r, up) => {
                if let Some(o) = self.infos_mut().get_mut(i) {
                    let j = if up { r.checked_sub(1) } else { Some(r + 1).filter(|j| *j < o.rows.len()) };
                    if let Some(j) = j {
                        o.rows.swap(r, j);
                    }
                }
            }
            Message::RemoveInfoRow(i, r) => {
                if let Some(o) = self.infos_mut().get_mut(i)
                    && r < o.rows.len()
                {
                    o.rows.remove(r);
                }
            }
            Message::AddInfoCell(i, r) => {
                if let Some(row) = self.infos_mut().get_mut(i).and_then(|o| o.rows.get_mut(r)) {
                    row.push(String::new());
                }
            }
            Message::SetInfoCell(i, r, c, value) => {
                if let Some(cell) = self.infos_mut().get_mut(i).and_then(|o| o.rows.get_mut(r)).and_then(|row| row.get_mut(c)) {
                    *cell = value;
                }
            }
            Message::InsertInfoToken(i, r, c, token) => {
                if let Some(cell) = self.infos_mut().get_mut(i).and_then(|o| o.rows.get_mut(r)).and_then(|row| row.get_mut(c)) {
                    if !cell.is_empty() && !cell.ends_with(' ') {
                        cell.push(' ');
                    }
                    cell.push_str(&format!("{{{}}}", token.0));
                }
            }
            Message::RemoveInfoCell(i, r, c) => {
                if let Some(row) = self.infos_mut().get_mut(i).and_then(|o| o.rows.get_mut(r))
                    && c < row.len()
                {
                    row.remove(c);
                }
            }
            Message::ToggleMacro(i) => {
                if !self.open_macros.remove(&i) {
                    self.open_macros.insert(i);
                }
            }
            Message::NewMacro => {
                let name = self.new_item_name(ItemKind::Macro, "Macro");
                let tap = MacroStep::Tap { action: ButtonAction::Keys(Vec::new()), hold_ms: DEFAULT_TAP_MS };
                // New macros go first, right under the button that made them, already open.
                self.macros_mut().insert(0, Macro { name, steps: vec![tap] });
                self.open_macros = self.open_macros.iter().map(|j| j + 1).collect();
                self.open_macros.insert(0);
            }
            Message::DeleteMacro(i) => {
                if i < self.macros().len() {
                    self.macros_mut().remove(i);
                    self.open_macros = shift_removed(&self.open_macros, i);
                }
            }
            Message::RenameMacro(mi, name) => {
                if self.item_name_free(ItemKind::Macro, Some(mi), &name)
                    && let Some(m) = self.macros_mut().get_mut(mi)
                {
                    let old = std::mem::replace(&mut m.name, name.clone());
                    // Keep every mapping of this macro pointing at it.
                    self.follow_item_rename(ItemKind::Macro, &old, &name);
                }
            }
            Message::AddMacroStep(mi, kind) => {
                if let Some(m) = self.macros_mut().get_mut(mi) {
                    m.steps.push(convert_step(&MacroStep::Wait(0), kind));
                }
            }
            Message::SetMacroStep(mi, i, step) => {
                if let Some(s) = self.macros_mut().get_mut(mi).and_then(|m| m.steps.get_mut(i)) {
                    *s = step;
                }
            }
            Message::MoveMacroStep(mi, i, up) => {
                if let Some(m) = self.macros_mut().get_mut(mi) {
                    let j = if up { i.checked_sub(1) } else { Some(i + 1).filter(|j| *j < m.steps.len()) };
                    if let Some(j) = j {
                        m.steps.swap(i, j);
                    }
                }
            }
            Message::ToggleMenu(i) => {
                if !self.open_menus.remove(&i) {
                    self.open_menus.insert(i);
                }
            }
            Message::ToggleAppearance(which) => {
                if !self.open_appearance.remove(&which) {
                    self.open_appearance.insert(which);
                }
            }
            Message::NewMenu(kind) => {
                let name = self.new_item_name(ItemKind::Menu, "Menu");
                let kind = MenuKind::default_for(kind);
                let mut menu = Menu { name, kind, items: Vec::new(), cancel: None, style: OverlayStyle::default() };
                fit_items(&mut menu);
                if menu.items.is_empty() {
                    menu.items.push(MenuItem { label: "Item 1".into(), action: ButtonAction::Keys(Vec::new()), button: None });
                }
                // New menus go first, right under the button that made them, already open.
                self.menus_mut().insert(0, menu);
                self.open_menus = self.open_menus.iter().map(|j| j + 1).collect();
                self.open_menus.insert(0);
                self.open_appearance = self.open_appearance.iter().map(|a| a.map(|j| j + 1)).collect();
            }
            Message::DeleteMenu(i) => {
                if i < self.menus().len() {
                    self.menus_mut().remove(i);
                    // Later menus move up one place, and so do their open cards.
                    self.open_menus = shift_removed(&self.open_menus, i);
                    let menus_open: HashSet<usize> = self.open_appearance.iter().filter_map(|a| *a).collect();
                    let keyboard = self.open_appearance.contains(&None);
                    self.open_appearance = shift_removed(&menus_open, i).into_iter().map(Some).collect();
                    if keyboard {
                        self.open_appearance.insert(None);
                    }
                }
            }
            Message::RenameMenu(i, name) => {
                if self.item_name_free(ItemKind::Menu, Some(i), &name)
                    && let Some(m) = self.menus_mut().get_mut(i)
                {
                    let old = std::mem::replace(&mut m.name, name.clone());
                    // Keep every "Open menu" (in profiles and in other menus) pointing at it.
                    self.follow_item_rename(ItemKind::Menu, &old, &name);
                }
            }
            Message::SetMenuKind(i, kind) => {
                if let Some(m) = self.menus_mut().get_mut(i) {
                    m.kind = kind;
                    fit_items(m);
                }
            }
            Message::SetMenuStyle(i, style) => {
                if let Some(m) = self.menus_mut().get_mut(i) {
                    m.style = style;
                }
            }
            Message::AddMenuItem(i) => {
                if let Some(m) = self.menus_mut().get_mut(i)
                    && item_limit(m.kind).is_none_or(|limit| m.items.len() < limit)
                {
                    let label = format!("Item {}", m.items.len() + 1);
                    m.items.push(MenuItem { label, action: ButtonAction::Keys(Vec::new()), button: None });
                }
            }
            Message::RemoveMenuItem(i, item) => {
                if let Some(m) = self.menus_mut().get_mut(i)
                    && item < m.items.len()
                {
                    m.items.remove(item);
                }
            }
            Message::MoveMenuItem(i, item, up) => {
                if let Some(m) = self.menus_mut().get_mut(i) {
                    let j = if up { item.checked_sub(1) } else { Some(item + 1).filter(|j| *j < m.items.len()) };
                    if let Some(j) = j {
                        m.items.swap(item, j);
                    }
                }
            }
            Message::SetMenuItemLabel(i, item, label) => {
                if let Some(it) = self.menus_mut().get_mut(i).and_then(|m| m.items.get_mut(item)) {
                    it.label = label;
                }
            }
            Message::SetMenuItemButton(i, item, choice) => {
                if let Some(it) = self.menus_mut().get_mut(i).and_then(|m| m.items.get_mut(item)) {
                    it.button = choice.0;
                }
            }
            Message::InsertMotion(mi, motion) => {
                if let Some(m) = self.macros_mut().get_mut(mi) {
                    m.steps.extend(motion.steps());
                }
            }
            Message::RemoveMacroStep(mi, i) => {
                if let Some(m) = self.macros_mut().get_mut(mi)
                    && i < m.steps.len()
                {
                    m.steps.remove(i);
                }
            }
            other => return self.update_profile(other),
        }
        Task::none()
    }

    /// Points every reference to a renamed item at its new name: within its game, or for a
    /// shared item, in shared items and every game that doesn't have its own of that name.
    pub(super) fn follow_item_rename(&mut self, kind: ItemKind, old: &str, new: &str) {
        if !self.on_shared() {
            self.game_mut().rename_refs(kind, old, new);
            return;
        }
        self.config.shared.rename_refs(kind, old, new);
        let own = |g: &Game| g.names(kind).contains(&old);
        if !own(&self.config.general) {
            self.config.general.rename_refs(kind, old, new);
        }
        for g in self.config.games.iter_mut().filter(|g| !own(g)) {
            g.rename_refs(kind, old, new);
        }
    }

    /// Why an item of `kind` in the shown list (other than the one at `index`) can't be named
    /// `name`: taken there, or clashing between a game's items and shared ones.
    pub(super) fn name_clash(&self, kind: ItemKind, index: Option<usize>, name: &str) -> Option<String> {
        let list: Vec<&String> = match kind {
            ItemKind::Macro => self.macros().iter().map(|m| &m.name).collect(),
            ItemKind::Menu => self.menus().iter().map(|m| &m.name).collect(),
            ItemKind::Info => self.infos().iter().map(|o| &o.name).collect(),
            ItemKind::Layer => self.game().layers.iter().map(|l| &l.name).collect(),
        };
        if list.iter().enumerate().any(|(i, n)| Some(i) != index && *n == name) {
            return Some("name used twice".into());
        }
        let noun = kind.noun();
        if self.on_shared() {
            // A shared item can't take a name a game's own item uses.
            let (game, _) = self.config.all_games().find(|(_, g)| g.names(kind).contains(&name))?;
            Some(format!("{} has its own {noun} with this name", game.unwrap_or("General")))
        } else {
            self.config.shared.names(kind).contains(&name).then(|| format!("a shared {noun} has this name"))
        }
    }

    pub(super) fn item_name_free(&self, kind: ItemKind, index: Option<usize>, name: &str) -> bool {
        self.name_clash(kind, index, name).is_none()
    }

    /// What's wrong with the name of the item of `kind` at `index`, for its card.
    pub(super) fn name_problem(&self, kind: ItemKind, index: usize, name: &str) -> Option<String> {
        if name.trim().is_empty() {
            return Some("needs a name".into());
        }
        self.name_clash(kind, Some(index), name)
    }

    /// A free name for a new item of `kind`, "Macro", "Macro 2", ….
    pub(super) fn new_item_name(&self, kind: ItemKind, base: &str) -> String {
        unique_name(base, |n| !self.item_name_free(kind, None, n))
    }

    pub(super) fn view_macros(&self, names: &Names) -> Element<'_, Message> {
        let add = container(
            row![
                button(text("+ New macro")).style(button::secondary).on_press(Message::NewMacro),
                text("A macro plays a sequence of inputs.").size(13).color(MUTED_COLOR),
                space::horizontal(),
                button(text("Copy from another game…").size(13)).style(button::text).on_press(Message::OpenBrowse(ItemKind::Macro)),
                help(
                    "A macro plays a sequence of inputs. Map it to any button, gesture, combo, trigger, \
                     zone or menu item with the \"Macro…\" action."
                        .into(),
                ),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding(14)
        .width(Length::Fill)
        .style(style::card);
        let mut col = column![add].spacing(16);
        for (i, m) in self.macros().iter().enumerate() {
            col = col.push(self.view_macro_card(i, m, names));
        }
        col.into()
    }

    /// A macro as its own collapsible card: a summary line, or its steps when open.
    pub(super) fn view_macro_card<'a>(&'a self, mi: usize, m: &'a Macro, names: &Names) -> Element<'a, Message> {
        let open = self.open_macros.contains(&mi);
        let problem = self
            .name_problem(ItemKind::Macro, mi, &m.name)
            .or_else(|| m.steps.iter().filter_map(MacroStep::action).find_map(|a| action_problem(a, names)));
        let chevron = if open { "▾" } else { "▸" };
        let title = text(format!("{chevron}  {}", if m.name.is_empty() { "(unnamed)" } else { &m.name })).size(18);
        let title = if problem.is_some() { title.color(ERROR_COLOR) } else { title };
        let total: u64 = m.steps.iter().map(MacroStep::duration_ms).sum();
        let n = m.steps.len();
        let mut header = row![
            button(title).style(button::text).padding(0).on_press(Message::ToggleMacro(mi)),
            text(format!("{n} step{} · {total} ms", if n == 1 { "" } else { "s" }))
            .size(13)
            .color(MUTED_COLOR),
            space::horizontal(),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        if let Some(problem) = &problem {
            header = header.push(text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR));
        }
        header = header.push(button(text("Delete").size(13)).style(button::danger).on_press(Message::DeleteMacro(mi)));
        let mut col = column![header].spacing(12);
        if open {
            col = col.push(self.view_macro_editor(mi, m, names));
        }
        container(col).padding(14).width(Length::Fill).style(style::card).into()
    }

    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    pub(super) fn view_macro_editor<'a>(&'a self, mi: usize, m: &'a Macro, names: &Names) -> Element<'a, Message> {
        let last = m.steps.len().saturating_sub(1);
        let mut steps = column![].spacing(8);
        for (i, step) in m.steps.iter().enumerate() {
            let kind = dropdown(STEP_KINDS, Some(step_kind(step)), {
                let step = step.clone();
                move |k| Message::SetMacroStep(mi, i, convert_step(&step, k))
            })
            .width(120);
            let body: Element<'_, Message> = match step {
                MacroStep::Wait(ms) => row![
                    slider(10.0..=5000.0, *ms as f32, move |v| Message::SetMacroStep(mi, i, MacroStep::Wait(v as u64)))
                        .step(10.0_f32)
                        .width(220),
                    ms_field(*ms, move |v| Message::SetMacroStep(mi, i, MacroStep::Wait(v))),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                MacroStep::Tap { action, hold_ms } => {
                    let hold_ms = *hold_ms;
                    let held = action.clone();
                    column![
                        action_editor(action, Button::South, MACRO_STEP_KINDS, set_action(Target::MacroStep(mi, i)), KeyField::root(Target::MacroStep(mi, i)), names),
                        row![
                            text("held for").size(13),
                            slider(10.0..=1000.0, hold_ms as f32, move |v| {
                                Message::SetMacroStep(mi, i, MacroStep::Tap { action: held.clone(), hold_ms: v as u64 })
                            })
                            .step(10.0_f32)
                            .width(180),
                            {
                                let held = action.clone();
                                ms_field(hold_ms, move |v| Message::SetMacroStep(mi, i, MacroStep::Tap { action: held.clone(), hold_ms: v }))
                            },
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(4)
                    .into()
                }
                MacroStep::Stick { stick, x, y } => {
                    let (stick, x, y) = (*stick, *x, *y);
                    let preset = StickPreset::of(x, y);
                    let mut body = column![row![
                        dropdown([Stick::Left, Stick::Right], Some(stick), move |s| {
                            Message::SetMacroStep(mi, i, MacroStep::Stick { stick: s, x, y })
                        })
                        .width(140),
                        dropdown(StickPreset::ALL, Some(preset), move |p: StickPreset| {
                            let (x, y) = p.position().unwrap_or((x, y));
                            Message::SetMacroStep(mi, i, MacroStep::Stick { stick, x, y })
                        })
                        .width(170),
                    ]
                    .spacing(8)]
                    .spacing(4);
                    if preset == StickPreset::Custom {
                        // Sliders show up as positive, matching how people think of a stick.
                        body = body.push(
                            row![
                                text("Horizontal").size(12),
                                slider(-1.0..=1.0, x, move |v| Message::SetMacroStep(mi, i, MacroStep::Stick { stick, x: v, y }))
                                    .step(0.05_f32)
                                    .width(120),
                                text("Vertical").size(12),
                                slider(-1.0..=1.0, -y, move |v| Message::SetMacroStep(mi, i, MacroStep::Stick { stick, x, y: -v }))
                                    .step(0.05_f32)
                                    .width(120),
                                text(format!("{x:+.2}, {:+.2}", -y)).size(12),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        );
                    }
                    body.into()
                }
                MacroStep::Press(action) | MacroStep::Release(action) => action_editor(
                    action,
                    Button::South,
                    MACRO_STEP_KINDS,
                    set_action(Target::MacroStep(mi, i)),
                    KeyField::root(Target::MacroStep(mi, i)),
                    names,
                ),
            };
            let small = |label: &'static str, msg: Option<Message>| {
                button(text(label).size(13)).style(button::secondary).on_press_maybe(msg)
            };
            steps = steps.push(
                container(
                    row![
                        container(text(format!("{}.", i + 1))).width(28).padding(iced::Padding::ZERO.top(6)),
                        kind,
                        body,
                        space::horizontal(),
                        small("↑", (i > 0).then_some(Message::MoveMacroStep(mi, i, true))),
                        small("↓", (i < last).then_some(Message::MoveMacroStep(mi, i, false))),
                        small("✕", Some(Message::RemoveMacroStep(mi, i))),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Start),
                )
                .padding(8)
                .style(style::inset),
            );
        }
        let total: u64 = m.steps.iter().map(MacroStep::duration_ms).sum();
        let unreleased = unreleased_holds(m);
        let mut footer = column![
            row![
                text("Add step:").size(13),
                button(text("Tap").size(13)).style(button::secondary).on_press(Message::AddMacroStep(mi, StepKind::Tap)),
                button(text("Hold down").size(13)).style(button::secondary).on_press(Message::AddMacroStep(mi, StepKind::Press)),
                button(text("Release").size(13)).style(button::secondary).on_press(Message::AddMacroStep(mi, StepKind::Release)),
                button(text("Wait").size(13)).style(button::secondary).on_press(Message::AddMacroStep(mi, StepKind::Wait)),
                button(text("Move stick").size(13)).style(button::secondary).on_press(Message::AddMacroStep(mi, StepKind::Stick)),
                dropdown(Motion::ALL, None::<Motion>, move |m| Message::InsertMotion(mi, m)).placeholder("Insert motion…").width(230),
                space::horizontal(),
                text(format!("Plays for {total} ms")).size(13).color(MUTED_COLOR),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(6);
        if unreleased > 0 {
            footer = footer.push(
                text(format!(
                    "{unreleased} held input(s) are never released by a step; they are released when the macro ends."
                ))
                .size(12)
                .color(MUTED_COLOR),
            );
        }
        column![
            labeled(
                "Name",
                field("Macro name", &m.name).on_input(move |n| Message::RenameMacro(mi, n)).width(220).into(),
            ),
            text("Steps").size(16),
            steps,
            footer,
        ]
        .spacing(10)
        .into()
    }

    pub(super) fn view_menus<'a>(&'a self, names: &Names, reachable: &[&Menu]) -> Element<'a, Message> {
        let mut col = column![view_new_menu_card()].spacing(16);
        for (i, menu) in self.menus().iter().enumerate() {
            col = col.push(self.view_menu_card(i, menu, names, reachable));
        }
        col.into()
    }

    pub(super) fn view_infos(&self) -> Element<'_, Message> {
        let mut col = column![view_new_info_card()].spacing(16);
        for (i, o) in self.infos().iter().enumerate() {
            col = col.push(self.view_info_card(i, o));
        }
        col.into()
    }

    /// An info overlay as its own collapsible card.
    pub(super) fn view_info_card<'a>(&'a self, i: usize, o: &'a InfoOverlay) -> Element<'a, Message> {
        let open = self.open_infos.contains(&i);
        let problem = self.name_problem(ItemKind::Info, i, &o.name);
        let chevron = if open { "▾" } else { "▸" };
        let title = text(format!("{chevron}  {}", if o.name.is_empty() { "(unnamed)" } else { &o.name })).size(18);
        let title = if problem.is_some() { title.color(ERROR_COLOR) } else { title };
        let rows = o.rows.len();
        let shown = match (o.always, o.on_start) {
            (true, _) => "always shown (not by actions)".to_string(),
            (false, Some(s)) => format!("shown {s:.0} s when the game starts"),
            (false, None) => "shown by an action".to_string(),
        };
        let lingers = o.linger.filter(|_| !o.always).map(|s| format!(" · lingers {s:.0} s")).unwrap_or_default();
        let summary = format!("{rows} row{} · {shown}{lingers}", if rows == 1 { "" } else { "s" });
        let mut header = row![
            button(title).style(button::text).padding(0).on_press(Message::ToggleInfo(i)),
            text(summary).size(13).color(MUTED_COLOR),
            space::horizontal(),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        if let Some(problem) = problem {
            header = header.push(text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR));
        }
        header = header.push(button(text("Delete").size(13)).style(button::danger).on_press(Message::DeleteInfo(i)));
        let mut col = column![header].spacing(12);
        if open {
            col = col.push(self.view_info_editor(i, o));
        }
        container(col).padding(14).width(Length::Fill).style(style::card).into()
    }

    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    pub(super) fn view_info_editor<'a>(&'a self, i: usize, o: &'a InfoOverlay) -> Element<'a, Message> {
        let mut rows: Vec<Element<'a, Message>> = vec![
            labeled("Name", field("Info overlay name", &o.name).on_input(move |n| Message::RenameInfo(i, n)).width(240).into()),
            labeled(
                "Shown",
                column![
                    checkbox(o.always)
                        .label(if self.on_shared() { "Always, in every game" } else { "Always, while this game is active" })
                        .on_toggle(move |a| Message::SetInfoAlways(i, a)),
                ]
                .extend((!o.always).then(|| {
                    seconds_option(
                        if self.on_shared() { "When a game starts, for" } else { "When the game starts, for" },
                        o.on_start,
                        DEFAULT_START_SECONDS,
                        move |v| Message::SetInfoOnStart(i, v),
                    )
                }))
                .push(
                    text(if o.always {
                        "Always on screen, so actions can't show it. For an overlay shown only sometimes, untick \
                         this and map \"Toggle → Show info overlay…\" to a button (a Toggle can start on, to show \
                         it at launch until dismissed)."
                    } else {
                        "\"Starts\" is the first time the game is focused after launching; it fades out at the end. \
                         Or map \"Show info overlay…\" to a button: shown while held, or until pressed again if \
                         wrapped in Toggle (which can be set to start on, to keep it up until dismissed)."
                    })
                    .size(12)
                    .color(MUTED_COLOR),
                )
                .spacing(4)
                .into(),
            ),
        ];
        // Lingering is about being let go, which an always-shown overlay never is.
        if !o.always {
            rows.push(labeled(
                "Lingers",
                row![
                    seconds_option("After it's let go (or toggled off), stays for", o.linger, DEFAULT_LINGER_SECONDS, move |v| {
                        Message::SetInfoLinger(i, v)
                    }),
                    help("Then it fades out. Pressing the button again while it lingers keeps it up as usual.".into()),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ));
        }

        // Appearance, and a live preview with the fallback glyphs.
        let appearance_open = self.open_info_appearance.contains(&i);
        rows.push(labeled("", disclosure("Appearance", appearance_open, Message::ToggleInfoAppearance(i))));
        if appearance_open {
            rows.push(style_editor(&o.style, Rc::new(move |s| Message::SetInfoStyle(i, s))));
        }
        let sample = InfoOverlay { style: preview_style(&o.style), ..o.clone() };
        let view = crate::info::resolve(&sample, &crate::info::Live::sample(self.config.info_glyphs));
        rows.push(preview(crate::overlay::draw::info_panel(&view, self.preview_font())));

        // The grid: rows of cells, which line up in columns on screen.
        let small = |label: &'static str, msg: Option<Message>| button(text(label).size(13)).style(button::secondary).on_press_maybe(msg);
        let tokens: Vec<TokenChoice> = crate::info::TOKENS.iter().map(|(t, d)| TokenChoice(t, d)).collect();
        let mut grid = column![
            row![
                text("Cells").size(16),
                help(
                    "Each row's cells line up in columns. Type text, and insert {tokens}: button tokens \
                     ({south}, {lb}, {lt}, {start}, …) draw that button as the controller in use labels \
                     it (A, ✕ or B for {south}); others show live values such as {time}, {cpu} or {app}."
                        .into(),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        ]
        .spacing(8);
        let last = o.rows.len().saturating_sub(1);
        for (r, cells) in o.rows.iter().enumerate() {
            let mut line = row![].spacing(10);
            for (c, cell) in cells.iter().enumerate() {
                line = line.push(
                    row![
                        field("Text or {token}", cell).on_input(move |v| Message::SetInfoCell(i, r, c, v)).width(170),
                        dropdown(tokens.clone(), None::<TokenChoice>, move |t| Message::InsertInfoToken(i, r, c, t))
                            .placeholder("Insert…")
                            .menu_height(320)
                            .width(110),
                        small("✕", Some(Message::RemoveInfoCell(i, r, c))),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                );
            }
            line = line.push(small("+ Cell", Some(Message::AddInfoCell(i, r))));
            grid = grid.push(
                container(
                    row![
                        container(text(format!("{}.", r + 1))).width(28).padding(iced::Padding::ZERO.top(6)),
                        line.wrap().vertical_spacing(6),
                        space::horizontal(),
                        small("↑", (r > 0).then_some(Message::MoveInfoRow(i, r, true))),
                        small("↓", (r < last).then_some(Message::MoveInfoRow(i, r, false))),
                        small("✕", Some(Message::RemoveInfoRow(i, r))),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Start),
                )
                .padding(8)
                .style(style::inset),
            );
        }
        grid = grid.push(button(text("+ Add row").size(13)).style(button::secondary).on_press(Message::AddInfoRow(i)));
        rows.push(grid.into());
        column(rows).spacing(10).into()
    }

    /// A menu as its own collapsible card: a summary line, or the full editor when open.
    pub(super) fn view_menu_card<'a>(&'a self, mi: usize, menu: &'a Menu, names: &Names, reachable: &[&Menu]) -> Element<'a, Message> {
        let open = self.open_menus.contains(&mi);
        let problem = self
            .name_problem(ItemKind::Menu, mi, &menu.name)
            .or_else(|| menu.items.iter().find_map(|item| item_problem(menu, &item.action, reachable, names)));
        let chevron = if open { "▾" } else { "▸" };
        let title = text(format!("{chevron}  {}", if menu.name.is_empty() { "(unnamed)" } else { &menu.name })).size(18);
        let title = if problem.is_some() { title.color(ERROR_COLOR) } else { title };
        let items = menu.items.iter().filter(|i| !i.label.is_empty() || !matches!(i.action, ButtonAction::Disabled)).count();
        let mut header = row![
            button(title).style(button::text).padding(0).on_press(Message::ToggleMenu(mi)),
            text(format!("{} · {items} item{}", menu.kind.tag().short(), if items == 1 { "" } else { "s" }))
                .size(13)
                .color(MUTED_COLOR),
            space::horizontal(),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        if let Some(problem) = &problem {
            header = header.push(text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR));
        }
        header = header.push(button(text("Delete").size(13)).style(button::danger).on_press(Message::DeleteMenu(mi)));
        let mut col = column![header].spacing(12);
        if open {
            col = col.push(self.view_menu_editor(mi, menu, names, reachable));
        }
        container(col).padding(14).width(Length::Fill).style(style::card).into()
    }

    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    pub(super) fn view_menu_editor<'a>(&'a self, mi: usize, menu: &'a Menu, names: &Names, reachable: &[&Menu]) -> Element<'a, Message> {
        let mut rows: Vec<Element<'a, Message>> = vec![labeled(
            "Name",
            field("Menu name", &menu.name).on_input(move |n| Message::RenameMenu(mi, n)).width(240).into(),
        )];

        rows.push(labeled("Kind", menu_kind_row(mi, menu)));

        // Appearance, with a live preview.
        let appearance_open = self.open_appearance.contains(&Some(mi));
        rows.push(labeled("", disclosure("Appearance", appearance_open, Message::ToggleAppearance(Some(mi)))));
        if appearance_open {
            rows.push(style_editor(&menu.style, Rc::new(move |s| Message::SetMenuStyle(mi, s))));
        }
        if let Some(view) = MenuSession::open(std::slice::from_ref(menu), &menu.name, Opener { buttons: vec![Button::LeftBumper], ..Opener::default() })
            .and_then(|s| s.view(std::slice::from_ref(menu)))
        {
            let view = crate::menu::MenuView { style: preview_style(&menu.style), ..view };
            rows.push(preview(crate::overlay::draw::menu_panel(&view, self.preview_font())));
        }

        // Items.
        let direction_slots = match menu.kind {
            MenuKind::Directional { cluster } => Some(cluster.slots()),
            _ => None,
        };
        let quick = matches!(menu.kind, MenuKind::Buttons);
        // Items open only other menus of the same kind; radial menus open none.
        let radial = matches!(menu.kind, MenuKind::Radial { .. });
        let item_kinds = if radial { RADIAL_ITEM_KINDS } else { MENU_ITEM_KINDS };
        let item_names = Names {
            macros: names.macros.clone(),
            menus: reachable
                .iter()
                .filter(|m| m.name != menu.name && m.kind.tag() == menu.kind.tag())
                .map(|m| m.name.clone())
                .collect(),
            ..names.clone()
        };
        let last = menu.items.len().saturating_sub(1);
        let mut items = column![text("Items").size(16)].spacing(8);
        for (i, item) in menu.items.iter().enumerate() {
            let target = Target::MenuItem(mi, i);
            let name: String = match direction_slots {
                Some(slots) => format!("{}  {}", crate::menu::button_badge(slots[i]), ["up", "right", "down", "left"][i]),
                None => format!("{}.", i + 1),
            };
            let mut line = row![
                container(text(name)).width(70).padding(iced::Padding::ZERO.top(6)),
                field("Label", &item.label).on_input(move |l| Message::SetMenuItemLabel(mi, i, l)).width(150),
            ]
            .spacing(8)
            .align_y(Alignment::Start);
            if quick {
                let mut options = vec![QuickChoice(None)];
                options.extend(Button::ALL.into_iter().map(|b| QuickChoice(Some(b))));
                line = line.push(tooltip(
                    dropdown(options, Some(QuickChoice(item.button)), move |c| Message::SetMenuItemButton(mi, i, c)).width(90),
                    container(text("Quick-select button").size(13)).padding(8).style(style::tooltip),
                    tooltip::Position::Top,
                ));
            }
            line = line.push(action_editor(&item.action, Button::South, item_kinds, set_action(target), KeyField::root(target), &item_names));
            if direction_slots.is_none() {
                let small = |label: &'static str, msg: Option<Message>| {
                    button(text(label).size(13)).style(button::secondary).on_press_maybe(msg)
                };
                line = line
                    .push(space::horizontal())
                    .push(small("↑", (i > 0).then_some(Message::MoveMenuItem(mi, i, true))))
                    .push(small("↓", (i < last).then_some(Message::MoveMenuItem(mi, i, false))))
                    .push(small("✕", Some(Message::RemoveMenuItem(mi, i))));
            }
            let mut boxed = column![line].spacing(4);
            if let Some(problem) = item_problem(menu, &item.action, reachable, names) {
                boxed = boxed.push(text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR));
            }
            items = items.push(container(boxed).padding(8).style(style::inset));
        }
        if direction_slots.is_none() {
            items = items.push(add_menu_item_row(mi, menu));
        } else {
            items = items.push(
                text("Give a direction \"Open menu…\" to open another directional menu.").size(12).color(MUTED_COLOR),
            );
        }
        rows.push(items.into());
        column(rows).spacing(10).into()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::tests::*;

    #[test]
    fn macro_editing_steps_and_renames_follow_mappings() {
        let mut app = app();
        let _ = app.update(Message::NewMacro);
        assert!(app.open_macros.contains(&0), "a new macro opens as its own card");
        let _ = app.update(Message::AddMacroStep(0, StepKind::Wait));
        let _ = app.update(Message::AddMacroStep(0, StepKind::Press));
        let _ = app.update(Message::MoveMacroStep(0, 2, true));
        assert_eq!(
            app.config.general.macros[0].steps.iter().map(step_kind).collect::<Vec<_>>(),
            [StepKind::Tap, StepKind::Press, StepKind::Wait]
        );
        // The on-screen keyboard writes into a macro step.
        pick(&mut app, KeyField::root(Target::MacroStep(0, 0)), false, &["KEY_SPACE"]);
        assert_eq!(app.config.general.macros[0].steps[0].action(), Some(&ButtonAction::Keys(vec!["KEY_SPACE".into()])));

        let mapped = ButtonAction::toggle(ButtonAction::Macro { name: "Macro".into(), repeat: true });
        app.config.general.profiles[1].set_button(Button::West, mapped);
        let _ = app.update(Message::RenameMacro(0, "Jump spam".into()));
        // A second macro goes on top, and the first one's open card moves down with it.
        let _ = app.update(Message::NewMacro);
        assert_eq!(app.config.general.macros[1].name, "Jump spam");
        assert_eq!(app.open_macros, HashSet::from([0, 1]));
        let _ = app.update(Message::DeleteMacro(0));
        assert_eq!(app.open_macros, HashSet::from([0]));
        assert_eq!(
            app.config.general.profiles[1].button(Button::West),
            &ButtonAction::toggle(ButtonAction::Macro { name: "Jump spam".into(), repeat: true })
        );
        // Step 2 (Hold down) has no key yet, so saving is blocked until it gets one.
        assert!(app.validate().is_none() || app.validate().unwrap().contains("unknown key"));

        let _ = app.update(Message::DeleteMacro(0));
        assert!(app.validate().unwrap().contains("missing macro"), "{:?}", app.validate());
    }

    #[test]
    fn motions_and_stick_presets() {
        let d = DIAGONAL;
        let qcf = Motion::QuarterCircleForward.steps();
        assert_eq!(
            qcf,
            vec![
                MacroStep::Stick { stick: Stick::Left, x: 0.0, y: 1.0 },
                MacroStep::Wait(MOTION_FRAME_MS),
                MacroStep::Stick { stick: Stick::Left, x: d, y: d },
                MacroStep::Wait(MOTION_FRAME_MS),
                MacroStep::Stick { stick: Stick::Left, x: 1.0, y: 0.0 },
                MacroStep::Wait(MOTION_FRAME_MS),
            ]
        );
        for preset in StickPreset::ALL {
            if let Some((x, y)) = preset.position() {
                assert_eq!(StickPreset::of(x, y), preset);
            }
        }
        assert_eq!(StickPreset::of(0.3, -0.2), StickPreset::Custom);
    }

    #[test]
    fn grid_menus_stay_within_six_by_six() {
        let mut app = app();
        let _ = app.update(Message::NewMenu(MenuKindTag::List));
        for _ in 0..39 {
            let _ = app.update(Message::AddMenuItem(0));
        }
        assert_eq!(app.config.general.menus[0].items.len(), 40);
        let _ = app.update(Message::SetMenuKind(0, MenuKind::Grid { columns: 2 }));
        let menu = &app.config.general.menus[0];
        assert_eq!((menu.items.len(), menu.kind), (36, MenuKind::Grid { columns: 6 }), "trimmed to 6 × 6");
        let _ = app.update(Message::AddMenuItem(0));
        assert_eq!(app.config.general.menus[0].items.len(), 36, "full");

        for _ in 0..26 {
            let _ = app.update(Message::RemoveMenuItem(0, 0));
        }
        let _ = app.update(Message::SetMenuKind(0, MenuKind::Grid { columns: 1 }));
        assert_eq!(app.config.general.menus[0].kind, MenuKind::Grid { columns: 2 }, "10 items need 2 columns for 6 rows");
        for _ in 0..5 {
            let _ = app.update(Message::AddMenuItem(0));
        }
        assert_eq!(app.config.general.menus[0].items.len(), 12, "2 columns hold 12");
        assert_eq!(app.validate(), None);
    }

    #[test]
    fn menu_editor_creates_fits_renames_and_validates() {
        let mut app = app();
        let _ = app.update(Message::NewMenu(MenuKindTag::Directional));
        assert_eq!(app.config.general.menus[0].items.len(), 4, "a directional menu has four slots");
        assert!(app.open_menus.contains(&0), "a new menu opens as its own card");
        let _ = app.update(Message::SetMenuKind(0, MenuKind::List));
        let _ = app.update(Message::AddMenuItem(0));
        assert_eq!(app.config.general.menus[0].items.len(), 5);
        let _ = app.update(Message::SetMenuKind(0, MenuKind::Directional { cluster: Cluster::FaceButtons }));
        assert_eq!(app.config.general.menus[0].items.len(), 4, "switching back trims to four slots");

        // Items take any action; the key picker writes into them.
        let _ = app.update(Message::SetAction(Target::MenuItem(0, 0), ButtonAction::Keys(vec![])));
        pick(&mut app, KeyField::root(Target::MenuItem(0, 0)), false, &["KEY_M"]);
        assert_eq!(app.config.general.menus[0].items[0].action, ButtonAction::Keys(vec!["KEY_M".into()]));

        // A second menu goes on top; opened from the first and from a profile, it follows a rename.
        let _ = app.update(Message::NewMenu(MenuKindTag::Directional));
        assert_eq!(app.open_menus, HashSet::from([0, 1]), "the older card moved down, still open");
        let _ = app.update(Message::RenameMenu(0, "Weapons".into()));
        app.config.general.menus[1].items[1].action = ButtonAction::OpenMenu("Weapons".into());
        app.config.general.profiles[0].set_button(Button::Select, ButtonAction::OpenMenu("Weapons".into()));
        let _ = app.update(Message::RenameMenu(0, "Wheel".into()));
        assert_eq!(app.config.general.menus[1].items[1].action, ButtonAction::OpenMenu("Wheel".into()));
        assert_eq!(app.config.general.profiles[0].button(Button::Select), &ButtonAction::OpenMenu("Wheel".into()));
        assert_eq!(app.validate(), None);
        // Submenus must be the same kind as the menu opening them.
        let _ = app.update(Message::SetMenuKind(0, MenuKind::List));
        let err = app.validate().unwrap();
        assert!(err.contains("isn't a directional menu"), "{err}");
        let _ = app.update(Message::SetMenuKind(0, MenuKind::Directional { cluster: Cluster::DPad }));
        assert_eq!(app.validate(), None);

        // Deleting the first menu shifts the open cards along with the menus.
        let _ = app.update(Message::ToggleMenu(0));
        let _ = app.update(Message::ToggleAppearance(Some(1)));
        let _ = app.update(Message::DeleteMenu(0));
        assert_eq!(app.config.general.menus[0].name, "Menu");
        assert!(app.open_menus.contains(&0) && app.open_appearance.contains(&Some(0)));
        let _ = app.update(Message::DeleteMenu(0));
        let err = app.validate().unwrap();
        assert!(err.contains("missing menu"), "{err}");
    }

    #[test]
    fn games_own_their_items_and_can_use_shared_ones() {
        let mut app = with_game();
        let _ = app.update(Message::NewMacro);
        assert_eq!(app.config.games[0].macros[0].name, "Macro");
        assert!(app.config.general.macros.is_empty(), "a game's new items are its own");
        // General's profiles can't use it.
        app.config.general.profiles[0].set_button(Button::West, ButtonAction::Macro { name: "Macro".into(), repeat: false });
        let err = app.validate().unwrap();
        assert!(err.starts_with("General: ") && err.contains("missing macro"), "{err}");

        // A shared macro is usable everywhere.
        let _ = app.update(Message::SelectPage(Page::Game(None)));
        let _ = app.update(Message::SelectGameTab(GameTab::Macros));
        let _ = app.update(Message::SetSharedView(true));
        let _ = app.update(Message::NewMacro);
        let _ = app.update(Message::RenameMacro(0, "Screenshot".into()));
        app.config.general.profiles[0].set_button(Button::West, ButtonAction::Macro { name: "Screenshot".into(), repeat: false });
        app.config.games[0].profiles[0].set_button(Button::West, ButtonAction::Macro { name: "Screenshot".into(), repeat: false });
        assert_eq!(app.validate(), None);
        // Renaming a shared item follows into every game.
        let _ = app.update(Message::RenameMacro(0, "Snap".into()));
        let snap = ButtonAction::Macro { name: "Snap".into(), repeat: false };
        assert_eq!(app.config.general.profiles[0].button(Button::West), &snap);
        assert_eq!(app.config.games[0].profiles[0].button(Button::West), &snap);
        // A shared name can't be taken by a game's item, and a game's name isn't taken twice.
        let _ = app.update(Message::RenameMacro(0, "Macro".into()));
        assert_eq!(app.config.shared.macros[0].name, "Snap", "Doom has a macro named that");
        let _ = app.update(Message::SelectPage(Page::Game(Some("Doom".into()))));
        let _ = app.update(Message::RenameMacro(0, "Snap".into()));
        assert_eq!(app.config.games[0].macros[0].name, "Macro");
        let _ = app.update(Message::NewMacro);
        assert_eq!(app.config.games[0].macros[0].name, "Macro 2");
    }

    #[test]
    fn the_shared_switch_only_applies_to_item_tabs() {
        let mut app = app();
        app.config.general.macros.push(Macro { name: "Mine".into(), steps: vec![] });
        app.config.general.profiles[0].set_button(Button::West, ButtonAction::Macro { name: "Mine".into(), repeat: false });
        let _ = app.update(Message::SelectGameTab(GameTab::Macros));
        let _ = app.update(Message::SetSharedView(true));
        assert!(app.on_shared() && app.macros().is_empty());
        let _ = app.update(Message::SelectGameTab(GameTab::Profiles));
        assert!(!app.on_shared());
        assert!(app.names().macros.contains(&"Mine".to_string()), "General's profiles see General's macros");
    }

    #[test]
    fn name_clashes_say_where_the_other_item_is() {
        let mut app = with_game();
        app.config.games[0].macros.push(Macro { name: "Dodge".into(), steps: vec![] });
        app.config.shared.macros.push(Macro { name: "Dodge".into(), steps: vec![] });
        assert_eq!(app.name_problem(ItemKind::Macro, 0, "Dodge").as_deref(), Some("a shared macro has this name"));
        let _ = app.update(Message::SelectPage(Page::Game(None)));
        let _ = app.update(Message::SelectGameTab(GameTab::Macros));
        let _ = app.update(Message::SetSharedView(true));
        assert_eq!(app.name_problem(ItemKind::Macro, 0, "Dodge").as_deref(), Some("Doom has its own macro with this name"));
        assert_eq!(app.name_problem(ItemKind::Macro, 0, " ").as_deref(), Some("needs a name"));
    }

    #[test]
    fn info_overlays_can_show_at_start_and_fade() {
        let mut app = with_game();
        let _ = app.update(Message::SelectGameTab(GameTab::Info));
        let _ = app.update(Message::NewInfo);
        let _ = app.update(Message::SetInfoOnStart(0, Some(8.0)));
        let _ = app.update(Message::SetInfoAlways(0, false));
        let _ = app.update(Message::SetInfoLinger(0, Some(DEFAULT_LINGER_SECONDS)));
        let o = &app.game().info[0];
        assert_eq!((o.on_start, o.linger), (Some(8.0), Some(DEFAULT_LINGER_SECONDS)));
        let _ = app.view();
        let _ = app.update(Message::SetInfoLinger(0, None));
        let _ = app.update(Message::SetInfoOnStart(0, None));
        assert_eq!((app.game().info[0].on_start, app.game().info[0].linger), (None, None));

        // A toggle that starts on keeps an overlay up until pressed.
        let start = ButtonAction::Toggle(Toggled { action: Box::new(ButtonAction::ShowInfo("Info".into())), start_on: true });
        let _ = app.update(Message::SetAction(Target::Button(Button::Select), start.clone()));
        assert_eq!(summarize(&start), "Toggle Info “Info” (starts on)");
        assert_eq!(app.validate(), None);
    }

    #[test]
    fn info_overlay_editing() {
        let mut app = app();
        let _ = app.update(Message::NewInfo);
        assert!(app.open_infos.contains(&0));
        let _ = app.update(Message::AddInfoRow(0));
        let _ = app.update(Message::SetInfoCell(0, 2, 0, "Time".into()));
        let _ = app.update(Message::InsertInfoToken(0, 2, 0, TokenChoice("time", "")));
        let _ = app.update(Message::AddInfoCell(0, 2));
        assert_eq!(app.config.general.info[0].rows[2], ["Time {time}", ""]);
        let _ = app.update(Message::MoveInfoRow(0, 2, true));
        assert_eq!(app.config.general.info[0].rows[1][0], "Time {time}");
        let _ = app.update(Message::RemoveInfoCell(0, 1, 1));
        assert_eq!(app.config.general.info[0].rows[1].len(), 1);

        // "Show info overlay" mappings follow a rename; a missing one blocks saving.
        app.config.general.profiles[0].set_button(Button::Select, ButtonAction::ShowInfo("Info".into()));
        let _ = app.update(Message::RenameInfo(0, "Controls".into()));
        assert_eq!(app.config.general.profiles[0].button(Button::Select), &ButtonAction::ShowInfo("Controls".into()));
        // New overlays always show, so no action can show them; shown sometimes, one can.
        assert!(app.validate().unwrap().contains("always shown, so an action can't show it"), "{:?}", app.validate());
        assert!(!app.names().infos.contains(&"Controls".to_string()), "not offered in pickers");
        let _ = app.update(Message::SetInfoAlways(0, false));
        assert_eq!(app.validate(), None);
        assert!(app.names().infos.contains(&"Controls".to_string()));
        let _ = app.update(Message::DeleteInfo(0));
        assert!(app.validate().unwrap().contains("missing info overlay"));
        assert!(app.open_infos.is_empty());
    }

    #[test]
    fn menu_and_keyboard_style_edits() {
        let mut app = app();
        let _ = app.update(Message::NewMenu(MenuKindTag::List));
        let style = OverlayStyle {
            position: ScreenPosition::BottomRight,
            scale: 1.5,
            background: Paint::new("#000000", 0.5),
            ..OverlayStyle::default()
        };
        let _ = app.update(Message::SetMenuStyle(0, style.clone()));
        assert_eq!(app.config.general.menus[0].style, style);
        let _ = app.update(Message::SetKeyboardStyle(OverlayStyle::default()));
        assert_eq!(app.config.keyboard_style.position, ScreenPosition::Center);
        // Previews keep colors but never grow past 100% so they fit the window.
        assert_eq!(preview_style(&style).scale, 1.0);
        assert_eq!(preview_style(&style).background, style.background);
    }
}

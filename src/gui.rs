//! iced front-end. Talks to the daemon over IPC; falls back to editing the config file directly
//! when the daemon is not running.

use std::{borrow::Borrow, collections::HashSet, fmt, rc::Rc, str::FromStr, time::Duration};

use evdev::KeyCode;
use iced::{
    Alignment, Color, Element, Length, Subscription, Task,
    widget::{
        button, center, checkbox, column, container, mouse_area, opaque, pick_list, pin, row, rule,
        scrollable, slider, space, stack, svg, text, text_input, toggler, tooltip,
    },
};

use crate::{
    config::{
        Analog, Button, ButtonAction, CarouselControls, Cluster, Combo, Config, Game, GestureKind, GyroActivation,
        GyroConfig, GyroHorizontal, GyroInput, GyroMode, InfoOverlay, ItemKind, Macro, MacroStep, Menu, MenuItem,
        MenuKind, MenuKindTag, MouseButton, OverlayStyle, Paint, Profile, ProfileRef, Rule, RuleKind, ScopeRef,
        ScreenPosition, Stick, StickAction, StickConfig, Trigger, TriggerAction, WheelDirection, Zone, free_name,
    },
    engine::Opener,
    info::PadFamily,
    ipc::{self, FocusBackend, InputSnapshot, Request, Response, Status, WindowInfo},
    keyboard::{self, Layout},
    launchers, library,
    menu::MenuSession,
    pack, pad_svg, style,
};

mod layers;
mod packs;

use layers::IndicatorChoice;
use packs::{BrowseSource, Dialog, PackField};

pub fn run() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Controller App")
        .subscription(App::subscription)
        .window_size((1100.0, 900.0))
        .run()
}

const LABEL_WIDTH: f32 = 170.0;
const SIDEBAR_WIDTH: f32 = 210.0;
const ERROR_COLOR: Color = Color::from_rgb(0.9, 0.3, 0.3);
const MUTED_COLOR: Color = Color::from_rgb(0.55, 0.55, 0.6);

struct App {
    /// Working copy being edited.
    config: Config,
    /// Last copy known to be applied; `config != saved` means unsaved edits.
    saved: Config,
    /// `None` while the daemon is unreachable.
    status: Option<Status>,
    page: Page,
    /// The profile being edited, by index in the current game's profiles.
    editing: usize,
    game_tab: GameTab,
    /// On General's Macros, Menus and Info overlays tabs: show the shared items (usable by
    /// every game) instead of General's own.
    shared_view: bool,
    /// The read-only list of shared items under a game's own is open.
    shared_open: bool,
    /// Filters the sidebar's game list.
    game_search: String,
    message: Option<(String, bool)>,
    /// Latest physical input streamed from the daemon.
    live: Option<InputSnapshot>,
    picker: Option<KeyPicker>,
    dialog: Option<Dialog>,
    profile_tab: ProfileTab,
    /// Rows showing their full editor instead of a one-line summary.
    expanded: HashSet<Target>,
    /// Waiting for a controller button press to jump to its row.
    finding: bool,
    found: Option<Button>,
    /// Macros whose cards are open, by index in the shown list.
    open_macros: HashSet<usize>,
    /// Menus (by index) whose card is open.
    open_menus: HashSet<usize>,
    /// Appearance editors that are open: a menu's, or the keyboard's (`None`).
    open_appearance: HashSet<Option<usize>>,
    /// The numpad card's Appearance section is open.
    numpad_appearance: bool,
    /// Info overlays (by index) whose card, or Appearance section, is open.
    open_infos: HashSet<usize>,
    open_info_appearance: HashSet<usize>,
    /// The built-in game library.
    library: Vec<library::Entry>,
    /// Installed and running games, once looked up for the library picker.
    installed: Option<launchers::Installed>,
    /// Games and profiles renamed since the last save. The daemon still reports the active
    /// profile by its old name until then.
    renames: Vec<Rename>,
    /// The shown game's items changed since it was imported (see `pack::edited_items`),
    /// worked out after each edit rather than every frame.
    edited: Vec<String>,
    /// The layer shown on the Layers tab, by index in the game's layers.
    layer: usize,
    /// The profile a layer is shown over, by index in the game's profiles.
    compare: usize,
    /// What the layer editor edits: the compared profile with the layer on top (see
    /// `layers.rs`).
    layer_view: Option<Profile>,
    /// The layer name label's Appearance section is open.
    indicator_appearance: bool,
}

/// What the sidebar shows on the right.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Page {
    Overview,
    Settings,
    /// A game's page; `None` is General.
    Game(Option<String>),
}

/// Sections of a game's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameTab {
    Profiles,
    Layers,
    Macros,
    Menus,
    Info,
    Details,
}

impl GameTab {
    const ALL: [GameTab; 6] = [GameTab::Profiles, GameTab::Layers, GameTab::Macros, GameTab::Menus, GameTab::Info, GameTab::Details];
}

impl fmt::Display for GameTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            GameTab::Profiles => "Profiles",
            GameTab::Layers => "Layers",
            GameTab::Macros => "Macros",
            GameTab::Menus => "Menus",
            GameTab::Info => "Info overlays",
            GameTab::Details => "Details",
        })
    }
}

/// A rename not yet saved, for following the daemon's active profile.
#[derive(Debug, Clone, PartialEq)]
enum Rename {
    Game { old: String, new: String },
    Profile { game: Option<String>, old: String, new: String },
}

/// Where `at` is once these renames apply.
fn follow_renames(renames: &[Rename], mut at: ProfileRef) -> ProfileRef {
    for r in renames {
        match r {
            Rename::Game { old, new } if at.game.as_ref() == Some(old) => at.game = Some(new.clone()),
            Rename::Profile { game, old, new } if &at.game == game && &at.profile == old => at.profile = new.clone(),
            _ => {}
        }
    }
    at
}

/// A token in an info cell's "Insert…" list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TokenChoice(&'static str, &'static str);

impl fmt::Display for TokenChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{}}}  {}", self.0, self.1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Target {
    Button(Button),
    Trigger(Trigger),
    Combo(usize),
    Gesture(Button, GestureKind),
    Zone(Analog, usize),
    /// Step `.1` of macro `.0` (in the shown list), not part of any profile.
    MacroStep(usize, usize),
    /// Item `.1` of menu `.0` (in the shown list).
    MenuItem(usize, usize),
}

/// Sections of the profile editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileTab {
    Buttons,
    Sticks,
    Combos,
    Gyro,
}

/// A menu item's quick-select button in a picker; `None` means none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuickChoice(Option<Button>);

impl fmt::Display for QuickChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(b) => f.write_str(crate::menu::button_badge(b)),
            None => f.write_str("—"),
        }
    }
}

/// Open-card indices after item `i` is removed: later items move up one place.
fn shift_removed(set: &HashSet<usize>, i: usize) -> HashSet<usize> {
    set.iter().filter(|j| **j != i).map(|j| if *j > i { j - 1 } else { *j }).collect()
}

/// `base`, or `base 2`, `base 3`, … whichever isn't taken.
fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..)
        .map(|i| if i == 1 { base.to_string() } else { format!("{base} {i}") })
        .find(|n| !taken(n))
        .unwrap()
}

/// The "add an info overlay" card.
fn view_new_info_card<'a>() -> Element<'a, Message> {
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
fn view_new_menu_card<'a>() -> Element<'a, Message> {
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

/// A "▸ Label" / "▾ Label" toggle for an optional section.
fn disclosure<'a>(label: &str, open: bool, message: Message) -> Element<'a, Message> {
    let chevron = if open { "▾" } else { "▸" };
    button(text(format!("{chevron} {label}")).size(14)).style(button::text).padding([4, 0]).on_press(message).into()
}

type OnStyle<'a> = Rc<dyn Fn(OverlayStyle) -> Message + 'a>;

/// Colors offered as swatches; any other color can be typed as #rrggbb.
const SWATCHES: [&str; 12] = [
    "#000000", "#16181c", "#30343c", "#5a606b", "#e6e8eb", "#ffffff", "#2f5db0", "#1f7a7a", "#2e7d46", "#6a3fb0",
    "#a83232", "#c26a1d",
];

/// Position, size and colors of an overlay.
fn style_editor<'a>(style: &OverlayStyle, on_change: OnStyle<'a>) -> Element<'a, Message> {
    let with = |f: &dyn Fn(&mut OverlayStyle)| {
        let mut s = style.clone();
        f(&mut s);
        on_change(s)
    };
    // A small "screen" of 3×3 spots to click.
    let mut grid = column![].spacing(3);
    for r in 0..3 {
        let mut line = row![].spacing(3);
        for c in 0..3 {
            let pos = ScreenPosition::GRID[r * 3 + c];
            let chosen = pos == style.position;
            line = line.push(
                tooltip(
                    button(space().width(22).height(12))
                        .padding(2)
                        .style(if chosen { button::primary } else { button::secondary })
                        .on_press(with(&|s| s.position = pos)),
                    container(text(pos.to_string()).size(13)).padding(6).style(style::tooltip),
                    tooltip::Position::Top,
                ),
            );
        }
        grid = grid.push(line);
    }
    let position = row![grid, text(style.position.to_string()).size(13).color(MUTED_COLOR)]
        .spacing(12)
        .align_y(Alignment::Center);

    let scale = style.scale;
    let size = row![
        slider(50.0..=200.0, scale * 100.0, {
            let on_change = on_change.clone();
            let style = style.clone();
            move |v| on_change(OverlayStyle { scale: (v / 100.0 * 20.0).round() / 20.0, ..style.clone() })
        })
        .step(5.0_f32)
        .width(220),
        text(format!("{:.0}%", scale * 100.0)).size(13),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let paint_editor = |label: &'static str, paint: &Paint, set: fn(&mut OverlayStyle, Paint)| -> Element<'a, Message> {
        let mut swatches = row![].spacing(4).align_y(Alignment::Center);
        for hex in SWATCHES {
            let [r, g, b] = crate::config::parse_hex(hex).unwrap_or_default();
            let color = Color::from_rgb8(r, g, b);
            let chosen = paint.color.eq_ignore_ascii_case(hex);
            let pick = Paint { color: hex.into(), ..paint.clone() };
            swatches = swatches.push(
                button(space().width(16).height(16))
                    .padding(0)
                    .style(move |_, _| button::Style {
                        background: Some(color.into()),
                        border: iced::Border {
                            width: if chosen { 3.0 } else { 1.0 },
                            radius: 4.0.into(),
                            color: if chosen { Color::from_rgb8(0x4e, 0xa1, 0xff) } else { Color::from_rgb(0.5, 0.5, 0.5) },
                        },
                        ..button::Style::default()
                    })
                    .on_press(with(&|s| set(s, pick.clone()))),
            );
        }
        let typed = {
            let on_change = on_change.clone();
            let style = style.clone();
            let opacity = paint.opacity;
            move |hex: String| {
                let mut s = style.clone();
                set(&mut s, Paint { color: hex, opacity });
                on_change(s)
            }
        };
        let valid = paint.rgb().is_some();
        let opacity = {
            let on_change = on_change.clone();
            let style = style.clone();
            let color = paint.color.clone();
            move |v: f32| {
                let mut s = style.clone();
                set(&mut s, Paint { color: color.clone(), opacity: (v / 100.0).clamp(0.0, 1.0) });
                on_change(s)
            }
        };
        let mut line = row![
            swatches,
            field("#rrggbb", &paint.color).on_input(typed).width(90),
            slider(0.0..=100.0, paint.opacity * 100.0, opacity).step(5.0_f32).width(110),
            text(format!("{:.0}%", paint.opacity * 100.0)).size(12),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if !valid {
            line = line.push(text("not a #rrggbb color").size(12).color(ERROR_COLOR));
        }
        labeled(format!("    {label}"), line.into())
    };

    column![
        labeled("    Position", position.into()),
        labeled("    Size", size.into()),
        paint_editor("Background", &style.background, |s, p| s.background = p),
        paint_editor("Items", &style.items, |s, p| s.items = p),
        paint_editor("Selected item", &style.selected, |s, p| s.selected = p),
    ]
    .spacing(8)
    .into()
}

/// The preview keeps the real colors but caps the size so it fits the window.
fn preview_style(style: &OverlayStyle) -> OverlayStyle {
    OverlayStyle { scale: style.scale.min(1.0), ..style.clone() }
}

/// A live preview on a dark "screen", so transparency shows.
fn preview<'a>(panel: Element<'a, Message>) -> Element<'a, Message> {
    column![
        text("Preview").size(12).color(MUTED_COLOR),
        container(container(panel).center_x(Length::Fill))
            .padding(16)
            .width(Length::Fill)
            .style(|_: &iced::Theme| container::Style {
                background: Some(Color::from_rgb8(0x3a, 0x4a, 0x5c).into()),
                border: iced::Border { radius: 8.0.into(), ..iced::Border::default() },
                ..container::Style::default()
            }),
    ]
    .spacing(4)
    .into()
}

/// A directional menu always has exactly four slots (up, right, down, left), some maybe empty.
fn fit_items(menu: &mut Menu) {
    if let MenuKind::Directional { .. } = menu.kind {
        menu.items.truncate(4);
        while menu.items.len() < 4 {
            menu.items.push(MenuItem { label: String::new(), action: ButtonAction::Disabled, button: None });
        }
    }
}

/// What's wrong with a menu item's action: the usual checks, plus which menus it may open
/// (see [`crate::menu::can_open`]).
fn item_problem(menu: &Menu, action: &ButtonAction, menus: &[&Menu], names: &Names) -> Option<String> {
    if holds_layer(action) {
        return Some("a menu item can only toggle a layer (wrap it in Toggle)".into());
    }
    action_problem(action, names).or_else(|| {
        let mut problem = None;
        action.walk(&mut |a| {
            if let ButtonAction::OpenMenu(name) = a
                && problem.is_none()
            {
                if matches!(menu.kind, MenuKind::Radial { .. }) {
                    problem = Some("radial menus can't open other menus".to_string());
                } else if let Some(child) = menus.iter().find(|m| &m.name == name)
                    && child.kind.tag() != menu.kind.tag()
                {
                    problem = Some(format!("{name:?} isn't a {} menu", menu.kind.tag().short().to_lowercase()));
                }
            }
        });
        problem
    })
}

fn info_has_problem(info: &[InfoOverlay]) -> bool {
    info.iter().enumerate().any(|(i, o)| o.name.trim().is_empty() || info[..i].iter().any(|other| other.name == o.name))
}

fn menus_have_problem(menus: &[Menu], reachable: &[&Menu], names: &Names) -> bool {
    menus.iter().enumerate().any(|(i, m)| {
        m.name.trim().is_empty()
            || menus[..i].iter().any(|o| o.name == m.name)
            || m.items.iter().any(|item| item_problem(m, &item.action, reachable, names).is_some())
    })
}

/// "from <pack> 1.2 by <author>" for a game made from a pack.
fn origin_note(game: &Game) -> Option<String> {
    let origin = game.origin.as_ref()?;
    let by = if game.pack.author.is_empty() { String::new() } else { format!(" by {}", game.pack.author) };
    let source = if origin.library { "library" } else { "pack" };
    Some(format!("from {source}, version {}{by}", origin.version))
}

/// Macro, menu, info overlay and layer names, for the "Macro…"/"Open menu…"/"Show info
/// overlay…"/"Layer…" pickers and for checking references.
#[derive(Debug, Clone, Default)]
struct Names {
    macros: Vec<String>,
    menus: Vec<String>,
    infos: Vec<String>,
    layers: Vec<String>,
    /// False for shared items, which can't use layers (they're always a game's own).
    layers_allowed: bool,
}

/// The "+ New layer" choice in a layer picker; replaced by a new layer's name when chosen.
const NEW_LAYER: &str = "+ New layer";

impl Names {
    fn of(scope: &ScopeRef, layers_allowed: bool) -> Self {
        let list = |kind| scope.names(kind).into_iter().map(str::to_string).collect();
        Names {
            macros: list(ItemKind::Macro),
            menus: list(ItemKind::Menu),
            infos: list(ItemKind::Info),
            layers: list(ItemKind::Layer),
            layers_allowed,
        }
    }

    /// What a game's profiles and items can use: its own items, then shared ones.
    fn for_game(config: &Config, game: &Game) -> Self {
        Names::of(&config.scope_of(Some(game)), true)
    }

    /// What shared items can use: only other shared items.
    fn shared(config: &Config) -> Self {
        Names::of(&config.scope_of(None), false)
    }

    fn list(&self, kind: ItemKind) -> &[String] {
        match kind {
            ItemKind::Macro => &self.macros,
            ItemKind::Menu => &self.menus,
            ItemKind::Info => &self.infos,
            ItemKind::Layer => &self.layers,
        }
    }
}

/// Whether `action` holds a layer outside any Toggle, which a tap (a menu item) can't do.
fn holds_layer(action: &ButtonAction) -> bool {
    match action {
        ButtonAction::Layer(_) => true,
        ButtonAction::Multi(list) => list.iter().any(holds_layer),
        ButtonAction::Turbo { action, .. } => holds_layer(action),
        _ => false,
    }
}

/// What the free view functions need to know about the app beyond the profile itself.
struct Ui<'a> {
    names: &'a Names,
    expanded: &'a HashSet<Target>,
    /// Row picked by "Find by pressing", highlighted and open.
    found: Option<Button>,
    analog_triggers: bool,
    any_gyro: bool,
    /// Set while editing a layer: the profile shown is the layer over `base`.
    layer: Option<LayerMarks<'a>>,
}

/// A layer being edited, over the profile it's compared with.
#[derive(Clone, Copy)]
struct LayerMarks<'a> {
    layer: &'a crate::config::Layer,
    base: &'a Profile,
}

/// Something a layer overrides as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerPart {
    /// A button's action and gestures.
    Button(Button),
    Stick(Stick),
    /// A trigger's action and zones.
    Trigger(Trigger),
    Gyro,
}

impl LayerMarks<'_> {
    fn overrides(&self, part: LayerPart) -> bool {
        let l = self.layer;
        match part {
            LayerPart::Button(b) => l.buttons.contains_key(&b) || l.gestures.contains_key(&b),
            LayerPart::Stick(Stick::Left) => l.left_stick.is_some(),
            LayerPart::Stick(Stick::Right) => l.right_stick.is_some(),
            LayerPart::Trigger(Trigger::Left) => l.left_trigger.is_some(),
            LayerPart::Trigger(Trigger::Right) => l.right_trigger.is_some(),
            LayerPart::Gyro => l.gyro.is_some(),
        }
    }
}

/// In a layer, a part it doesn't override reads "Same as Gameplay: …" with an Override
/// button; an overridden one gets its editor and "Back to base". Outside layers, just the
/// editor.
fn layer_part<'a>(ui: &Ui, part: LayerPart, label: String, summary: String, editor: impl FnOnce() -> Vec<Element<'a, Message>>) -> Vec<Element<'a, Message>> {
    let Some(marks) = ui.layer else { return editor() };
    if marks.overrides(part) {
        let mut rows = vec![
            row![
                text(label).size(13).color(style_accent()),
                space::horizontal(),
                button(text("Back to base").size(13)).style(button::text).on_press(Message::RevertInput(part)),
            ]
            .align_y(Alignment::Center)
            .into(),
        ];
        rows.extend(editor());
        return rows;
    }
    vec![
        row![
            text(label).width(LABEL_WIDTH).color(MUTED_COLOR),
            text(format!("Same as {}: {summary}", marks.base.name)).color(MUTED_COLOR),
            space::horizontal(),
            button(text("Override").size(13)).style(button::secondary).on_press(Message::OverrideInput(part)),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into(),
    ]
}

/// A stick's mode in a few words.
fn stick_summary(cfg: &StickConfig) -> String {
    let mode = match &cfg.action {
        StickAction::Disabled => "Disabled".to_string(),
        StickAction::Gamepad { stick, .. } => format!("pad {stick}"),
        StickAction::Mouse { speed } => format!("mouse, {speed:.0} px/s"),
        StickAction::Scroll { speed } => format!("scroll, {speed:.0} notches/s"),
        StickAction::Keys { up, down, left, right } => {
            [up, left, down, right].map(|k| keyboard::label(k)).join("/")
        }
    };
    format!("{mode}{}", zones_note(cfg.zones.len()))
}

/// A trigger's action in a few words.
fn trigger_summary(cfg: &crate::config::TriggerConfig) -> String {
    let action = match &cfg.action {
        TriggerAction::Disabled => "Disabled".to_string(),
        TriggerAction::Gamepad(t) => format!("pad {t}"),
        TriggerAction::Button { action, .. } => summarize(action),
    };
    format!("{action}{}", zones_note(cfg.zones.len()))
}

/// " + 2 zones", or nothing.
fn zones_note(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => " + 1 zone".into(),
        n => format!(" + {n} zones"),
    }
}

impl Ui<'_> {
    fn is_open(&self, target: Target) -> bool {
        self.expanded.contains(&target) || matches!(target, Target::Button(b) if self.found == Some(b))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepKind {
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

const STEP_KINDS: [StepKind; 5] = [StepKind::Tap, StepKind::Press, StepKind::Release, StepKind::Wait, StepKind::Stick];
/// One frame at 60 fps: the usual gap between motion inputs in fighting games.
const MOTION_FRAME_MS: u64 = 17;
const DIAGONAL: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Stick positions offered for "Move stick" steps (y positive is down).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StickPreset {
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
    const ALL: [StickPreset; 10] = [
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

    fn position(self) -> Option<(f32, f32)> {
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

    fn of(x: f32, y: f32) -> Self {
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
enum Motion {
    QuarterCircleForward,
    QuarterCircleBack,
    DragonPunch,
    HalfCircleForward,
    HalfCircleBack,
}

impl Motion {
    const ALL: [Motion; 5] = [
        Motion::QuarterCircleForward,
        Motion::QuarterCircleBack,
        Motion::DragonPunch,
        Motion::HalfCircleForward,
        Motion::HalfCircleBack,
    ];

    fn directions(self) -> &'static [StickPreset] {
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
    fn steps(self) -> Vec<MacroStep> {
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
const DEFAULT_TAP_MS: u64 = 50;
const DEFAULT_WAIT_MS: u64 = 100;

fn step_kind(step: &MacroStep) -> StepKind {
    match step {
        MacroStep::Tap { .. } => StepKind::Tap,
        MacroStep::Press(_) => StepKind::Press,
        MacroStep::Release(_) => StepKind::Release,
        MacroStep::Wait(_) => StepKind::Wait,
        MacroStep::Stick { .. } => StepKind::Stick,
    }
}

/// A step of another kind, keeping the action (or timing) where it carries over.
fn convert_step(step: &MacroStep, kind: StepKind) -> MacroStep {
    let action = step.action().cloned().unwrap_or(ButtonAction::Keys(Vec::new()));
    match kind {
        StepKind::Tap => MacroStep::Tap { action, hold_ms: DEFAULT_TAP_MS },
        StepKind::Press => MacroStep::Press(action),
        StepKind::Release => MacroStep::Release(action),
        StepKind::Wait => MacroStep::Wait(DEFAULT_WAIT_MS),
        StepKind::Stick => MacroStep::Stick { stick: Stick::Left, x: 1.0, y: 0.0 },
    }
}

/// Address of a key field, so the on-screen keyboard can write its result back.
#[derive(Debug, Clone, PartialEq)]
enum KeyField {
    /// A `Keys` action at `target`, reached through these `Multi` list indices.
    Action { target: Target, path: Vec<usize> },
    /// One direction (0 up, 1 down, 2 left, 3 right) of a stick in direction-keys mode.
    StickDir { stick: Stick, dir: usize },
}

impl KeyField {
    fn root(target: Target) -> Self {
        KeyField::Action { target, path: Vec::new() }
    }

    fn child(&self, index: usize) -> Self {
        match self {
            KeyField::Action { target, path } => {
                let mut path = path.clone();
                path.push(index);
                KeyField::Action { target: *target, path }
            }
            other => other.clone(),
        }
    }
}

/// Open on-screen keyboard. `single` fields hold one key and close on the first click.
struct KeyPicker {
    field: KeyField,
    keys: Vec<String>,
    single: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Poll,
    LiveInput(Option<InputSnapshot>),
    StatusLoaded(Result<Status, String>),
    ConfigLoaded(Box<Config>, Option<String>),
    SetEnabled(bool),
    ActivateProfile(ProfileRef),
    Done(Result<(), String>),
    SelectPage(Page),
    SetGameSearch(String),
    SelectGameTab(GameTab),
    SetSharedView(bool),
    ToggleSharedSection,
    RenameGame(String),
    EditProfile(String),
    RenameProfile(String),
    AddProfile(Template),
    DeleteProfile,
    SetAction(Target, ButtonAction),
    SetStick(Stick, StickConfig),
    SetTrigger(Trigger, TriggerAction),
    SetIgnored(String, bool),
    SetAutoSwitch(bool),
    SetDefaultProfile(DefaultChoice),
    AddRule(Option<WindowInfo>),
    RemoveRule(usize),
    SetRuleKind(usize, RuleKind),
    SetRuleValue(usize, String),
    SetRuleProfile(usize, String),
    SetRuleEnabled(usize, bool),
    TestRumble(String),
    ToggleOverlay,
    ToggleNumpad,
    ToggleNumpadAppearance,
    SetNumpadStyle(OverlayStyle),
    CalibrateGyro(String),
    CopyMotionRuleCommand,
    SetGyro(GyroConfig),
    AddCombo,
    RemoveCombo(usize),
    AddComboButton(usize, Button),
    RemoveComboButton(usize, Button),
    SetComboWindow(f32),
    AddGesture(Button, GestureKind),
    RemoveGesture(Button, GestureKind),
    SetTapWindow(f32),
    SetLongPress(f32),
    AddZone(Analog, ZonePreset),
    RemoveZone(Analog, usize),
    SetZoneRange(Analog, usize, f32, f32),
    SelectProfileTab(ProfileTab),
    ToggleExpanded(Target),
    /// Expand (true) or collapse every row in the current profile section.
    ExpandAll(bool),
    StartFind,
    CancelFind,
    ToggleMacro(usize),
    NewInfo,
    ToggleInfo(usize),
    DeleteInfo(usize),
    RenameInfo(usize, String),
    SetInfoAlways(usize, bool),
    SetInfoStyle(usize, OverlayStyle),
    ToggleInfoAppearance(usize),
    AddInfoRow(usize),
    /// Row `.1` of info overlay `.0`, moved up if `.2`.
    MoveInfoRow(usize, usize, bool),
    RemoveInfoRow(usize, usize),
    AddInfoCell(usize, usize),
    /// Cell `.2` of row `.1` of info overlay `.0`.
    SetInfoCell(usize, usize, usize, String),
    InsertInfoToken(usize, usize, usize, TokenChoice),
    RemoveInfoCell(usize, usize, usize),
    SetInfoGlyphs(PadFamily),
    NewMacro,
    DeleteMacro(usize),
    RenameMacro(usize, String),
    AddMacroStep(usize, StepKind),
    /// Step `.1` of macro `.0`.
    SetMacroStep(usize, usize, MacroStep),
    MoveMacroStep(usize, usize, bool),
    RemoveMacroStep(usize, usize),
    InsertMotion(usize, Motion),
    ToggleMenu(usize),
    ToggleAppearance(Option<usize>),
    NewMenu(MenuKindTag),
    DeleteMenu(usize),
    RenameMenu(usize, String),
    SetMenuKind(usize, MenuKind),
    SetMenuStyle(usize, OverlayStyle),
    SetKeyboardStyle(OverlayStyle),
    AddMenuItem(usize),
    RemoveMenuItem(usize, usize),
    MoveMenuItem(usize, usize, bool),
    SetMenuItemLabel(usize, usize, String),
    SetMenuItemButton(usize, usize, QuickChoice),
    OpenKeyPicker(KeyField, Vec<String>, bool),
    PickerKey(&'static str),
    PickerClear,
    PickerClose { apply: bool },
    /// "+ Add game": the library picker.
    OpenAddGame,
    CloseDialog,
    SetLibrarySearch(String),
    /// Library entry to show details of (index in `App::library`).
    SelectLibrary(usize),
    Installed(launchers::Installed),
    AddEmptyGame(Template),
    /// Preview adding the library entry at this index.
    PreviewLibrary(usize),
    /// Preview the library's newer version of this game.
    UpdateFromLibrary(String),
    ImportFile,
    /// A pack file's text, or why it couldn't be read; `None` if the user cancelled.
    PackFileRead(Option<Result<String, String>>),
    SetReplaceGame(bool),
    /// Keep the other game's rule in rule clash `.0`.
    SetKeepMine(usize, bool),
    ConfirmImport,
    OpenExport,
    SetPackField(PackField, String),
    SetLibraryExport(bool),
    SaveExport,
    /// Where the pack was saved, or why not; `None` if the user cancelled.
    Exported(Option<Result<String, String>>),
    AskDeleteGame,
    ConfirmDeleteGame,
    OpenBrowse(ItemKind),
    NewLayer,
    SelectLayer(String),
    RenameLayer(String),
    DeleteLayer,
    /// Which of the game's profiles the layer editor shows the layer over.
    SetCompare(String),
    SetIndicator(IndicatorChoice),
    SetIndicatorStyle(OverlayStyle),
    ToggleIndicatorAppearance,
    /// Make the layer set this (a copy of the profile's, to edit).
    OverrideInput(LayerPart),
    /// Stop the layer setting this.
    RevertInput(LayerPart),
    /// Switch a profile combo (by its buttons) off in the layer, or back on.
    ToggleBaseCombo(Vec<Button>),
    BrowseFrom(BrowseSource),
    BrowseOpen(usize),
    CopyItem(usize),
    Save,
    Saved(Result<(), String>),
    Revert,
}

/// An entry in the auto-switch "Otherwise use" list.
#[derive(Debug, Clone, PartialEq)]
struct DefaultChoice(Option<ProfileRef>);

impl fmt::Display for DefaultChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(at) => at.fmt(f),
            None => f.write_str("(keep current profile)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZonePreset {
    Empty,
    /// Left Shift while a stick is only partly pushed (walk in WASD games).
    Walk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Template {
    Gamepad,
    Desktop,
    Action,
    Strategy,
    Platformer,
    Duplicate,
}

impl Template {
    /// A new profile from this template (Duplicate makes a passthrough one; copy instead).
    fn make(self, name: &str) -> Profile {
        match self {
            Template::Gamepad | Template::Duplicate => Profile::passthrough(name),
            Template::Desktop => Profile::desktop(name),
            Template::Action => Profile::pc_action(name),
            Template::Strategy => Profile::strategy(name),
            Template::Platformer => Profile::platformer(name),
        }
    }

    /// Offered in the "New from template" list (Duplicate has its own button).
    const NEW: [Template; 5] = [
        Template::Gamepad,
        Template::Action,
        Template::Strategy,
        Template::Platformer,
        Template::Desktop,
    ];

    fn base_name(self) -> &'static str {
        match self {
            Template::Gamepad => "Gamepad",
            Template::Desktop => "Desktop",
            Template::Action => "PC Action",
            Template::Strategy => "Strategy",
            Template::Platformer => "Platformer",
            Template::Duplicate => "Copy",
        }
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Template::Gamepad => "Gamepad passthrough",
            Template::Desktop => "Desktop navigation",
            Template::Action => "PC action (WASD + mouse look)",
            Template::Strategy => "Strategy (mouse pointer + hotkeys)",
            Template::Platformer => "Retro / platformer (arrows + Z/X/C)",
            Template::Duplicate => "Copy of this profile",
        })
    }
}

async fn call(req: Request) -> Result<Response, String> {
    tokio::task::spawn_blocking(move || ipc::request(&req))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

fn call_ok(req: Request) -> Task<Message> {
    Task::perform(call(req), |r| Message::Done(r.map(|_| ())))
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let config = Config::default();
        let app = App {
            saved: config.clone(),
            config,
            status: None,
            page: Page::Overview,
            editing: 0,
            game_tab: GameTab::Profiles,
            shared_view: false,
            shared_open: false,
            game_search: String::new(),
            message: None,
            live: None,
            picker: None,
            dialog: None,
            profile_tab: ProfileTab::Buttons,
            expanded: HashSet::new(),
            finding: false,
            found: None,
            open_macros: HashSet::new(),
            open_menus: HashSet::new(),
            open_appearance: HashSet::new(),
            numpad_appearance: false,
            open_infos: HashSet::new(),
            open_info_appearance: HashSet::new(),
            library: library::entries(),
            installed: None,
            renames: Vec::new(),
            edited: Vec::new(),
            layer: 0,
            compare: 0,
            layer_view: None,
            indicator_appearance: false,
        };
        let load = Task::perform(
            async {
                match call(Request::GetConfig).await {
                    Ok(Response::Config(c)) => (c, None),
                    _ => match tokio::task::spawn_blocking(Config::load).await {
                        Ok(Ok(c)) => (Box::new(c), None),
                        Ok(Err(e)) => (Box::default(), Some(format!("{e:#}"))),
                        Err(e) => (Box::default(), Some(e.to_string())),
                    },
                }
            },
            |(c, err)| Message::ConfigLoaded(c, err),
        );
        (app, Task::batch([load, Task::done(Message::Poll)]))
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::time::every(Duration::from_secs(1)).map(|_| Message::Poll),
            Subscription::run(watch_input),
        ])
    }

    /// The game whose page is shown (General when it's not a game page).
    fn game_key(&self) -> Option<&str> {
        match &self.page {
            Page::Game(key) => key.as_deref(),
            _ => None,
        }
    }

    fn game(&self) -> &Game {
        self.config.game(self.game_key()).unwrap_or(&self.config.general)
    }

    fn game_mut(&mut self) -> &mut Game {
        let index = self.game_key().and_then(|name| self.config.games.iter().position(|g| g.name == name));
        match index {
            Some(i) => &mut self.config.games[i],
            None => &mut self.config.general,
        }
    }

    /// The Macros, Menus and Info overlays tabs show shared items (on General's page).
    fn on_shared(&self) -> bool {
        let item_tab = matches!(self.game_tab, GameTab::Macros | GameTab::Menus | GameTab::Info);
        self.shared_view && item_tab && self.page == Page::Game(None)
    }

    fn macros(&self) -> &Vec<Macro> {
        if self.on_shared() { &self.config.shared.macros } else { &self.game().macros }
    }

    fn macros_mut(&mut self) -> &mut Vec<Macro> {
        if self.on_shared() { &mut self.config.shared.macros } else { &mut self.game_mut().macros }
    }

    fn menus(&self) -> &Vec<Menu> {
        if self.on_shared() { &self.config.shared.menus } else { &self.game().menus }
    }

    fn menus_mut(&mut self) -> &mut Vec<Menu> {
        if self.on_shared() { &mut self.config.shared.menus } else { &mut self.game_mut().menus }
    }

    fn infos(&self) -> &Vec<InfoOverlay> {
        if self.on_shared() { &self.config.shared.info } else { &self.game().info }
    }

    fn infos_mut(&mut self) -> &mut Vec<InfoOverlay> {
        if self.on_shared() { &mut self.config.shared.info } else { &mut self.game_mut().info }
    }

    /// What the shown profile and items can refer to.
    fn names(&self) -> Names {
        if self.on_shared() { Names::shared(&self.config) } else { Names::for_game(&self.config, self.game()) }
    }

    /// Points every reference to a renamed item at its new name: within its game, or for a
    /// shared item, in shared items and every game that doesn't have its own of that name.
    fn follow_item_rename(&mut self, kind: ItemKind, old: &str, new: &str) {
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
    fn name_clash(&self, kind: ItemKind, index: Option<usize>, name: &str) -> Option<String> {
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

    fn item_name_free(&self, kind: ItemKind, index: Option<usize>, name: &str) -> bool {
        self.name_clash(kind, index, name).is_none()
    }

    /// What's wrong with the name of the item of `kind` at `index`, for its card.
    fn name_problem(&self, kind: ItemKind, index: usize, name: &str) -> Option<String> {
        if name.trim().is_empty() {
            return Some("needs a name".into());
        }
        self.name_clash(kind, Some(index), name)
    }

    /// A free name for a new item of `kind`, "Macro", "Macro 2", ….
    fn new_item_name(&self, kind: ItemKind, base: &str) -> String {
        unique_name(base, |n| !self.item_name_free(kind, None, n))
    }

    /// The profile the editor shows: the edited profile, or on the Layers tab, the layer over
    /// the profile it's compared with.
    fn profile(&self) -> Option<&Profile> {
        if self.editing_layer() {
            return self.layer_view.as_ref();
        }
        self.game().profiles.get(self.editing)
    }

    fn profile_mut(&mut self) -> Option<&mut Profile> {
        if self.editing_layer() {
            return self.layer_view.as_mut();
        }
        let editing = self.editing;
        self.game_mut().profiles.get_mut(editing)
    }

    /// The edited profile, as the daemon names it.
    fn profile_ref(&self) -> Option<ProfileRef> {
        self.profile().map(|p| ProfileRef::new(self.game_key(), &p.name))
    }

    /// Forgets per-page state when another page or list is shown.
    fn reset_page_state(&mut self) {
        self.open_macros.clear();
        self.open_menus.clear();
        self.open_infos.clear();
        self.open_info_appearance.clear();
        self.open_appearance.retain(Option::is_none);
        self.expanded.clear();
        self.found = None;
        self.finding = false;
    }

    /// Shows a game's page, editing its active profile if it has it.
    fn show_game(&mut self, key: Option<String>) {
        self.page = Page::Game(key.clone());
        let active = self.config.active_ref();
        self.editing = if active.game == key {
            self.game().profiles.iter().position(|p| p.name == active.profile).unwrap_or(0)
        } else {
            0
        };
        if key.is_some() {
            self.shared_view = false;
        }
        self.layer = 0;
        self.compare = self.editing;
        self.reset_page_state();
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        // Polling and live input don't change the config, and arrive many times a second.
        let edits = !matches!(message, Message::Poll | Message::LiveInput(_) | Message::StatusLoaded(_));
        let message = self.new_layer_for(message);
        // Edits in the layer editor change a working copy; they're kept as the layer's
        // overrides.
        let before = self.layer_view.clone().filter(|_| self.editing_layer());
        let task = self.handle(message);
        if let Some(before) = before
            && self.editing_layer()
        {
            self.write_back(&before);
        }
        if edits {
            self.refresh_layer_view();
            self.edited = if self.page == Page::Game(None) { Vec::new() } else { pack::edited_items(self.game()) };
        }
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Poll => {
                return Task::perform(call(Request::Status), |r| {
                    Message::StatusLoaded(r.and_then(|r| match r {
                        Response::Status(s) => Ok(s),
                        other => Err(format!("unexpected reply: {other:?}")),
                    }))
                });
            }
            Message::LiveInput(snapshot) => {
                if self.finding
                    && let Some(b) = snapshot.as_ref().and_then(|s| newly_pressed(self.live.as_ref(), s))
                {
                    return self.jump_to(b);
                }
                self.live = snapshot;
            }
            Message::StatusLoaded(Ok(status)) => {
                // Mirror daemon-owned fields so both copies stay comparable.
                let active = ProfileRef { game: status.active_game.clone(), profile: status.active_profile.clone() };
                self.saved.enabled = status.enabled;
                self.saved.active = active.clone();
                self.config.enabled = status.enabled;
                self.config.active = follow_renames(&self.renames, active);
                self.status = Some(status);
            }
            Message::StatusLoaded(Err(_)) => self.status = None,
            Message::ConfigLoaded(config, err) => {
                let config = *config;
                self.saved = config.clone();
                self.config = config;
                if let Page::Game(key) = self.page.clone() {
                    let exists = key.as_deref().is_none_or(|k| self.config.games.iter().any(|g| g.name == k));
                    self.show_game(if exists { key } else { None });
                }
                if let Some(e) = err {
                    self.message = Some((format!("Could not load config: {e}"), true));
                }
            }
            Message::SetEnabled(on) => return call_ok(Request::SetEnabled(on)),
            Message::TestRumble(path) => return call_ok(Request::TestRumble(path)),
            Message::ToggleOverlay => return call_ok(Request::ToggleOverlay),
            Message::ToggleNumpad => return call_ok(Request::ToggleNumpad),
            Message::ToggleNumpadAppearance => self.numpad_appearance = !self.numpad_appearance,
            Message::SetNumpadStyle(style) => self.config.numpad_style = style,
            Message::CalibrateGyro(path) => {
                self.message = Some(("Calibrating gyro: keep the controller still for 2 seconds.".into(), false));
                return call_ok(Request::CalibrateGyro(path));
            }
            Message::CopyMotionRuleCommand => {
                self.message = Some(("Copied. Paste it into a terminal and enter your password.".into(), false));
                return iced::clipboard::write(motion_rule_command());
            }
            Message::SetGyro(gyro) => {
                if let Some(p) = self.profile_mut() {
                    p.gyro = gyro;
                }
            }
            Message::ActivateProfile(at) => {
                if self.saved.profile(&at).is_some() {
                    return Task::batch([call_ok(Request::Activate(at)), Task::done(Message::Poll)]);
                }
                self.message = Some(("Save the new profile before activating it.".into(), true));
            }
            Message::SelectPage(page) => {
                // Also how a dialog's "Open it" leaves it.
                self.dialog = None;
                match page {
                    Page::Game(key) => self.show_game(key),
                    other => {
                        self.page = other;
                        self.reset_page_state();
                    }
                }
            }
            Message::SetGameSearch(search) => self.game_search = search,
            Message::SelectGameTab(tab) => {
                self.game_tab = tab;
                self.found = None;
            }
            Message::SetSharedView(shared) => {
                self.shared_view = shared;
                self.reset_page_state();
            }
            Message::ToggleSharedSection => self.shared_open = !self.shared_open,
            Message::RenameGame(name) => {
                let Some(old) = self.game_key().map(str::to_string) else { return Task::none() };
                let taken = self.config.games.iter().any(|g| g.name == name && g.name != old);
                if !taken {
                    self.game_mut().name = name.clone();
                    if let Some(d) = self.config.auto_switch.default_profile.as_mut().filter(|d| d.game.as_ref() == Some(&old)) {
                        d.game = Some(name.clone());
                    }
                    let rename = Rename::Game { old, new: name.clone() };
                    self.config.active = follow_renames(std::slice::from_ref(&rename), self.config.active.clone());
                    self.renames.push(rename);
                    self.page = Page::Game(Some(name));
                }
            }
            Message::Done(Ok(())) => return Task::done(Message::Poll),
            Message::Done(Err(e)) => self.message = Some((e, true)),
            Message::EditProfile(name) => {
                if let Some(i) = self.game().profiles.iter().position(|p| p.name == name) {
                    self.editing = i;
                }
            }
            Message::RenameProfile(name) => {
                let editing = self.editing;
                let taken = self.game().profiles.iter().enumerate().any(|(i, p)| i != editing && p.name == name);
                let Some(old) = self.profile().map(|p| p.name.clone()).filter(|_| !taken) else { return Task::none() };
                let key = self.game_key().map(str::to_string);
                let game = self.game_mut();
                game.profiles[editing].name = name.clone();
                // Keep the game's rules, and the auto-switch default, pointing at it.
                for rule in game.rules.iter_mut().filter(|r| r.profile == old) {
                    rule.profile = name.clone();
                }
                let rename = Rename::Profile { game: key.clone(), old: old.clone(), new: name.clone() };
                if let Some(d) = self.config.auto_switch.default_profile.as_mut().filter(|d| d.game == key && d.profile == old) {
                    d.profile = name;
                }
                self.config.active = follow_renames(std::slice::from_ref(&rename), self.config.active.clone());
                self.renames.push(rename);
            }
            Message::SetAutoSwitch(on) => self.config.auto_switch.enabled = on,
            Message::SetDefaultProfile(choice) => self.config.auto_switch.default_profile = choice.0,
            Message::AddRule(window) => {
                let profile = self.profile().map(|p| p.name.clone()).unwrap_or_default();
                let rule = match window {
                    Some(w) => rule_for_window(&w, profile),
                    None => Rule::new(RuleKind::Executable, "", profile),
                };
                self.game_mut().rules.push(rule);
            }
            Message::RemoveRule(i) => {
                if i < self.game().rules.len() {
                    self.game_mut().rules.remove(i);
                }
            }
            Message::SetRuleKind(i, kind) => {
                if let Some(r) = self.game_mut().rules.get_mut(i) {
                    r.kind = kind;
                }
            }
            Message::SetRuleValue(i, value) => {
                if let Some(r) = self.game_mut().rules.get_mut(i) {
                    r.value = value;
                }
            }
            Message::SetRuleProfile(i, profile) => {
                if let Some(r) = self.game_mut().rules.get_mut(i) {
                    r.profile = profile;
                }
            }
            Message::SetRuleEnabled(i, on) => {
                if let Some(r) = self.game_mut().rules.get_mut(i) {
                    r.enabled = on;
                }
            }
            Message::AddProfile(template) => {
                let name = unique_name(template.base_name(), |n| self.game().profile(n).is_some());
                let profile = match template {
                    Template::Duplicate => {
                        let mut p = self.profile().cloned().unwrap_or_else(|| Profile::passthrough(""));
                        p.name = name;
                        p
                    }
                    other => other.make(&name),
                };
                self.game_mut().profiles.push(profile);
                self.editing = self.game().profiles.len() - 1;
            }
            Message::DeleteProfile => {
                let editing = self.editing;
                let game = self.game_mut();
                if game.profiles.len() > 1 && editing < game.profiles.len() {
                    game.profiles.remove(editing);
                    self.editing = editing.min(self.game().profiles.len() - 1);
                }
            }
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
            Message::SetAction(target, action) => {
                if let Some(p) = self.profile_mut() {
                    match target {
                        Target::Button(b) => p.set_button(b, action),
                        Target::Trigger(t) => {
                            if let TriggerAction::Button { action: a, .. } = p.trigger_mut(t) {
                                *a = action;
                            }
                        }
                        Target::Combo(i) => {
                            if let Some(c) = p.combos.get_mut(i) {
                                c.action = action;
                            }
                        }
                        Target::Gesture(b, kind) => {
                            *p.gestures.entry(b).or_default().slot(kind) = Some(action);
                        }
                        Target::Zone(a, i) => {
                            if let Some(z) = p.zones_mut(a).get_mut(i) {
                                z.action = action;
                            }
                        }
                        Target::MacroStep(..) | Target::MenuItem(..) => {}
                    }
                }
            }
            Message::AddGesture(b, kind) => {
                if let Some(p) = self.profile_mut() {
                    let slot = p.gestures.entry(b).or_default().slot(kind);
                    if slot.is_none() {
                        *slot = Some(ButtonAction::Keys(Vec::new()));
                    }
                }
            }
            Message::RemoveGesture(b, kind) => {
                if let Some(p) = self.profile_mut()
                    && let Some(g) = p.gestures.get_mut(&b)
                {
                    *g.slot(kind) = None;
                    if g.is_empty() {
                        p.gestures.remove(&b);
                    }
                }
            }
            Message::AddZone(a, preset) => {
                if let Some(p) = self.profile_mut() {
                    p.zones_mut(a).push(match preset {
                        ZonePreset::Empty => Zone { min: 0.0, max: 0.5, action: ButtonAction::Keys(Vec::new()) },
                        ZonePreset::Walk => Zone {
                            min: 0.0,
                            max: 0.75,
                            action: ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into()]),
                        },
                    });
                }
            }
            Message::RemoveZone(a, i) => {
                if let Some(p) = self.profile_mut()
                    && i < p.zones(a).len()
                {
                    p.zones_mut(a).remove(i);
                }
            }
            Message::SetZoneRange(a, i, min, max) => {
                if let Some(z) = self.profile_mut().and_then(|p| p.zones_mut(a).get_mut(i)) {
                    z.min = min;
                    z.max = max;
                }
            }
            Message::SelectProfileTab(tab) => {
                self.profile_tab = tab;
                self.found = None;
            }
            Message::ToggleExpanded(target) => {
                if !self.expanded.remove(&target) {
                    self.expanded.insert(target);
                }
                if let Target::Button(b) = target
                    && self.found == Some(b)
                {
                    self.found = None;
                    self.expanded.remove(&target);
                }
            }
            Message::ExpandAll(open) => {
                let targets: Vec<Target> = match self.profile_tab {
                    ProfileTab::Buttons => Button::ALL.into_iter().map(Target::Button).collect(),
                    ProfileTab::Sticks => [Stick::Left, Stick::Right]
                        .into_iter()
                        .flat_map(Button::stick_directions)
                        .map(Target::Button)
                        .collect(),
                    ProfileTab::Combos => {
                        (0..self.profile().map_or(0, |p| p.combos.len())).map(Target::Combo).collect()
                    }
                    ProfileTab::Gyro => Vec::new(),
                };
                for t in targets {
                    if open {
                        self.expanded.insert(t);
                    } else {
                        self.expanded.remove(&t);
                    }
                }
                self.found = None;
            }
            Message::StartFind => {
                self.finding = true;
                self.found = None;
            }
            Message::CancelFind => self.finding = false,
            Message::SetInfoGlyphs(family) => self.config.info_glyphs = family,
            Message::NewInfo => {
                let name = self.new_item_name(ItemKind::Info, "Info");
                let cells = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
                let overlay = InfoOverlay {
                    name,
                    always: true,
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
            Message::SetKeyboardStyle(style) => self.config.keyboard_style = style,
            Message::AddMenuItem(i) => {
                if let Some(m) = self.menus_mut().get_mut(i) {
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
            Message::OpenKeyPicker(field, keys, single) => {
                let keys = keys.into_iter().filter(|k| KeyCode::from_str(k).is_ok()).collect();
                self.picker = Some(KeyPicker { field, keys, single });
            }
            Message::PickerKey(code) => {
                if let Some(picker) = &mut self.picker {
                    if picker.single {
                        picker.keys = vec![code.to_string()];
                        return Task::done(Message::PickerClose { apply: true });
                    }
                    match picker.keys.iter().position(|k| k == code) {
                        Some(i) => {
                            picker.keys.remove(i);
                        }
                        None => picker.keys.push(code.to_string()),
                    }
                }
            }
            Message::PickerClear => {
                if let Some(picker) = &mut self.picker {
                    picker.keys.clear();
                }
            }
            Message::PickerClose { apply } => {
                if let Some(picker) = self.picker.take()
                    && apply
                {
                    self.apply_keys(picker.field, picker.keys);
                }
            }
            Message::SetTapWindow(ms) => {
                if let Some(p) = self.profile_mut() {
                    p.tap_window_ms = ms.round() as u64;
                }
            }
            Message::SetLongPress(ms) => {
                if let Some(p) = self.profile_mut() {
                    p.long_press_ms = ms.round() as u64;
                }
            }
            Message::AddCombo => {
                if let Some(p) = self.profile_mut() {
                    p.combos.push(Combo {
                        buttons: vec![Button::LeftBumper, Button::RightBumper],
                        action: ButtonAction::Keys(Vec::new()),
                    });
                    let new = Target::Combo(p.combos.len() - 1);
                    self.expanded.insert(new);
                }
            }
            Message::RemoveCombo(i) => {
                if let Some(p) = self.profile_mut()
                    && i < p.combos.len()
                {
                    p.combos.remove(i);
                    // Indices shift, so open state can't carry over.
                    self.expanded.retain(|t| !matches!(t, Target::Combo(_)));
                }
            }
            Message::AddComboButton(i, b) => {
                if let Some(c) = self.profile_mut().and_then(|p| p.combos.get_mut(i))
                    && !c.buttons.contains(&b)
                {
                    c.buttons.push(b);
                }
            }
            Message::RemoveComboButton(i, b) => {
                if let Some(c) = self.profile_mut().and_then(|p| p.combos.get_mut(i)) {
                    c.buttons.retain(|x| *x != b);
                }
            }
            Message::SetComboWindow(ms) => {
                if let Some(p) = self.profile_mut() {
                    p.combo_window_ms = ms.round() as u64;
                }
            }
            Message::SetStick(s, cfg) => {
                if let Some(p) = self.profile_mut() {
                    *p.stick_mut(s) = cfg;
                }
            }
            Message::SetTrigger(t, action) => {
                if let Some(p) = self.profile_mut() {
                    *p.trigger_mut(t) = action;
                }
            }
            Message::SetIgnored(name, ignored) => {
                // Applies immediately, independent of unsaved profile edits.
                for c in [&mut self.config, &mut self.saved] {
                    c.ignored_devices.retain(|n| n != &name);
                    if ignored {
                        c.ignored_devices.push(name.clone());
                    }
                }
                return self.push(self.saved.clone());
            }
            Message::Save => {
                if let Some(err) = self.validate() {
                    self.message = Some((err, true));
                    return Task::none();
                }
                return self.push(self.config.clone());
            }
            Message::Saved(Ok(())) => {
                self.saved = self.config.clone();
                self.renames.clear();
                self.message = Some(("Saved and applied.".into(), false));
                return Task::done(Message::Poll);
            }
            Message::Saved(Err(e)) => self.message = Some((e, true)),
            Message::Revert => {
                self.config = self.saved.clone();
                self.renames.clear();
                let exists = self.game_key().is_none_or(|k| self.config.games.iter().any(|g| g.name == k));
                if !exists {
                    self.show_game(None);
                }
                self.editing = self.editing.min(self.game().profiles.len().saturating_sub(1));
                self.message = None;
            }
            other => return self.update_layers(other),
        }
        Task::none()
    }

    /// Sends a config to the daemon, or writes the file if the daemon is not running.
    fn push(&self, config: Config) -> Task<Message> {
        let daemon_up = self.status.is_some();
        Task::perform(
            async move {
                if daemon_up {
                    call(Request::SetConfig(Box::new(config))).await.map(|_| ())
                } else {
                    tokio::task::spawn_blocking(move || config.save())
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| format!("{e:#}"))
                }
            },
            Message::Saved,
        )
    }

    /// Writes keys chosen in the on-screen keyboard into the field it was opened for.
    fn apply_keys(&mut self, field: KeyField, keys: Vec<String>) {
        if let KeyField::Action { target: Target::MenuItem(m, i), path } = &field {
            let item = self.menus_mut().get_mut(*m).and_then(|m| m.items.get_mut(*i));
            if let Some(action) = item.and_then(|item| action_at(&mut item.action, path)) {
                *action = ButtonAction::Keys(keys);
            }
            return;
        }
        if let KeyField::Action { target: Target::MacroStep(m, s), path } = &field {
            let step = self.macros_mut().get_mut(*m).and_then(|m| m.steps.get_mut(*s));
            if let Some(action) = step.and_then(|s| s.action_mut()).and_then(|a| action_at(a, path)) {
                *action = ButtonAction::Keys(keys);
            }
            return;
        }
        let Some(p) = self.profile_mut() else { return };
        match field {
            KeyField::StickDir { stick, dir } => {
                if let StickAction::Keys { up, down, left, right } = &mut p.stick_mut(stick).action {
                    let slot = match dir {
                        0 => up,
                        1 => down,
                        2 => left,
                        _ => right,
                    };
                    *slot = keys.into_iter().next().unwrap_or_default();
                }
            }
            KeyField::Action { target, path } => {
                let root = match target {
                    Target::Button(b) => Some(p.buttons.entry(b).or_insert(ButtonAction::Disabled)),
                    Target::Trigger(t) => match p.trigger_mut(t) {
                        TriggerAction::Button { action, .. } => Some(action),
                        _ => None,
                    },
                    Target::Combo(i) => p.combos.get_mut(i).map(|c| &mut c.action),
                    Target::Gesture(b, kind) => p.gestures.get_mut(&b).and_then(|g| g.slot(kind).as_mut()),
                    Target::Zone(a, i) => p.zones_mut(a).get_mut(i).map(|z| &mut z.action),
                    // Handled above, outside any profile.
                    Target::MacroStep(..) | Target::MenuItem(..) => None,
                };
                if let Some(action) = root.and_then(|a| action_at(a, &path)) {
                    *action = ButtonAction::Keys(keys);
                }
            }
        }
    }

    /// False only when every controller in use has on/off triggers (e.g. Switch pads), in
    /// which case pull-depth settings are hidden. With nothing detected, assume analog.
    fn analog_triggers(&self) -> bool {
        let Some(status) = &self.status else { return true };
        let mut in_use = status.devices.iter().filter(|d| !d.ignored).peekable();
        in_use.peek().is_none() || in_use.any(|d| d.analog_triggers)
    }

    /// Whether any managed controller has a gyro (only affects hints, not what can be set).
    fn any_gyro(&self) -> bool {
        self.status.as_ref().is_some_and(|s| s.devices.iter().any(|d| d.gyro))
    }

    /// Opens and highlights `b`'s row and scrolls near it.
    fn jump_to(&mut self, b: Button) -> Task<Message> {
        self.finding = false;
        self.found = Some(b);
        if self.game_tab != GameTab::Layers {
            self.game_tab = GameTab::Profiles;
        }
        let (tab, position) = match Button::ALL.iter().position(|x| *x == b) {
            Some(i) => (ProfileTab::Buttons, i as f32 / Button::ALL.len() as f32),
            None => {
                let right = b.stick_direction().is_some_and(|(s, _)| s == Stick::Right);
                (ProfileTab::Sticks, if right { 0.75 } else { 0.25 })
            }
        };
        self.profile_tab = tab;
        iced::widget::operation::snap_to("main", scrollable::RelativeOffset { x: Some(0.0), y: Some(position) })
    }

    /// `p` with the layers the daemon says are on right now (they belong to the active game).
    fn with_active_layers(&self, p: &Profile) -> Profile {
        let layers = self.status.as_ref().map(|s| s.active_layers.as_slice()).unwrap_or_default();
        p.with_layers(self.config.active_game().layers_named(layers))
    }

    fn validate(&self) -> Option<String> {
        let config = &self.config;
        let shared = Names::shared(config);
        let reachable = reachable_menus(config, None);
        if let Some(problem) = items_problem(&config.shared.macros, &config.shared.menus, &config.shared.info, &shared, &reachable, None) {
            return Some(format!("Shared: {problem}"));
        }
        for (i, g) in config.games.iter().enumerate() {
            if g.name.trim().is_empty() {
                return Some("Game names cannot be empty.".into());
            }
            if config.games[..i].iter().any(|o| o.name == g.name) {
                return Some(format!("Two games are named {:?}.", g.name));
            }
        }
        for (key, g) in config.all_games() {
            let label = key.unwrap_or("General");
            if let Some(problem) = game_problem(config, g) {
                return Some(format!("{label}: {problem}"));
            }
        }
        if let Some(d) = config.auto_switch.default_profile.as_ref().filter(|d| config.profile(d).is_none()) {
            return Some(format!("The per-game default profile {d} no longer exists."));
        }
        None
    }

    fn view(&self) -> Element<'_, Message> {
        let page: Element<'_, Message> = match &self.page {
            Page::Overview => column![
                self.view_live(self.config.active().map(|p| self.with_active_layers(p)).as_ref()),
                rule::horizontal(1),
                self.view_devices()
            ]
                .spacing(16)
                .into(),
            Page::Settings => self.view_settings(),
            Page::Game(_) => self.view_game(),
        };
        let content = column![self.view_header(), rule::horizontal(1), page].spacing(16).padding(20);
        let main = column![scrollable(content).id("main").height(Length::Fill), self.view_footer()].width(Length::Fill);
        // Always a stack with the page first, popups or not: iced keeps widget state (such as
        // the page's scroll position) by place in the tree, so the shape must not change when
        // a popup opens or closes.
        let base = row![self.view_sidebar(), rule::vertical(1), main];
        let popup = match (&self.picker, &self.dialog) {
            (Some(picker), _) => Some(view_picker(picker)),
            (None, Some(dialog)) => Some(self.view_dialog(dialog)),
            (None, None) => None,
        };
        let mut layers = stack![base];
        if let Some(popup) = popup {
            layers = layers.push(popup);
        }
        layers.into()
    }

    /// Overview and Settings, then General and the games, with a search over the games.
    fn view_sidebar(&self) -> Element<'_, Message> {
        let entry = |label: String, page: Page, note: Option<&'static str>| -> Element<'_, Message> {
            let selected = self.page == page;
            let mut line = row![text(label).size(15).width(Length::Fill)].spacing(6).align_y(Alignment::Center);
            if let Some(note) = note {
                line = line.push(text(note).size(12).color(style_accent()));
            }
            button(line)
                .width(Length::Fill)
                .padding([6, 10])
                .style(style::nav(selected))
                .on_press(Message::SelectPage(page))
                .into()
        };
        let active_game = self.config.active_ref().game;
        let playing = |key: &Option<String>| (&active_game == key && self.status.is_some()).then_some("●");
        let updates: Vec<&str> = library::updates(&self.config, &self.library).into_iter().map(|(name, _)| name).collect();

        let mut col = column![
            entry("Overview".into(), Page::Overview, None),
            entry("Settings".into(), Page::Settings, None),
            rule::horizontal(1),
            entry("General".into(), Page::Game(None), playing(&None)),
            text("Games").size(13).color(MUTED_COLOR),
            field("Search games", &self.game_search).on_input(Message::SetGameSearch).size(14),
        ]
        .spacing(6);
        let search = self.game_search.trim().to_lowercase();
        let mut games: Vec<&Game> = self.config.games.iter().filter(|g| g.name.to_lowercase().contains(&search)).collect();
        games.sort_by_key(|g| g.name.to_lowercase());
        for g in games {
            let key = Some(g.name.clone());
            let note = if updates.contains(&g.name.as_str()) { Some("update") } else { playing(&key) };
            let label = if g.name.is_empty() { "(unnamed)".to_string() } else { g.name.clone() };
            col = col.push(entry(label, Page::Game(key), note));
        }
        if self.config.games.is_empty() {
            col = col.push(text("No games yet.").size(13).color(MUTED_COLOR));
        }
        col = col.push(button(text("+ Add game").size(14)).width(Length::Fill).style(button::secondary).on_press(Message::OpenAddGame));
        container(scrollable(col.padding(12)).height(Length::Fill)).width(SIDEBAR_WIDTH).into()
    }

    /// A game's (or General's) page: its name and sub-tabs.
    fn view_game(&self) -> Element<'_, Message> {
        let general = self.game_key().is_none();
        let game = self.game();
        let names = self.names();
        let mut title = row![text(if general { "General" } else { game.name.as_str() }).size(26)]
            .spacing(12)
            .align_y(Alignment::Center);
        if let Some(note) = origin_note(game) {
            title = title.push(text(note).size(13).color(MUTED_COLOR));
        }
        if general {
            title = title.push(help(
                "Profiles for no particular game, such as the desktop or a plain gamepad. Its \
                 Macros, Menus and Info overlays tabs also hold the shared items every game can use."
                    .into(),
            ));
        }

        let profile_issue = self.profile().is_some_and(|p| ProfileTab::ALL.iter().any(|t| section_has_problem(p, *t, &names)));
        let reachable = reachable_menus(&self.config, (!self.on_shared()).then_some(game));
        let issue = |tab: GameTab| match tab {
            GameTab::Profiles => profile_issue,
            GameTab::Layers => layers_problem(game, &names).is_some(),
            GameTab::Macros => macros_have_problem(self.macros(), &names),
            GameTab::Menus => menus_have_problem(self.menus(), &reachable, &names),
            GameTab::Info => info_has_problem(self.infos()),
            GameTab::Details => !general && (rules_problem(game).is_some() || game.name.trim().is_empty()),
        };
        let mut segments = row![].spacing(2);
        for tab in GameTab::ALL.into_iter().filter(|t| !(general && *t == GameTab::Details)) {
            let label = if issue(tab) { format!("{tab}  ⚠") } else { tab.to_string() };
            segments = segments.push(
                button(text(label).size(15))
                    .style(style::segment(self.game_tab == tab))
                    .padding([6, 16])
                    .on_press(Message::SelectGameTab(tab)),
            );
        }
        let tab = if general && self.game_tab == GameTab::Details { GameTab::Profiles } else { self.game_tab };
        let mut col = column![title, container(segments).padding(3).style(style::segments)].spacing(16);

        let item_tab = matches!(tab, GameTab::Macros | GameTab::Menus | GameTab::Info);
        if general && item_tab {
            let choice = |label: &'static str, shared: bool| {
                button(text(label).size(14))
                    .style(style::segment(self.shared_view == shared))
                    .padding([5, 14])
                    .on_press(Message::SetSharedView(shared))
            };
            col = col.push(
                row![
                    container(row![choice("General's own", false), choice("Shared by all games", true)].spacing(2))
                        .padding(3)
                        .style(style::segments),
                    help(
                        "Shared macros, menus and info overlays can be used by every profile of every game. \
                         A game's own items can't reuse a shared item's name."
                            .into(),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        let body = match tab {
            GameTab::Profiles => self.view_profile_tab(&names),
            GameTab::Layers => self.view_layers(&names),
            GameTab::Macros => self.view_macros(&names),
            GameTab::Menus => self.view_menus(&names, &reachable),
            GameTab::Info => self.view_infos(),
            GameTab::Details => self.view_details(),
        };
        col = col.push(body);
        if item_tab && !self.on_shared() {
            col = col.push(self.view_shared_section(match tab {
                GameTab::Macros => ItemKind::Macro,
                GameTab::Menus => ItemKind::Menu,
                _ => ItemKind::Info,
            }));
        }
        col.into()
    }

    /// The shared items a game's profiles can also use, read-only.
    fn view_shared_section(&self, kind: ItemKind) -> Element<'_, Message> {
        let names = Names::shared(&self.config).list(kind).to_vec();
        let mut col = column![disclosure(
            &format!("Shared {}s usable here ({})", kind.noun(), names.len()),
            self.shared_open,
            Message::ToggleSharedSection
        )]
        .spacing(6);
        if self.shared_open {
            if names.is_empty() {
                col = col.push(text(format!("No shared {}s.", kind.noun())).size(13).color(MUTED_COLOR));
            }
            for name in names {
                col = col.push(text(format!("• {name}")).size(14));
            }
            col = col.push(
                button(text("Edit shared items under General").size(13))
                    .style(button::text)
                    .on_press(Message::SelectPage(Page::Game(None))),
            );
        }
        col.into()
    }

    fn view_settings(&self) -> Element<'_, Message> {
        let glyphs = section(
            "Info overlays",
            None,
            vec![labeled(
                "Fallback glyphs",
                row![
                    dropdown(PadFamily::ALL, Some(self.config.info_glyphs), Message::SetInfoGlyphs).width(170),
                    help(
                        "Glyphs follow the controller in use: Xbox, PlayStation or Nintendo labels. For a \
                         controller that can't be recognized, they're drawn like this kind instead. Also \
                         used for previews."
                            .into(),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            )],
        );
        column![self.view_auto_switch(), self.view_keyboard_card(), self.view_numpad_card(), glyphs].spacing(16).into()
    }

    fn view_profile_tab<'a>(&'a self, names: &Names) -> Element<'a, Message> {
        let col = column![self.view_profile_bar()].spacing(16);
        let Some(p) = self.profile() else { return col.into() };
        col.push(self.view_profile_editor(p, names, None)).into()
    }

    /// The live drawing, "Find by pressing", and the Buttons, Sticks & triggers, Combos and
    /// Gyro tabs for `p`: a profile, or (with `layer`) a layer over one.
    fn view_profile_editor<'a>(&'a self, p: &'a Profile, names: &Names, layer: Option<LayerMarks<'a>>) -> Element<'a, Message> {
        // The active profile's drawing also shows the layers that are on right now.
        let live = (layer.is_none() && self.profile_ref().is_some_and(|at| at == self.saved.active)).then(|| self.with_active_layers(p));
        let mut col = column![self.view_live(Some(live.as_ref().unwrap_or(p)))].spacing(16);

        let find: Element<'_, Message> = if self.finding {
            row![
                text("Press a button or push a stick on your controller…").color(style_accent()),
                button(text("Cancel")).style(button::secondary).on_press(Message::CancelFind),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
        } else {
            row![
                button(text("Find by pressing")).style(button::secondary).on_press(Message::StartFind),
                help("Press a button or push a stick on your controller to jump to its mapping.".into()),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        };
        let sub_tab = |t: ProfileTab| {
            let label = if section_has_problem(p, t, names) { format!("{t}  ⚠") } else { t.to_string() };
            button(text(label).size(14))
                .style(style::segment(self.profile_tab == t))
                .padding([5, 14])
                .on_press(Message::SelectProfileTab(t))
        };
        let mut segments = row![].spacing(2);
        for t in ProfileTab::ALL {
            segments = segments.push(sub_tab(t));
        }
        let mut tabs = row![container(segments).padding(3).style(style::segments), space::horizontal()]
            .spacing(6)
            .align_y(Alignment::Center);
        if self.profile_tab != ProfileTab::Gyro {
            tabs = tabs
                .push(button(text("Expand all").size(13)).style(button::text).on_press(Message::ExpandAll(true)))
                .push(button(text("Collapse all").size(13)).style(button::text).on_press(Message::ExpandAll(false)));
        }
        col = col.push(row![find].align_y(Alignment::Center)).push(tabs);

        let ui = Ui {
            names,
            expanded: &self.expanded,
            found: self.found,
            analog_triggers: self.analog_triggers(),
            any_gyro: self.any_gyro(),
            layer,
        };
        col.push(view_profile(p, &ui, self.profile_tab)).into()
    }

    fn view_header(&self) -> Element<'_, Message> {
        let (status_text, color) = match &self.status {
            Some(_) => ("● Daemon running".to_string(), Color::from_rgb(0.3, 0.8, 0.4)),
            None => ("○ Daemon not running".to_string(), ERROR_COLOR),
        };
        let profiles: Vec<ProfileRef> = self
            .saved
            .all_games()
            .flat_map(|(key, g)| g.profiles.iter().map(move |p| ProfileRef::new(key, &p.name)))
            .collect();
        let running = self.status.is_some();

        let mut header = column![
            row![
                text("Controller App").size(26),
                space::horizontal(),
                text(status_text).color(color),
            ]
            .align_y(Alignment::Center),
            row![
                toggler(self.config.enabled)
                    .label("Remapping enabled")
                    .on_toggle_maybe(running.then_some(Message::SetEnabled)),
                space::horizontal(),
                text("Active profile"),
                dropdown(profiles, Some(self.saved.active.clone()), Message::ActivateProfile).width(280),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ]
        .spacing(12);

        if !running {
            header = header.push(
                text("Start it with `systemctl --user start controller_app` or `controller_app daemon`. Edits are saved to the config file.")
                    .size(13)
                    .color(MUTED_COLOR),
            );
        }
        header.into()
    }

    /// The live controller drawing, labelled with `labels_from`'s mappings.
    fn view_live(&self, labels_from: Option<&Profile>) -> Element<'_, Message> {
        let caption = match (&self.live, &self.status) {
            (Some(live), _) => match live.gyro {
                Some([pitch, yaw, roll]) => {
                    let [pitch, yaw, roll] = [pitch, yaw, roll].map(|v| crate::monitor::signed(v, 0));
                    format!("{} · gyro pitch {pitch} yaw {yaw} roll {roll} °/s", live.device)
                }
                None => live.device.clone(),
            },
            (None, Some(_)) => "Press a button on a managed controller".into(),
            (None, None) => "Live input needs the daemon".into(),
        };
        let layers = self.status.as_ref().map(|s| s.active_layers.as_slice()).unwrap_or_default();
        let caption = if layers.is_empty() { caption } else { format!("{caption} · Layers: {}", layers.join(" + ")) };
        column![
            text("Live input").size(20),
            container(controller_drawing(self.live.as_ref(), labels_from)).center_x(Length::Fill),
            container(text(caption).size(13).color(MUTED_COLOR)).center_x(Length::Fill),
        ]
        .spacing(8)
        .into()
    }

    fn view_devices(&self) -> Element<'_, Message> {
        let mut list = column![text("Controllers").size(20)].spacing(8);
        match &self.status {
            Some(status) if !status.devices.is_empty() => {
                for d in &status.devices {
                    let state = if d.managed {
                        "remapping"
                    } else if d.ignored {
                        "ignored"
                    } else if !status.enabled {
                        "idle (disabled)"
                    } else {
                        "not grabbed"
                    };
                    let state = if d.analog_triggers { state.to_string() } else { format!("{state} · digital triggers") };
                    let state = if d.rumble { state } else { format!("{state} · no rumble") };
                    let state = if d.gyro { format!("{state} · gyro") } else { state };
                    let calibrate: Element<'_, Message> = if d.gyro {
                        button(text("Calibrate gyro").size(13))
                            .style(button::secondary)
                            .on_press(Message::CalibrateGyro(d.path.clone()))
                            .into()
                    } else {
                        space().into()
                    };
                    let name = d.name.clone();
                    let rumble: Element<'_, Message> = if d.rumble {
                        button(text("Test rumble").size(13))
                            .style(button::secondary)
                            .on_press(Message::TestRumble(d.path.clone()))
                            .into()
                    } else {
                        space().into()
                    };
                    list = list.push(
                        row![
                            column![text(&d.name), text(format!("{} · {state}", d.path)).size(12).color(MUTED_COLOR)]
                                .width(Length::Fill),
                            calibrate,
                            rumble,
                            checkbox(!d.ignored)
                                .label("Manage")
                                .on_toggle(move |on| Message::SetIgnored(name.clone(), !on)),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    );
                }
            }
            Some(_) => list = list.push(text("No controllers detected.").color(MUTED_COLOR)),
            None => list = list.push(text("Unavailable while the daemon is not running.").color(MUTED_COLOR)),
        }
        if let Some(status) = &self.status
            && !status.motion_access_denied.is_empty()
        {
            let names = status.motion_access_denied.join(", ");
            list = list.push(
                column![
                    text(format!(
                        "Gyro unavailable for {names}: no permission to read its motion sensors. \
                         Installing a udev rule (needs your password once) fixes this:"
                    ))
                    .size(12)
                    .color(ERROR_COLOR),
                    row![
                        text(motion_rule_command()).size(11).font(iced::Font::MONOSPACE).width(Length::Fill),
                        button(text("Copy command").size(13))
                            .style(button::secondary)
                            .on_press(Message::CopyMotionRuleCommand),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
            );
        }
        list.into()
    }

    fn view_auto_switch(&self) -> Element<'_, Message> {
        let auto = &self.config.auto_switch;
        let how = match self.status.as_ref().map(|s| s.focus_backend) {
            Some(FocusBackend::Kwin) => "Follows the focused window (KWin).",
            Some(FocusBackend::ProcessScan) => {
                "Focus tracking isn't available on this desktop, so rules apply while a matching \
                 game process is running."
            }
            None => "Needs the daemon to be running.",
        };
        let mut defaults = vec![DefaultChoice(None)];
        defaults.extend(
            self.config.all_games().flat_map(|(key, g)| g.profiles.iter().map(move |p| DefaultChoice(Some(ProfileRef::new(key, &p.name))))),
        );
        let default = DefaultChoice(auto.default_profile.clone());
        section(
            "Per-game profiles",
            Some("Each game's rules (on its Details tab) say which windows switch to which of its profiles.".into()),
            vec![
                row![
                    toggler(auto.enabled).label("Switch profiles automatically").on_toggle(Message::SetAutoSwitch),
                    space::horizontal(),
                    text("Otherwise use"),
                    dropdown(defaults, Some(default), Message::SetDefaultProfile).width(280),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
                text(how).size(13).color(MUTED_COLOR).into(),
            ],
        )
    }

    /// A game's name, rules, pack details, export and deletion.
    fn view_details(&self) -> Element<'_, Message> {
        let game = self.game();
        let names: Vec<String> = game.profiles.iter().map(|p| p.name.clone()).collect();
        let mut rules = column![].spacing(8);
        for (i, r) in game.rules.iter().enumerate() {
            let placeholder = match r.kind {
                RuleKind::Executable => "e.g. eldenring.exe",
                RuleKind::SteamAppId => "e.g. 1245620",
                RuleKind::WindowClass => "e.g. steam_app_1245620",
            };
            let mut line = row![
                checkbox(r.enabled).on_toggle(move |on| Message::SetRuleEnabled(i, on)),
                text("When").size(14),
                dropdown(RuleKind::ALL, Some(r.kind), move |k| Message::SetRuleKind(i, k)).width(150),
                text("is").size(14),
                field(placeholder, &r.value).on_input(move |v| Message::SetRuleValue(i, v)).width(200),
                text("use").size(14),
                dropdown(names.clone(), Some(r.profile.clone()), move |p| Message::SetRuleProfile(i, p)).width(160),
                button(text("✕").size(13)).style(button::secondary).on_press(Message::RemoveRule(i)),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            if !names.contains(&r.profile) {
                line = line.push(text("missing profile").size(12).color(ERROR_COLOR));
            }
            if let Some(other) = self.config.games.iter().find(|g| g.name != game.name && g.rules.iter().any(|o| o.enabled && o.same_match(r))) {
                line = line.push(text(format!("also in {}", other.name)).size(12).color(MUTED_COLOR));
            }
            rules = rules.push(line);
        }
        if game.rules.is_empty() {
            rules = rules.push(text("No rules: this game is only used when picked by hand.").size(13).color(MUTED_COLOR));
        }
        rules = rules.push(button(text("+ Add rule").size(13)).style(button::secondary).on_press(Message::AddRule(None)));
        let recent = self.status.as_ref().map(|s| s.recent_windows.as_slice()).unwrap_or_default();
        if !recent.is_empty() {
            let mut list = column![
                text("Recently focused (adds a rule for the profile being edited on the Profiles tab)").size(13).color(MUTED_COLOR)
            ]
            .spacing(6);
            for w in recent {
                let mut details = vec![w.exe.clone()];
                if let Some(id) = &w.steam_app_id {
                    details.push(format!("Steam {id}"));
                }
                if !w.class.is_empty() {
                    details.push(format!("class {}", w.class));
                }
                let title: String = w.title.chars().take(60).collect();
                list = list.push(
                    row![
                        column![text(title).size(14), text(details.join(" · ")).size(12).color(MUTED_COLOR)].width(Length::Fill),
                        button(text("+ Rule").size(13)).style(button::secondary).on_press(Message::AddRule(Some(w.clone()))),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
            rules = rules.push(list);
        }

        let mut pack_rows: Vec<Element<'_, Message>> = Vec::new();
        if let Some(origin) = &game.origin {
            let source = if origin.library { "the built-in library" } else { "a pack file" };
            let by = if game.pack.author.is_empty() { String::new() } else { format!(" by {}", game.pack.author) };
            pack_rows.push(text(format!("Added from {source}: version {}{by}.", origin.version)).into());
            if !self.edited.is_empty() {
                pack_rows.push(text(format!("Changed since: {}.", self.edited.join(", "))).size(13).color(MUTED_COLOR).into());
            }
            let update = library::updates(&self.config, &self.library).into_iter().find(|(n, _)| *n == game.name);
            if let Some(entry) = update.map(|(_, i)| &self.library[i]) {
                pack_rows.push(
                    row![
                        text(format!("The library has version {}.", entry.pack.pack.version)).color(style_accent()),
                        button(text("Update…").size(13)).on_press(Message::UpdateFromLibrary(game.name.clone())),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                );
            }
        } else if !game.pack.id.is_empty() {
            pack_rows.push(text(format!("You've exported this game (version {}).", game.pack.version)).into());
        } else {
            pack_rows.push(text("Export this game to share it as a .padpack file.").size(13).color(MUTED_COLOR).into());
        }
        if let Some(b) = &game.pack.based_on {
            pack_rows.push(text(format!("Based on {} {} by {}.", b.name, b.version, b.author)).size(13).color(MUTED_COLOR).into());
        }
        if !game.pack.description.is_empty() {
            pack_rows.push(text(game.pack.description.clone()).size(13).into());
        }
        pack_rows.push(
            row![
                button(text("Export…")).on_press(Message::OpenExport),
                space::horizontal(),
                button(text("Delete game")).style(button::danger).on_press(Message::AskDeleteGame),
            ]
            .spacing(8)
            .into(),
        );

        column![
            labeled(
                "Name",
                field("Game name", &game.name).on_input(Message::RenameGame).width(260).into(),
            ),
            section(
                "Auto-switch rules",
                Some("When the focused window (or a running process) matches a rule, its profile becomes active. Untick a rule to switch it off.".into()),
                vec![rules.into()],
            ),
            section("Pack", None, pack_rows),
        ]
        .spacing(16)
        .into()
    }

    fn view_macros(&self, names: &Names) -> Element<'_, Message> {
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
    fn view_macro_card<'a>(&'a self, mi: usize, m: &'a Macro, names: &Names) -> Element<'a, Message> {
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

    fn view_macro_editor<'a>(&'a self, mi: usize, m: &'a Macro, names: &Names) -> Element<'a, Message> {
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

    fn view_menus<'a>(&'a self, names: &Names, reachable: &[&Menu]) -> Element<'a, Message> {
        let mut col = column![view_new_menu_card()].spacing(16);
        for (i, menu) in self.menus().iter().enumerate() {
            col = col.push(self.view_menu_card(i, menu, names, reachable));
        }
        col.into()
    }

    fn view_infos(&self) -> Element<'_, Message> {
        let mut col = column![view_new_info_card()].spacing(16);
        for (i, o) in self.infos().iter().enumerate() {
            col = col.push(self.view_info_card(i, o));
        }
        col.into()
    }

    /// An info overlay as its own collapsible card.
    fn view_info_card<'a>(&'a self, i: usize, o: &'a InfoOverlay) -> Element<'a, Message> {
        let open = self.open_infos.contains(&i);
        let problem = self.name_problem(ItemKind::Info, i, &o.name);
        let chevron = if open { "▾" } else { "▸" };
        let title = text(format!("{chevron}  {}", if o.name.is_empty() { "(unnamed)" } else { &o.name })).size(18);
        let title = if problem.is_some() { title.color(ERROR_COLOR) } else { title };
        let rows = o.rows.len();
        let summary = format!(
            "{rows} row{} · {}",
            if rows == 1 { "" } else { "s" },
            if o.always { "always shown" } else { "shown by an action" }
        );
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

    fn view_info_editor<'a>(&'a self, i: usize, o: &'a InfoOverlay) -> Element<'a, Message> {
        let mut rows: Vec<Element<'a, Message>> = vec![
            labeled("Name", field("Info overlay name", &o.name).on_input(move |n| Message::RenameInfo(i, n)).width(240).into()),
            labeled(
                "Shown",
                column![
                    checkbox(o.always)
                        .label(if self.on_shared() { "Always, in every game" } else { "Always, while this game is active" })
                        .on_toggle(move |a| Message::SetInfoAlways(i, a)),
                    text(
                        "Or map \"Show info overlay…\" to a button: shown while held, or until pressed again \
                         if wrapped in Toggle."
                    )
                    .size(12)
                    .color(MUTED_COLOR),
                ]
                .spacing(4)
                .into(),
            ),
        ];

        // Appearance, and a live preview with the fallback glyphs.
        let appearance_open = self.open_info_appearance.contains(&i);
        rows.push(labeled("", disclosure("Appearance", appearance_open, Message::ToggleInfoAppearance(i))));
        if appearance_open {
            rows.push(style_editor(&o.style, Rc::new(move |s| Message::SetInfoStyle(i, s))));
        }
        let sample = InfoOverlay { style: preview_style(&o.style), ..o.clone() };
        let view = crate::info::resolve(&sample, &crate::info::Live::sample(self.config.info_glyphs));
        rows.push(preview(crate::overlay::draw::info_panel(&view)));

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

    fn view_keyboard_card(&self) -> Element<'_, Message> {
        let keyboard_open = self.status.as_ref().is_some_and(|s| s.overlay_visible);
        let appearance_open = self.open_appearance.contains(&None);
        let mut rows: Vec<Element<'_, Message>> = vec![
            row![
                button(text(if keyboard_open { "Close it" } else { "Open it now" }).size(14))
                    .style(button::secondary)
                    .on_press(Message::ToggleOverlay),
                disclosure("Appearance", appearance_open, Message::ToggleAppearance(None)),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
        ];
        if appearance_open {
            let style = &self.config.keyboard_style;
            rows.push(style_editor(style, Rc::new(Message::SetKeyboardStyle)));
            let sample = crate::overlay::KeyboardView {
                layout: Layout::Keyboard,
                style: preview_style(style),
                cursor: crate::keyboard::find(Layout::Keyboard, "KEY_H").unwrap_or_default(),
                latched: vec!["KEY_LEFTSHIFT".into()],
                pressed: None,
                closing: 0.0,
            };
            rows.push(preview(crate::overlay::draw::keyboard_panel(&sample)));
        }
        section(
            "On-screen keyboard",
            Some(
                "A keyboard over everything, typed with the controller: D-pad or stick to move, A to \
                 press, X backspace, Y space, Start enter, hold LT for Shift, hold B to close. Map \"On-screen keyboard\" \
                 to a button or gesture to open it from the controller."
                    .into(),
            ),
            rows,
        )
    }

    fn view_numpad_card(&self) -> Element<'_, Message> {
        let numpad_open = self.status.as_ref().is_some_and(|s| s.numpad_visible);
        let mut rows: Vec<Element<'_, Message>> = vec![
            row![
                button(text(if numpad_open { "Close it" } else { "Open it now" }).size(14))
                    .style(button::secondary)
                    .on_press(Message::ToggleNumpad),
                disclosure("Appearance", self.numpad_appearance, Message::ToggleNumpadAppearance),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
        ];
        if self.numpad_appearance {
            let style = &self.config.numpad_style;
            rows.push(style_editor(style, Rc::new(Message::SetNumpadStyle)));
            let sample = crate::overlay::KeyboardView {
                layout: Layout::Numpad,
                style: preview_style(style),
                cursor: Layout::Numpad.home(),
                latched: Vec::new(),
                pressed: None,
                closing: 0.0,
            };
            rows.push(preview(crate::overlay::draw::keyboard_panel(&sample)));
        }
        section(
            "On-screen numpad",
            Some(
                "Digits 0–9 and a dot, typed with the controller: D-pad or stick to move, A to press, \
                 X backspace, Start enter, hold B to close. Map \"On-screen numpad\" to a button or \
                 gesture to open it from the controller."
                    .into(),
            ),
            rows,
        )
    }

    /// A menu as its own collapsible card: a summary line, or the full editor when open.
    fn view_menu_card<'a>(&'a self, mi: usize, menu: &'a Menu, names: &Names, reachable: &[&Menu]) -> Element<'a, Message> {
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

    fn view_menu_editor<'a>(&'a self, mi: usize, menu: &'a Menu, names: &Names, reachable: &[&Menu]) -> Element<'a, Message> {
        let mut rows: Vec<Element<'a, Message>> = vec![labeled(
            "Name",
            field("Menu name", &menu.name).on_input(move |n| Message::RenameMenu(mi, n)).width(240).into(),
        )];

        // Kind and its settings.
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
            MenuKind::List | MenuKind::Buttons => {}
        }
        rows.push(labeled("Kind", kind_row.into()));

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
            rows.push(preview(crate::overlay::draw::menu_panel(&view)));
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
            items = items.push(button(text("+ Add item").size(13)).style(button::secondary).on_press(Message::AddMenuItem(mi)));
        } else {
            items = items.push(
                text("Give a direction \"Open menu…\" to open another directional menu.").size(12).color(MUTED_COLOR),
            );
        }
        rows.push(items.into());
        column(rows).spacing(10).into()
    }

    fn view_profile_bar(&self) -> Element<'_, Message> {
        let names: Vec<String> = self.game().profiles.iter().map(|p| p.name.clone()).collect();
        let current = self.profile().map(|p| p.name.clone());
        let can_delete = names.len() > 1;
        let active = self.profile_ref().is_some_and(|at| at == self.saved.active);
        let activate: Element<'_, Message> = match self.profile_ref() {
            Some(at) if !active && self.status.is_some() => {
                button(text("Make active").size(13)).style(button::secondary).on_press(Message::ActivateProfile(at)).into()
            }
            _ if active => text("● active").size(13).color(style_accent()).into(),
            _ => space().into(),
        };
        column![
            row![text("Edit profile").size(20), activate].spacing(12).align_y(Alignment::Center),
            row![
                dropdown(names, current.clone(), Message::EditProfile).width(170),
                field("Profile name", current.as_deref().unwrap_or(""))
                    .on_input(Message::RenameProfile)
                    .width(170),
                space::horizontal(),
                dropdown(Template::NEW, None::<Template>, Message::AddProfile)
                    .placeholder("+ From template…")
                    .width(200),
                button(text("Duplicate")).style(button::secondary).on_press(Message::AddProfile(Template::Duplicate)),
                button(text("Delete"))
                    .style(button::danger)
                    .on_press_maybe(can_delete.then_some(Message::DeleteProfile)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .into()
    }

    fn view_footer(&self) -> Element<'_, Message> {
        let dirty = self.config != self.saved;
        let msg: Element<'_, Message> = match &self.message {
            Some((m, is_err)) => text(m).color(if *is_err { ERROR_COLOR } else { MUTED_COLOR }).into(),
            None if dirty => text("Unsaved changes").color(MUTED_COLOR).into(),
            None => space().into(),
        };
        container(
            row![
                msg,
                space::horizontal(),
                button(text("Revert")).style(button::secondary).on_press_maybe(dirty.then_some(Message::Revert)),
                button(text("Save & apply")).on_press_maybe(dirty.then_some(Message::Save)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(12)
        .style(style::inset)
        .into()
    }
}

/// Follows `Multi` list indices down from `action`.
fn action_at<'a>(action: &'a mut ButtonAction, path: &[usize]) -> Option<&'a mut ButtonAction> {
    match path.split_first() {
        None => Some(action),
        Some((i, rest)) => match action {
            ButtonAction::Multi(list) => action_at(list.get_mut(*i)?, rest),
            // Toggle and Turbo wrap a single action, addressed as index 0.
            ButtonAction::Toggle(inner) | ButtonAction::Turbo { action: inner, .. } if *i == 0 => {
                action_at(inner, rest)
            }
            _ => None,
        },
    }
}

/// Modal on-screen keyboard over a dimmed backdrop; clicking the backdrop cancels.
fn view_picker(picker: &KeyPicker) -> Element<'_, Message> {
    let selection = if picker.keys.is_empty() {
        "Nothing selected".to_string()
    } else {
        picker.keys.iter().map(|k| keyboard::label(k)).collect::<Vec<_>>().join(" + ")
    };
    let (title, hint) = if picker.single {
        ("Pick a key", "Click a key to choose it.")
    } else {
        ("Pick keys", "Click keys to toggle them. Selected keys are pressed together, in the order chosen.")
    };
    let mut actions = row![
        button(text("Clear")).style(button::secondary).on_press(Message::PickerClear),
        space::horizontal(),
        button(text("Cancel")).style(button::secondary).on_press(Message::PickerClose { apply: false }),
    ]
    .spacing(8);
    if !picker.single {
        actions = actions.push(button(text("Done")).on_press(Message::PickerClose { apply: true }));
    }
    let dialog = container(
        column![
            text(title).size(20),
            text(hint).size(13).color(MUTED_COLOR),
            text(selection).size(16),
            keyboard::view(&picker.keys, Message::PickerKey),
            actions,
        ]
        .spacing(12),
    )
    .padding(20)
    .style(style::inset);

    let backdrop = container(center(opaque(dialog)))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Color { a: 0.6, ..Color::BLACK }.into()),
            ..container::Style::default()
        });
    opaque(mouse_area(backdrop).on_press(Message::PickerClose { apply: false }))
}

/// The motion-sensor udev rule shipped in dist/, built in so the command works from anywhere.
const MOTION_RULE_FILE: &str = include_str!("../dist/70-controller-app-motion.rules");

/// Shell command (bash or fish) that installs the motion-sensor udev rule and applies it.
fn motion_rule_command() -> String {
    let rules: Vec<&str> = MOTION_RULE_FILE
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    format!(
        "echo '{}' | sudo tee /etc/udev/rules.d/70-controller-app-motion.rules >/dev/null \
         && sudo udevadm control --reload && sudo udevadm trigger",
        rules.join("\n")
    )
}

/// Number of "Hold down" steps with no later matching "Release".
fn unreleased_holds(m: &Macro) -> usize {
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



/// A dropdown in the app's style: stands out from cards, with a raised menu.
fn dropdown<'a, T, L, V>(options: L, selected: Option<V>, on_select: impl Fn(T) -> Message + 'a) -> iced::widget::PickList<'a, T, L, V, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: Borrow<[T]> + 'a,
    V: Borrow<T> + 'a,
{
    pick_list(options, selected, on_select)
        .style(style::dropdown)
        .menu_style(style::dropdown_menu)
        .padding([6, 10])
}

/// An exact milliseconds entry beside a slider (sliders can't hit 17 ms on a 5 s range).
fn ms_field<'a>(ms: u64, on_change: impl Fn(u64) -> Message + 'a) -> Element<'a, Message> {
    row![
        field("ms", &ms.to_string())
            .on_input(move |s| {
                let digits: String = s.chars().filter(char::is_ascii_digit).take(6).collect();
                on_change(digits.parse().unwrap_or(0))
            })
            .width(80),
        text("ms").size(13),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// A text input in the app's style.
fn field<'a>(placeholder: &str, value: &str) -> iced::widget::TextInput<'a, Message> {
    text_input(placeholder, value).style(style::text_field).padding([6, 10])
}

/// Rule for a window using its most specific identifier: Steam App ID, then a Windows `.exe`
/// name, then the window class, then the native executable name.
fn rule_for_window(w: &WindowInfo, profile: String) -> Rule {
    let (kind, value) = if let Some(id) = &w.steam_app_id {
        (RuleKind::SteamAppId, id.clone())
    } else if w.exe.to_ascii_lowercase().ends_with(".exe") {
        (RuleKind::Executable, w.exe.clone())
    } else if !w.class.is_empty() {
        (RuleKind::WindowClass, w.class.clone())
    } else {
        (RuleKind::Executable, w.exe.clone())
    };
    Rule::new(kind, value, profile)
}

/// The controller SVG with button letters and mapping-label pills placed over it. (iced's
/// SVG renderer may not draw SVG text, so text is real widgets pinned at drawing
/// coordinates; the drawing is shown at 1:1.)
fn controller_drawing<'a>(input: Option<&InputSnapshot>, labels_from: Option<&Profile>) -> Element<'a, Message> {
    let labels = labels_from.map(drawing_labels).unwrap_or_default();
    let handle = svg::Handle::from_memory(pad_svg::render(input, &labels).into_bytes());
    let mut layers: Vec<Element<'a, Message>> =
        vec![svg(handle).width(pad_svg::WIDTH).height(pad_svg::HEIGHT).into()];

    for o in pad_svg::overlays(input) {
        let [r, g, b] = o.color;
        let letter = container(text(o.text).size(12).color(Color::from_rgb8(r, g, b)))
            .center_x(24)
            .center_y(20);
        layers.push(pin(letter).x(o.x - 12.0).y(o.y - 10.0).into());
    }
    for l in pad_svg::place_labels(&labels) {
        let pill = container(text(l.text).size(11).color(Color::from_rgb8(0xe6, 0xed, 0xf3)))
            .padding([2, 6])
            .style(|_: &iced::Theme| container::Style {
                background: Some(Color::from_rgb8(0x1d, 0x20, 0x26).into()),
                border: iced::Border { width: 1.0, radius: 5.0.into(), color: Color::from_rgb8(0x4e, 0xa1, 0xff) },
                ..container::Style::default()
            });
        // Each column is a box the width of the margin; pills hug its inner edge.
        let column = container(pill).width(pad_svg::LABEL_COLUMN);
        let (column, x) = if l.right {
            (column.align_x(iced::alignment::Horizontal::Left), pad_svg::RIGHT_COLUMN_X)
        } else {
            (column.align_x(iced::alignment::Horizontal::Right), 0.0)
        };
        layers.push(pin(column).x(x).y(l.y).into());
    }
    stack(layers).width(pad_svg::WIDTH).height(pad_svg::HEIGHT).into()
}

/// Labels for the controller drawing: every input that doesn't simply pass through.
fn drawing_labels(p: &Profile) -> Vec<(pad_svg::Spot, String)> {
    let mut labels = Vec::new();
    for b in Button::ALL {
        let passthrough = *p.button(b) == ButtonAction::Gamepad(b);
        let gestures = p.gestures(b).is_some();
        if passthrough && !gestures {
            continue;
        }
        let mut label = summarize(p.button(b));
        if gestures {
            label.push_str(" +");
        }
        labels.push((pad_svg::Spot::Button(b), label));
    }
    for t in [Trigger::Left, Trigger::Right] {
        let label = match p.trigger(t) {
            TriggerAction::Gamepad(out) if *out == t => continue,
            TriggerAction::Gamepad(out) => format!("Pad {}", if *out == Trigger::Left { "LT" } else { "RT" }),
            TriggerAction::Disabled => "—".into(),
            TriggerAction::Button { action, .. } => summarize(action),
        };
        labels.push((pad_svg::Spot::Trigger(t), label));
    }
    labels
}

/// Streams live input from the daemon, reconnecting every second while it is unavailable.
fn watch_input() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::{SinkExt, channel::mpsc};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    iced::stream::channel(16, async |mut output: mpsc::Sender<Message>| {
        let mut request = serde_json::to_string(&Request::WatchInput).unwrap();
        request.push('\n');
        loop {
            if let Ok(mut stream) = tokio::net::UnixStream::connect(ipc::socket_path()).await
                && stream.write_all(request.as_bytes()).await.is_ok()
            {
                let mut lines = BufReader::new(stream).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Ok(snapshot) = serde_json::from_str(&line) {
                        let _ = output.send(Message::LiveInput(snapshot)).await;
                    }
                }
            }
            let _ = output.send(Message::LiveInput(None)).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

fn section<'a>(title: &'a str, help_text: Option<String>, rows: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut heading = row![text(title).size(18)].spacing(8).align_y(Alignment::Center);
    if let Some(h) = help_text {
        heading = heading.push(help(h));
    }
    container(column![heading].extend(rows).spacing(10))
        .padding(14)
        .width(Length::Fill)
        .style(style::card)
        .into()
}

/// An ⓘ that explains a section on hover, instead of a paragraph of grey text.
fn help<'a>(explanation: String) -> Element<'a, Message> {
    tooltip(
        text("ⓘ").size(16).color(style_accent()),
        container(text(explanation).size(13)).padding(10).max_width(420.0).style(style::tooltip),
        tooltip::Position::Bottom,
    )
    .gap(6)
    .into()
}

/// The accent color, for hints that should catch the eye.
fn style_accent() -> Color {
    Color::from_rgb(0.35, 0.45, 0.95)
}

fn labeled<'a>(label: impl text::IntoFragment<'a>, editor: Element<'a, Message>) -> Element<'a, Message> {
    row![text(label).width(LABEL_WIDTH), editor]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}

fn view_profile<'a>(p: &'a Profile, ui: &Ui, tab: ProfileTab) -> Element<'a, Message> {
    let names = ui.names;
    let sections: Vec<Element<'a, Message>> = match tab {
        ProfileTab::Buttons => vec![section(
            "Buttons",
            Some(
                "Click a button's name to edit it. Add a double tap, triple tap or long press with \
                 \"+ Gesture\". Buttons with gestures act once the gesture is decided: a quick tap \
                 fires after the tap window; held past the tap window, the button's own action presses \
                 and holds until release (unless a long press is set, which takes over when held)."
                    .into(),
            ),
            button_rows(p, ui),
        )],
        ProfileTab::Sticks => {
            let mut sticks = Vec::new();
            for s in [Stick::Left, Stick::Right] {
                let base = ui.layer.map_or(p.stick(s), |m| m.base.stick(s));
                sticks.extend(layer_part(ui, LayerPart::Stick(s), s.to_string(), stick_summary(base), || vec![stick_editor(s, p.stick(s), names)]));
                for b in Button::stick_directions(s) {
                    sticks.extend(button_row(p, b, ui));
                }
            }
            let mut triggers = Vec::new();
            for t in [Trigger::Left, Trigger::Right] {
                let base = ui.layer.map_or(p, |m| m.base);
                let base = if t == Trigger::Left { &base.left_trigger } else { &base.right_trigger };
                triggers.extend(layer_part(ui, LayerPart::Trigger(t), t.to_string(), trigger_summary(base), || {
                    vec![trigger_editor(t, p.trigger(t), p.zones(Analog::Trigger(t)), ui.analog_triggers, names)]
                }));
            }
            vec![
                section(
                    "Sticks",
                    Some(
                        "Each stick's directions (Left Stick Up, …) also act as buttons on top of the \
                         stick's mode: give them actions or gestures, or use them in combos \
                         (e.g. LB + Right Stick Right). They press at the stick's \"Directions press at\" \
                         threshold."
                            .into(),
                    ),
                    sticks,
                ),
                section("Triggers", None, triggers),
            ]
        }
        ProfileTab::Combos => {
            let mut sections = vec![section(
                if ui.layer.is_some() { "This layer's combos" } else { "Combos" },
                Some(format!(
                    "Buttons pressed within the window act as one input. Combo buttons wait up to {} ms \
                     before acting alone; a combo button set to Disabled works as a modifier with no \
                     time limit.",
                    p.combo_window_ms
                )),
                combo_rows(p, ui),
            )];
            if let Some(marks) = ui.layer {
                sections.push(base_combos(marks));
            }
            sections
        }
        ProfileTab::Gyro => {
            let base = ui.layer.map_or(&p.gyro, |m| &m.base.gyro);
            let summary = match base.mode {
                GyroMode::Off => "off".to_string(),
                _ => "on".to_string(),
            };
            vec![section(
                "Gyro",
                Some("Uses the controller's motion sensors. Calibrate it from the controller list on the Overview tab if the aim drifts.".into()),
                layer_part(ui, LayerPart::Gyro, "Gyro".into(), summary, || gyro_rows(&p.gyro, ui.any_gyro)),
            )]
        }
    };
    column(sections).spacing(16).into()
}

/// The compared profile's combos, each of which a layer can switch off while it's on.
fn base_combos<'a>(marks: LayerMarks<'_>) -> Element<'a, Message> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    for combo in &marks.base.combos {
        let key = crate::config::combo_key(&combo.buttons);
        let off = marks.layer.disabled_combos.iter().any(|d| crate::config::combo_key(d) == key);
        let replaced = marks.layer.combos.iter().any(|c| crate::config::combo_key(&c.buttons) == key);
        let name = combo.buttons.iter().map(|b| short_button(*b)).collect::<Vec<_>>().join(" + ");
        let mut line = row![text(name).width(LABEL_WIDTH), text(summarize(&combo.action)).color(MUTED_COLOR), space::horizontal()]
            .spacing(10)
            .align_y(Alignment::Center);
        line = if replaced {
            line.push(text("replaced by this layer's").size(12).color(MUTED_COLOR))
        } else {
            line.push(checkbox(off).label("Off in this layer").on_toggle(move |_| Message::ToggleBaseCombo(key.clone())))
        };
        rows.push(line.into());
    }
    if rows.is_empty() {
        rows.push(text(format!("{} has no combos.", marks.base.name)).size(13).color(MUTED_COLOR).into());
    }
    section(
        "The profile's combos",
        Some("They stay on while the layer is held, unless switched off here or replaced by a layer combo with the same buttons.".into()),
        rows,
    )
}

impl ProfileTab {
    const ALL: [ProfileTab; 4] = [ProfileTab::Buttons, ProfileTab::Sticks, ProfileTab::Combos, ProfileTab::Gyro];
}

impl fmt::Display for ProfileTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ProfileTab::Buttons => "Buttons",
            ProfileTab::Sticks => "Sticks & triggers",
            ProfileTab::Combos => "Combos",
            ProfileTab::Gyro => "Gyro",
        })
    }
}

/// One-line description of an action, for collapsed rows and the controller drawing.
fn summarize(action: &ButtonAction) -> String {
    match action {
        ButtonAction::Disabled => "—".into(),
        ButtonAction::Gamepad(b) => format!("Pad {}", short_button(*b)),
        ButtonAction::Keys(keys) if keys.is_empty() => "(no key)".into(),
        ButtonAction::Keys(keys) => keys.iter().map(|k| keyboard::label(k)).collect::<Vec<_>>().join(" + "),
        ButtonAction::Mouse(m) => format!("{m} click"),
        ButtonAction::Wheel(d) => d.to_string(),
        ButtonAction::NextProfile => "Next profile".into(),
        ButtonAction::ToggleOverlay => "On-screen keyboard".into(),
        ButtonAction::ToggleNumpad => "On-screen numpad".into(),
        ButtonAction::OpenMenu(name) => format!("Menu “{name}”"),
        ButtonAction::ShowInfo(name) => format!("Info “{name}”"),
        ButtonAction::Multi(list) if list.is_empty() => "(nothing)".into(),
        ButtonAction::Multi(list) => list.iter().map(summarize).collect::<Vec<_>>().join(" & "),
        ButtonAction::Toggle(inner) => format!("Toggle {}", summarize(inner)),
        ButtonAction::Turbo { action, rate } => format!("Turbo {} ({rate:.0}/s)", summarize(action)),
        ButtonAction::Macro { name, repeat: true } => format!("Macro “{name}” (repeat)"),
        ButtonAction::Macro { name, .. } => format!("Macro “{name}”"),
        ButtonAction::Layer(name) => format!("Layer “{name}”"),
    }
}

/// What's wrong with an action, if anything (the same things saving checks).
fn action_problem(action: &ButtonAction, names: &Names) -> Option<String> {
    let mut problem = None;
    action.walk(&mut |a| {
        if problem.is_some() {
            return;
        }
        match a {
            ButtonAction::Keys(keys) => {
                if let Some(bad) = keys.iter().find(|k| KeyCode::from_str(k).is_err()) {
                    problem = Some(format!("unknown key {:?}", short_key(bad)));
                }
            }
            ButtonAction::Macro { name, .. } if !names.macros.contains(name) => {
                problem = Some(format!("missing macro {name:?}"));
            }
            ButtonAction::OpenMenu(name) if !names.menus.contains(name) => {
                problem = Some(format!("missing menu {name:?}"));
            }
            ButtonAction::ShowInfo(name) if !names.infos.contains(name) => {
                problem = Some(format!("missing info overlay {name:?}"));
            }
            ButtonAction::Layer(_) if !names.layers_allowed => {
                problem = Some("shared items can't use layers".into());
            }
            ButtonAction::Layer(name) if !names.layers.contains(name) => {
                problem = Some(format!("missing layer {name:?}"));
            }
            _ => {}
        }
    });
    problem
}

/// Whether a profile section contains anything saving would reject.
fn section_has_problem(p: &Profile, tab: ProfileTab, names: &Names) -> bool {
    let bad = |a: &ButtonAction| action_problem(a, names).is_some();
    let button_bad = |b: Button| {
        bad(p.button(b)) || p.gestures.get(&b).is_some_and(|g| GestureKind::ALL.iter().any(|k| g.get(*k).is_some_and(bad)))
    };
    match tab {
        ProfileTab::Buttons => Button::ALL.into_iter().any(button_bad),
        ProfileTab::Sticks => {
            let dirs = [Stick::Left, Stick::Right].into_iter().flat_map(Button::stick_directions).any(button_bad);
            let zones = [Analog::Stick(Stick::Left), Analog::Stick(Stick::Right), Analog::Trigger(Trigger::Left), Analog::Trigger(Trigger::Right)]
                .into_iter()
                .any(|a| p.zones(a).iter().any(|z| z.min >= z.max || bad(&z.action)));
            let triggers = [Trigger::Left, Trigger::Right]
                .into_iter()
                .any(|t| matches!(p.trigger(t), TriggerAction::Button { action, .. } if bad(action)));
            let keys = [Stick::Left, Stick::Right].into_iter().any(|s| match &p.stick(s).action {
                StickAction::Keys { up, down, left, right } => [up, down, left, right].iter().any(|k| KeyCode::from_str(k).is_err()),
                _ => false,
            });
            dirs || zones || triggers || keys
        }
        ProfileTab::Combos => p.combos.iter().any(|c| c.buttons.len() < 2 || bad(&c.action)),
        ProfileTab::Gyro => false,
    }
}

fn macros_have_problem(macros: &[Macro], names: &Names) -> bool {
    macros.iter().enumerate().any(|(i, m)| {
        m.name.trim().is_empty()
            || macros[..i].iter().any(|o| o.name == m.name)
            || m.steps.iter().filter_map(MacroStep::action).any(|a| action_problem(a, names).is_some())
    })
}

/// The menus a game's items can open: its own, then shared ones (only shared ones for
/// `None`, the shared items themselves).
fn reachable_menus<'a>(config: &'a Config, game: Option<&'a Game>) -> Vec<&'a Menu> {
    config.scope_of(game).menus
}

/// The first thing saving would reject in a list of macros, menus and info overlays.
/// `names` is what they may refer to; `shared` holds the shared items' names, which a game's
/// items may not reuse.
fn items_problem(macros: &[Macro], menus: &[Menu], info: &[InfoOverlay], names: &Names, reachable: &[&Menu], shared: Option<&Names>) -> Option<String> {
    let clash = |kind: ItemKind, name: &str| shared.is_some_and(|s| s.list(kind).iter().any(|n| n == name));
    for (i, m) in macros.iter().enumerate() {
        if m.name.trim().is_empty() {
            return Some("Macro names cannot be empty.".into());
        }
        if macros[..i].iter().any(|o| o.name == m.name) {
            return Some(format!("Two macros are named {:?}.", m.name));
        }
        if clash(ItemKind::Macro, &m.name) {
            return Some(format!("Macro {:?} has the same name as a shared macro.", m.name));
        }
        let keys = m.steps.iter().filter_map(MacroStep::action).flat_map(|a| a.key_names());
        if let Some(bad) = keys.into_iter().find(|k| KeyCode::from_str(k).is_err()) {
            return Some(format!("Macro {:?}: unknown key {:?}", m.name, short_key(bad)));
        }
        if m.steps.iter().filter_map(MacroStep::action).any(|a| {
            let mut layer = false;
            a.walk(&mut |a| layer |= matches!(a, ButtonAction::Layer(_)));
            layer
        }) {
            return Some(format!("Macro {:?}: macros can't hold layers.", m.name));
        }
        if let Some(problem) = m.steps.iter().filter_map(MacroStep::action).find_map(|a| action_problem(a, names)) {
            return Some(format!("Macro {:?}: {problem}", m.name));
        }
    }
    for (i, m) in menus.iter().enumerate() {
        if m.name.trim().is_empty() {
            return Some("Menu names cannot be empty.".into());
        }
        if menus[..i].iter().any(|o| o.name == m.name) {
            return Some(format!("Two menus are named {:?}.", m.name));
        }
        if clash(ItemKind::Menu, &m.name) {
            return Some(format!("Menu {:?} has the same name as a shared menu.", m.name));
        }
        if let Some(problem) = m.items.iter().find_map(|item| item_problem(m, &item.action, reachable, names)) {
            return Some(format!("Menu {:?}: {problem}", m.name));
        }
    }
    for (i, o) in info.iter().enumerate() {
        if o.name.trim().is_empty() {
            return Some("Info overlay names cannot be empty.".into());
        }
        if info[..i].iter().any(|other| other.name == o.name) {
            return Some(format!("Two info overlays are named {:?}.", o.name));
        }
        if clash(ItemKind::Info, &o.name) {
            return Some(format!("Info overlay {:?} has the same name as a shared one.", o.name));
        }
    }
    None
}

/// The first thing saving would reject in a game (or General).
fn game_problem(config: &Config, g: &Game) -> Option<String> {
    if g.profiles.is_empty() {
        return Some("needs at least one profile.".into());
    }
    let names = Names::for_game(config, g);
    let reachable = reachable_menus(config, Some(g));
    if let Some(problem) = items_problem(&g.macros, &g.menus, &g.info, &names, &reachable, Some(&Names::shared(config))) {
        return Some(problem);
    }
    for (i, p) in g.profiles.iter().enumerate() {
        if p.name.trim().is_empty() {
            return Some("Profile names cannot be empty.".into());
        }
        if g.profiles[..i].iter().any(|o| o.name == p.name) {
            return Some(format!("Two profiles are named {:?}.", p.name));
        }
        if let Some(problem) = p.actions().into_iter().find_map(|a| {
            action_problem(a, &names).filter(|problem| !problem.starts_with("unknown key"))
        }) {
            return Some(format!("Profile {:?}: {problem}.", p.name));
        }
        if let Some(c) = p.combos.iter().find(|c| c.buttons.len() < 2) {
            return Some(format!("Profile {:?}: a combo needs at least two buttons (has {}).", p.name, c.buttons.len()));
        }
        let analogs = [
            Analog::Stick(Stick::Left),
            Analog::Stick(Stick::Right),
            Analog::Trigger(Trigger::Left),
            Analog::Trigger(Trigger::Right),
        ];
        if analogs.into_iter().any(|a| p.zones(a).iter().any(|z| z.min >= z.max)) {
            return Some(format!("Profile {:?}: a zone's range must start below where it ends.", p.name));
        }
        let mut keys: Vec<&String> = p.actions().into_iter().flat_map(|a| a.key_names()).collect();
        for s in [Stick::Left, Stick::Right] {
            if let StickAction::Keys { up, down, left, right } = &p.stick(s).action {
                keys.extend([up, down, left, right]);
            }
        }
        if let Some(bad) = keys.iter().find(|k| KeyCode::from_str(k).is_err()) {
            return Some(format!("Profile {:?}: unknown key {:?}", p.name, short_key(bad)));
        }
    }
    layers_problem(g, &names).or_else(|| rules_problem(g))
}

/// The first thing saving would reject in a game's layers.
fn layers_problem(g: &Game, names: &Names) -> Option<String> {
    use crate::config::Indicator;
    for (i, l) in g.layers.iter().enumerate() {
        if l.name.trim().is_empty() {
            return Some("Layer names cannot be empty.".into());
        }
        if g.layers[..i].iter().any(|o| o.name == l.name) {
            return Some(format!("Two layers are named {:?}.", l.name));
        }
        let label = format!("Layer {:?}", l.name);
        if let Indicator::Info(info) = &l.indicator
            && !names.infos.contains(info)
        {
            return Some(format!("{label}: missing info overlay {info:?} to show."));
        }
        if let Some(problem) = l.actions().into_iter().find_map(|a| action_problem(a, names)) {
            return Some(format!("{label}: {problem}."));
        }
        if let Some(c) = l.combos.iter().find(|c| c.buttons.len() < 2) {
            return Some(format!("{label}: a combo needs at least two buttons (has {}).", c.buttons.len()));
        }
        let zones = [&l.left_stick, &l.right_stick].into_iter().flatten().flat_map(|s| &s.zones);
        let zones = zones.chain([&l.left_trigger, &l.right_trigger].into_iter().flatten().flat_map(|t| &t.zones));
        if zones.into_iter().any(|z| z.min >= z.max) {
            return Some(format!("{label}: a zone's range must start below where it ends."));
        }
        let stick_keys = [&l.left_stick, &l.right_stick].into_iter().flatten().flat_map(|s| match &s.action {
            StickAction::Keys { up, down, left, right } => vec![up, down, left, right],
            _ => Vec::new(),
        });
        if let Some(bad) = stick_keys.into_iter().find(|k| KeyCode::from_str(k).is_err()) {
            return Some(format!("{label}: unknown key {:?}", short_key(bad)));
        }
    }
    None
}

/// What's wrong with a game's auto-switch rules, if anything.
fn rules_problem(g: &Game) -> Option<String> {
    if let Some(r) = g.rules.iter().find(|r| r.value.trim().is_empty()) {
        return Some(format!("A per-game rule for profile {:?} has no {} to match.", r.profile, r.kind));
    }
    if let Some(r) = g.rules.iter().find(|r| g.profile(&r.profile).is_none()) {
        return Some(format!("A per-game rule points to missing profile {:?}.", r.profile));
    }
    None
}

/// A button pressed (or stick pushed firmly) in `now` that wasn't in `before`, for
/// "Find by pressing".
fn newly_pressed(before: Option<&InputSnapshot>, now: &InputSnapshot) -> Option<Button> {
    let was_down = |b: &Button| before.is_some_and(|s| s.buttons.contains(b));
    if let Some(b) = now.buttons.iter().find(|b| !was_down(b)) {
        return Some(*b);
    }
    const FIRM: f32 = 0.7;
    let pushed = |s: &InputSnapshot, stick: Stick| -> Option<Button> {
        let (x, y) = if stick == Stick::Left { s.left_stick } else { s.right_stick };
        let [up, down, left, right] = Button::stick_directions(stick);
        [(up, -y), (down, y), (left, -x), (right, x)].into_iter().find(|(_, v)| *v > FIRM).map(|(b, _)| b)
    };
    [Stick::Left, Stick::Right]
        .into_iter()
        .find_map(|stick| pushed(now, stick).filter(|b| before.and_then(|s| pushed(s, stick)) != Some(*b)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GyroModeKind {
    Off,
    Mouse,
    Stick,
    Steering,
}

impl fmt::Display for GyroModeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            GyroModeKind::Off => "Off",
            GyroModeKind::Mouse => "Mouse (gyro aiming)",
            GyroModeKind::Stick => "Gamepad stick (gyro aiming)",
            GyroModeKind::Steering => "Steering (tilt to steer)",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivationKind {
    Always,
    WhileHeld,
    UnlessHeld,
    Toggle,
}

impl fmt::Display for ActivationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ActivationKind::Always => "Always on",
            ActivationKind::WhileHeld => "Only while holding",
            ActivationKind::UnlessHeld => "Off while holding (clutch)",
            ActivationKind::Toggle => "Toggle with",
        })
    }
}

/// A recenter choice for a pick list, where `None` means no recenter input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RecenterChoice(Option<GyroInput>);

impl fmt::Display for RecenterChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(input) => write!(f, "{input}"),
            None => f.write_str("(none)"),
        }
    }
}

fn gyro_rows(cfg: &GyroConfig, any_gyro: bool) -> Vec<Element<'_, Message>> {
    let with = move |f: &dyn Fn(&mut GyroConfig)| {
        let mut c = cfg.clone();
        f(&mut c);
        Message::SetGyro(c)
    };
    // Without a gyro controller this is a status worth seeing, not just help.
    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    if !any_gyro {
        rows.push(
            text(
                "None of your managed controllers report a gyro (PlayStation and Switch controllers do). \
                 These settings apply once one is connected.",
            )
            .size(13)
            .color(MUTED_COLOR)
            .into(),
        );
    }

    let kind = match cfg.mode {
        GyroMode::Off => GyroModeKind::Off,
        GyroMode::Mouse { .. } => GyroModeKind::Mouse,
        GyroMode::Stick { .. } => GyroModeKind::Stick,
        GyroMode::Steering { .. } => GyroModeKind::Steering,
    };
    let kinds = [GyroModeKind::Off, GyroModeKind::Mouse, GyroModeKind::Stick, GyroModeKind::Steering];
    rows.push(labeled(
        "Gyro",
        dropdown(kinds, Some(kind), move |k| {
            let mode = match k {
                GyroModeKind::Off => GyroMode::Off,
                GyroModeKind::Mouse => GyroMode::Mouse { sensitivity: 15.0 },
                GyroModeKind::Stick => GyroMode::Stick { stick: Stick::Right, full_rate: 360.0, anti_deadzone: 0.15 },
                GyroModeKind::Steering => GyroMode::Steering { stick: Stick::Left, max_angle: 45.0 },
            };
            with(&|c| c.mode = mode.clone())
        })
        .width(260)
        .into(),
    ));

    match cfg.mode {
        GyroMode::Off => return rows,
        GyroMode::Mouse { sensitivity } => {
            rows.push(value_slider("    Sensitivity", 2.0..=60.0, sensitivity, 1.0, "px per degree", move |v| {
                with(&|c| c.mode = GyroMode::Mouse { sensitivity: v })
            }));
        }
        GyroMode::Stick { stick, full_rate, anti_deadzone } => {
            rows.push(labeled(
                "    Output stick",
                dropdown([Stick::Left, Stick::Right], Some(stick), move |s| {
                    with(&|c| c.mode = GyroMode::Stick { stick: s, full_rate, anti_deadzone })
                })
                .width(170)
                .into(),
            ));
            rows.push(value_slider("    Full tilt at", 60.0..=720.0, full_rate, 10.0, "°/s turning", move |v| {
                with(&|c| c.mode = GyroMode::Stick { stick, full_rate: v, anti_deadzone })
            }));
            rows.push(value_slider("    Anti-deadzone", 0.0..=0.4, anti_deadzone, 0.01, "(beats the game's stick deadzone)", move |v| {
                with(&|c| c.mode = GyroMode::Stick { stick, full_rate, anti_deadzone: v })
            }));
        }
        GyroMode::Steering { stick, max_angle } => {
            rows.push(labeled(
                "    Output stick",
                dropdown([Stick::Left, Stick::Right], Some(stick), move |s| {
                    with(&|c| c.mode = GyroMode::Steering { stick: s, max_angle })
                })
                .width(170)
                .into(),
            ));
            rows.push(value_slider("    Full lock at", 15.0..=90.0, max_angle, 1.0, "° tilt", move |v| {
                with(&|c| c.mode = GyroMode::Steering { stick, max_angle: v })
            }));
        }
    }

    let steering = matches!(cfg.mode, GyroMode::Steering { .. });
    if !steering {
        rows.push(labeled(
            "    Horizontal from",
            dropdown(GyroHorizontal::ALL, Some(cfg.horizontal), move |h| with(&|c| c.horizontal = h))
                .width(170)
                .into(),
        ));
    }
    let mut inverts = row![checkbox(cfg.invert_x).label("Invert horizontal").on_toggle(move |v| with(&|c| c.invert_x = v))]
        .spacing(16);
    if !steering {
        inverts = inverts.push(checkbox(cfg.invert_y).label("Invert vertical").on_toggle(move |v| with(&|c| c.invert_y = v)));
    }
    rows.push(labeled("", inverts.into()));

    let (activation_kind, activation_input) = match cfg.activation {
        GyroActivation::Always => (ActivationKind::Always, None),
        GyroActivation::WhileHeld(i) => (ActivationKind::WhileHeld, Some(i)),
        GyroActivation::UnlessHeld(i) => (ActivationKind::UnlessHeld, Some(i)),
        GyroActivation::Toggle(i) => (ActivationKind::Toggle, Some(i)),
    };
    let activation = move |kind: ActivationKind, input: GyroInput| match kind {
        ActivationKind::Always => GyroActivation::Always,
        ActivationKind::WhileHeld => GyroActivation::WhileHeld(input),
        ActivationKind::UnlessHeld => GyroActivation::UnlessHeld(input),
        ActivationKind::Toggle => GyroActivation::Toggle(input),
    };
    let current_input = activation_input.unwrap_or(GyroInput::LeftTrigger);
    let activation_kinds = [ActivationKind::Always, ActivationKind::WhileHeld, ActivationKind::UnlessHeld, ActivationKind::Toggle];
    let mut active_row = row![dropdown(activation_kinds, Some(activation_kind), move |k| {
        with(&|c| c.activation = activation(k, current_input))
    })
    .width(220)]
    .spacing(8)
    .align_y(Alignment::Center);
    if activation_input.is_some() {
        active_row = active_row.push(
            dropdown(GyroInput::all(), activation_input, move |i| with(&|c| c.activation = activation(activation_kind, i)))
                .width(200),
        );
    }
    rows.push(labeled("    Active", active_row.into()));

    let mut recenter_choices = vec![RecenterChoice(None)];
    recenter_choices.extend(GyroInput::all().into_iter().map(|i| RecenterChoice(Some(i))));
    rows.push(labeled(
        "    Recenter with",
        row![
            dropdown(recenter_choices, Some(RecenterChoice(cfg.recenter)), move |r: RecenterChoice| {
                with(&|c| c.recenter = r.0)
            })
            .width(200),
            text(if steering { "sets the current tilt as straight" } else { "clears leftover motion" })
                .size(12)
                .color(MUTED_COLOR),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
    ));
    if !steering {
        rows.push(value_slider("    Ignore jitter below", 0.0..=5.0, cfg.noise_threshold, 0.1, "°/s", move |v| {
            with(&|c| c.noise_threshold = v)
        }));
    }
    rows
}

fn button_rows<'a>(p: &'a Profile, ui: &Ui) -> Vec<Element<'a, Message>> {
    // Layers use their profile's timings.
    let mut rows: Vec<Element<'a, Message>> = if ui.layer.is_some() {
        Vec::new()
    } else {
        vec![
            value_slider("Tap window", 100.0..=600.0, p.tap_window_ms as f32, 10.0, "ms", Message::SetTapWindow),
            value_slider("Long press after", 200.0..=1500.0, p.long_press_ms as f32, 50.0, "ms", Message::SetLongPress),
        ]
    };
    for b in Button::ALL {
        rows.extend(button_row(p, b, ui));
    }
    rows
}

/// One button's action editor with its "+ Gesture" picker and any gesture rows.
/// The label column of a collapsible row: click to open or close it.
fn row_toggle<'a>(label: String, target: Target, open: bool, problem: bool) -> Element<'a, Message> {
    let chevron = if open { "▾" } else { "▸" };
    let label = text(format!("{chevron} {label}"));
    let label = if problem { label.color(ERROR_COLOR) } else { label };
    button(label)
        .style(button::text)
        .padding([4, 0])
        .width(LABEL_WIDTH)
        .on_press(Message::ToggleExpanded(target))
        .into()
}

/// One button: collapsed to a summary ("A ▸ E · double tap: Q"), or open with its action
/// editor, "+ Gesture" picker and gesture rows.
fn button_row<'a>(p: &'a Profile, b: Button, ui: &Ui) -> Vec<Element<'a, Message>> {
    if let Some(marks) = ui.layer
        && !marks.overrides(LayerPart::Button(b))
    {
        let mut summary = summarize(marks.base.button(b));
        if marks.base.gestures(b).is_some() {
            summary.push_str(" +");
        }
        return layer_part(ui, LayerPart::Button(b), b.to_string(), summary, Vec::new);
    }
    let target = Target::Button(b);
    let open = ui.is_open(target);
    let gestures = p.gestures.get(&b);
    let set_gestures: Vec<(GestureKind, &'a ButtonAction)> =
        GestureKind::ALL.into_iter().filter_map(|k| gestures.and_then(|g| g.get(k)).map(|a| (k, a))).collect();
    let problem = std::iter::once(p.button(b))
        .chain(set_gestures.iter().map(|(_, a)| *a))
        .find_map(|a| action_problem(a, ui.names));
    let mut rows: Vec<Element<'a, Message>> = Vec::new();

    if !open {
        let mut summary = summarize(p.button(b));
        for (k, a) in &set_gestures {
            summary.push_str(&format!("  ·  {k}: {}", summarize(a)));
        }
        let disabled = matches!(p.button(b), ButtonAction::Disabled) && set_gestures.is_empty();
        let summary = text(summary).color_maybe(disabled.then_some(MUTED_COLOR));
        let mut line = row![row_toggle(b.to_string(), target, false, problem.is_some()), summary]
            .spacing(10)
            .align_y(Alignment::Center);
        if let Some(problem) = problem {
            line = line.push(space::horizontal()).push(text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR));
        }
        if ui.layer.is_some() {
            line = line.push(space::horizontal()).push(
                button(text("Back to base").size(13)).style(button::text).on_press(Message::RevertInput(LayerPart::Button(b))),
            );
        }
        rows.push(line.into());
        return rows;
    }

    let missing: Vec<GestureKind> = GestureKind::ALL
        .into_iter()
        .filter(|k| gestures.and_then(|g| g.get(*k)).is_none())
        .collect();
    // The name gets a line of its own so the open editor reads as sitting under it.
    let mut header = row![row_toggle(b.to_string(), target, true, problem.is_some()), space::horizontal()]
        .spacing(10)
        .align_y(Alignment::Center);
    if !missing.is_empty() {
        header = header.push(
            dropdown(missing, None::<GestureKind>, move |k| Message::AddGesture(b, k))
                .placeholder("+ Gesture")
                .width(130),
        );
    }
    if ui.layer.is_some() {
        header = header.push(
            button(text("Back to base").size(13)).style(button::text).on_press(Message::RevertInput(LayerPart::Button(b))),
        );
    }
    rows.push(header.into());
    rows.push(labeled(
        "    Press",
        action_editor(p.button(b), b, &ACTION_KINDS, set_action(target), KeyField::root(target), ui.names),
    ));

    for (kind, action) in set_gestures {
        rows.push(labeled(
            format!("    {kind}"),
            row![
                action_editor(action, b, &ACTION_KINDS, set_action(Target::Gesture(b, kind)), KeyField::root(Target::Gesture(b, kind)), ui.names),
                button(text("✕").size(13))
                    .style(button::secondary)
                    .on_press(Message::RemoveGesture(b, kind)),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        ));
    }
    if let Some(problem) = problem {
        rows.push(labeled("", text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR).into()));
    }
    // An open row sits in a framed box, picked out in the accent color when it was just found.
    let border = if ui.found == Some(b) { style_accent() } else { Color::TRANSPARENT };
    vec![container(column(rows).spacing(8)).padding(6).style(style::highlight(border)).into()]
}

fn combo_rows<'a>(p: &'a Profile, ui: &Ui) -> Vec<Element<'a, Message>> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    if ui.layer.is_none() {
        rows.push(value_slider("Combo window", 20.0..=300.0, p.combo_window_ms as f32, 5.0, "ms", Message::SetComboWindow));
    }
    for (i, combo) in p.combos.iter().enumerate() {
        let target = Target::Combo(i);
        let open = ui.is_open(target);
        let name = combo.buttons.iter().map(|b| short_button(*b)).collect::<Vec<_>>().join(" + ");
        let name = if name.is_empty() { "(no buttons)".to_string() } else { name };
        let problem = if combo.buttons.len() < 2 {
            Some("needs at least two buttons".to_string())
        } else {
            action_problem(&combo.action, ui.names)
        };
        if !open {
            let mut line = row![row_toggle(name, target, false, problem.is_some()), text(summarize(&combo.action))]
                .spacing(10)
                .align_y(Alignment::Center);
            if let Some(problem) = problem {
                line = line.push(space::horizontal()).push(text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR));
            }
            rows.push(line.into());
            continue;
        }
        let mut members = row![].spacing(6).align_y(Alignment::Center);
        for (n, &b) in combo.buttons.iter().enumerate() {
            if n > 0 {
                members = members.push(text("+"));
            }
            members = members.push(
                button(text(format!("{} ✕", short_button(b))).size(13))
                    .style(button::secondary)
                    .on_press(Message::RemoveComboButton(i, b)),
            );
        }
        let remaining: Vec<Button> =
            Button::EVERY.into_iter().filter(|b| !combo.buttons.contains(b)).collect();
        members = members.push(
            dropdown(remaining, None::<Button>, move |b| Message::AddComboButton(i, b))
                .placeholder("Add button…")
                .width(170),
        );
        let mut body = column![
            row![
                row_toggle(name, target, true, problem.is_some()),
                members,
                space::horizontal(),
                button(text("Remove combo").size(13))
                    .style(button::danger)
                    .on_press(Message::RemoveCombo(i)),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            labeled("    Action", action_editor(&combo.action, Button::South, &ACTION_KINDS, set_action(target), KeyField::root(target), ui.names)),
        ]
        .spacing(8);
        if let Some(problem) = problem {
            body = body.push(labeled("", text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR).into()));
        }
        rows.push(container(body).padding(10).style(style::inset).into());
    }
    rows.push(button(text("+ Add combo")).style(button::secondary).on_press(Message::AddCombo).into());
    rows
}

fn short_button(b: Button) -> &'static str {
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
        Button::LeftStick => "LS",
        Button::RightStick => "RS",
        Button::DpadUp => "Up",
        Button::DpadDown => "Down",
        Button::DpadLeft => "Left",
        Button::DpadRight => "Right",
        Button::LeftStickUp => "LS↑",
        Button::LeftStickDown => "LS↓",
        Button::LeftStickLeft => "LS←",
        Button::LeftStickRight => "LS→",
        Button::RightStickUp => "RS↑",
        Button::RightStickDown => "RS↓",
        Button::RightStickLeft => "RS←",
        Button::RightStickRight => "RS→",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionKind {
    Disabled,
    Gamepad,
    Keys,
    Mouse,
    Wheel,
    NextProfile,
    Overlay,
    Numpad,
    Toggle,
    Turbo,
    Macro,
    Menu,
    Info,
    Layer,
    Multiple,
}

impl fmt::Display for ActionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ActionKind::Disabled => "Disabled",
            ActionKind::Gamepad => "Gamepad button",
            ActionKind::Keys => "Keyboard",
            ActionKind::Mouse => "Mouse button",
            ActionKind::Wheel => "Scroll wheel",
            ActionKind::NextProfile => "Next profile",
            ActionKind::Overlay => "On-screen keyboard",
            ActionKind::Numpad => "On-screen numpad",
            ActionKind::Toggle => "Toggle (press on / off)…",
            ActionKind::Macro => "Macro…",
            ActionKind::Menu => "Open menu…",
            ActionKind::Info => "Show info overlay…",
            ActionKind::Layer => "Layer…",
            ActionKind::Turbo => "Turbo (repeat while held)…",
            ActionKind::Multiple => "Multiple…",
        })
    }
}

/// Every kind, for a top-level action.
const ACTION_KINDS: [ActionKind; 15] = [
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::NextProfile,
    ActionKind::Overlay,
    ActionKind::Numpad,
    ActionKind::Toggle,
    ActionKind::Turbo,
    ActionKind::Macro,
    ActionKind::Menu,
    ActionKind::Info,
    ActionKind::Layer,
    ActionKind::Multiple,
];
/// Radial menu items: everything but opening another menu.
const RADIAL_ITEM_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::NextProfile,
    ActionKind::Overlay,
    ActionKind::Numpad,
    ActionKind::Toggle,
    ActionKind::Macro,
    ActionKind::Multiple,
];
/// What a menu item can do: anything a button can, including opening a submenu.
const MENU_ITEM_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::NextProfile,
    ActionKind::Overlay,
    ActionKind::Numpad,
    ActionKind::Toggle,
    ActionKind::Macro,
    ActionKind::Menu,
    ActionKind::Multiple,
];

const MENUS_HELP: &str = "On-screen menus you open with the \"Open menu…\" action from any button, \
    gesture, combo or trigger. A menu is up while that input is held and closes when you let go; wrap the \
    action in \"Toggle\" to keep it up until pressed again. Radial: aim a stick, let go to choose. \
    Directional: four slots on the D-pad or face buttons. List: move with the D-pad or left stick, A \
    chooses. Button menu: a list where items also have their own button. Carousel: cycle with the chosen \
    controls, A chooses. Items tap their action like a button press; an item can open another menu of the \
    same kind (not from radial menus), which closes along with it.";

/// What a macro step can press: plain outputs only.
const MACRO_STEP_KINDS: &[ActionKind] =
    &[ActionKind::Keys, ActionKind::Mouse, ActionKind::Wheel, ActionKind::Gamepad];
/// Entries of a Multiple list.
const MULTI_ENTRY_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::NextProfile,
];
/// What a Toggle can hold down: basic outputs, a turbo (toggleable auto-fire) or several.
const TOGGLE_INNER_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::Turbo,
    // A repeating macro toggled on loops until toggled off.
    ActionKind::Macro,
    // Shown until toggled off.
    ActionKind::Info,
    // On until toggled off; how a menu item switches a layer.
    ActionKind::Layer,
    ActionKind::Multiple,
];
/// What a Turbo can repeat.
const TURBO_INNER_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::Multiple,
];
const DEFAULT_TURBO_RATE: f32 = 10.0;

/// Callback that turns an edited action into a message; lets editors nest inside `Multi`.
type OnAction<'a> = Rc<dyn Fn(ButtonAction) -> Message + 'a>;

fn set_action<'a>(target: Target) -> OnAction<'a> {
    Rc::new(move |a| Message::SetAction(target, a))
}

/// Editor for one action. `nested` editors (entries of a Multiple list) can't be Multiple.
fn action_editor<'a>(
    action: &'a ButtonAction,
    default_button: Button,
    kinds: &'static [ActionKind],
    on_change: OnAction<'a>,
    field: KeyField,
    names: &Names,
) -> Element<'a, Message> {
    let kind = action_kind(action);
    // Shared items can't use layers, so they aren't offered there.
    let kinds: Vec<ActionKind> = kinds.iter().copied().filter(|k| *k != ActionKind::Layer || names.layers_allowed).collect();
    let kind_picker = {
        let on_change = on_change.clone();
        let (current, names) = (action.clone(), names.clone());
        dropdown(kinds, Some(kind), move |k| on_change(new_action(k, default_button, &current, &names))).width(170)
    };
    let value = action_value(action, default_button, on_change, field, names);
    row![kind_picker, value].spacing(8).align_y(Alignment::Start).into()
}

fn action_kind(action: &ButtonAction) -> ActionKind {
    match action {
        ButtonAction::Disabled => ActionKind::Disabled,
        ButtonAction::Gamepad(_) => ActionKind::Gamepad,
        ButtonAction::Keys(_) => ActionKind::Keys,
        ButtonAction::Mouse(_) => ActionKind::Mouse,
        ButtonAction::Wheel(_) => ActionKind::Wheel,
        ButtonAction::NextProfile => ActionKind::NextProfile,
        ButtonAction::ToggleOverlay => ActionKind::Overlay,
        ButtonAction::ToggleNumpad => ActionKind::Numpad,
        ButtonAction::Multi(_) => ActionKind::Multiple,
        ButtonAction::Toggle(_) => ActionKind::Toggle,
        ButtonAction::Turbo { .. } => ActionKind::Turbo,
        ButtonAction::Macro { .. } => ActionKind::Macro,
        ButtonAction::OpenMenu(_) => ActionKind::Menu,
        ButtonAction::ShowInfo(_) => ActionKind::Info,
        ButtonAction::Layer(_) => ActionKind::Layer,
    }
}

/// A fresh action of kind `k`, replacing `current`.
fn new_action(k: ActionKind, default_button: Button, current: &ButtonAction, names: &Names) -> ButtonAction {
    // When wrapping in Toggle/Turbo, keep a simple existing action as the thing wrapped.
    let wrappable = match current {
        ButtonAction::Gamepad(_) | ButtonAction::Keys(_) | ButtonAction::Mouse(_) | ButtonAction::Wheel(_) => {
            Some(current.clone())
        }
        _ => None,
    };
    match k {
        ActionKind::Disabled => ButtonAction::Disabled,
        ActionKind::Gamepad => ButtonAction::Gamepad(default_button),
        ActionKind::Keys => ButtonAction::Keys(Vec::new()),
        ActionKind::Mouse => ButtonAction::Mouse(MouseButton::Left),
        ActionKind::Wheel => ButtonAction::Wheel(WheelDirection::Up),
        ActionKind::NextProfile => ButtonAction::NextProfile,
        ActionKind::Overlay => ButtonAction::ToggleOverlay,
        ActionKind::Numpad => ButtonAction::ToggleNumpad,
        // Keep what was there as the first entry.
        ActionKind::Multiple => ButtonAction::Multi(wrappable.into_iter().collect()),
        ActionKind::Toggle => ButtonAction::Toggle(Box::new(wrappable.unwrap_or(ButtonAction::Keys(Vec::new())))),
        ActionKind::Turbo => ButtonAction::Turbo {
            action: Box::new(wrappable.unwrap_or(ButtonAction::Mouse(MouseButton::Left))),
            rate: DEFAULT_TURBO_RATE,
        },
        ActionKind::Macro => ButtonAction::Macro { name: names.macros.first().cloned().unwrap_or_default(), repeat: false },
        ActionKind::Menu => ButtonAction::OpenMenu(names.menus.first().cloned().unwrap_or_default()),
        ActionKind::Info => ButtonAction::ShowInfo(names.infos.first().cloned().unwrap_or_default()),
        // With no layers yet, picking the kind makes one.
        ActionKind::Layer => ButtonAction::Layer(names.layers.first().cloned().unwrap_or_else(|| NEW_LAYER.into())),
    }
}

/// The settings to the right of an action's kind picker.
fn action_value<'a>(
    action: &'a ButtonAction,
    default_button: Button,
    on_change: OnAction<'a>,
    field: KeyField,
    names: &Names,
) -> Element<'a, Message> {
    match action {
        ButtonAction::Gamepad(b) => dropdown(Button::EVERY, Some(*b), move |b| {
            on_change(ButtonAction::Gamepad(b))
        })
        .width(220)
        .into(),
        ButtonAction::Mouse(m) => dropdown(MouseButton::ALL, Some(*m), move |m| {
            on_change(ButtonAction::Mouse(m))
        })
        .width(220)
        .into(),
        ButtonAction::Wheel(d) => dropdown(WheelDirection::ALL, Some(*d), move |d| {
            on_change(ButtonAction::Wheel(d))
        })
        .width(220)
        .into(),
        ButtonAction::Keys(keys) => key_input(
            &keys_to_text(keys),
            "e.g. LEFTCTRL+C",
            move |s| on_change(ButtonAction::Keys(text_to_keys(&s))),
            Message::OpenKeyPicker(field, keys.clone(), false),
        ),
        ButtonAction::Multi(list) => multi_editor(list, default_button, on_change, field, names),
        ButtonAction::Toggle(inner) => {
            let parent = on_change.clone();
            let wrap: OnAction<'a> = Rc::new(move |a| parent(ButtonAction::Toggle(Box::new(a))));
            column![
                text("Each press turns this on or off:").size(12).color(MUTED_COLOR),
                action_editor(inner, default_button, TOGGLE_INNER_KINDS, wrap, field.child(0), names),
            ]
            .spacing(4)
            .into()
        }
        ButtonAction::Turbo { action: inner, rate } => {
            let rate = *rate;
            let parent = on_change.clone();
            let wrap: OnAction<'a> = Rc::new(move |a| parent(ButtonAction::Turbo { action: Box::new(a), rate }));
            let set_rate = {
                let inner = inner.clone();
                move |r: f32| on_change(ButtonAction::Turbo { action: inner.clone(), rate: r })
            };
            column![
                text("Repeats while held:").size(12).color(MUTED_COLOR),
                action_editor(inner, default_button, TURBO_INNER_KINDS, wrap, field.child(0), names),
                row![
                    slider(2.0..=30.0, rate, set_rate).step(1.0_f32).width(200),
                    text(format!("{rate:.0} presses/s")).size(13),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ]
            .spacing(4)
            .into()
        }
        ButtonAction::Macro { .. } if names.macros.is_empty() => {
            text("No macros yet: create one in the Macros tab.").size(12).color(MUTED_COLOR).into()
        }
        ButtonAction::Macro { name, repeat } => {
            let repeat = *repeat;
            let pick = {
                let on_change = on_change.clone();
                dropdown(names.macros.clone(), Some(name.clone()), move |n| on_change(ButtonAction::Macro { name: n, repeat }))
                    .width(200)
            };
            let name = name.clone();
            let missing = !names.macros.contains(&name);
            let mut line = row![
                pick,
                checkbox(repeat)
                    .label("Repeat while held")
                    .on_toggle(move |r| on_change(ButtonAction::Macro { name: name.clone(), repeat: r })),
            ]
            .spacing(12)
            .align_y(Alignment::Center);
            if missing {
                line = line.push(text("missing macro").size(12).color(ERROR_COLOR));
            }
            line.into()
        }
        ButtonAction::Disabled | ButtonAction::NextProfile => space().into(),
        ButtonAction::ToggleOverlay | ButtonAction::ToggleNumpad => {
            text("Hold B on the controller to close it.").size(12).color(MUTED_COLOR).into()
        }
        ButtonAction::ShowInfo(_) if names.infos.is_empty() => {
            text("No info overlays yet: create one on the Info overlays tab.").size(12).color(MUTED_COLOR).into()
        }
        ButtonAction::ShowInfo(name) => {
            let mut line = row![
                dropdown(names.infos.clone(), Some(name.clone()), move |n| on_change(ButtonAction::ShowInfo(n))).width(200),
                text("shown while held").size(12).color(MUTED_COLOR),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            if !names.infos.contains(name) {
                line = line.push(text("not available").size(12).color(ERROR_COLOR));
            }
            line.into()
        }
        ButtonAction::OpenMenu(_) if names.menus.is_empty() => {
            text("No menus to open: create one on the Menus tab.").size(12).color(MUTED_COLOR).into()
        }
        ButtonAction::Layer(name) => {
            let mut options = names.layers.clone();
            options.push(NEW_LAYER.to_string());
            let mut line = row![dropdown(options, Some(name.clone()), move |n| on_change(ButtonAction::Layer(n))).width(200)]
                .spacing(8)
                .align_y(Alignment::Center);
            if !names.layers.contains(name) {
                line = line.push(text("missing layer").size(12).color(ERROR_COLOR));
            }
            line.into()
        }
        ButtonAction::OpenMenu(name) => {
            let mut line = row![dropdown(names.menus.clone(), Some(name.clone()), move |n| on_change(ButtonAction::OpenMenu(n))).width(200)]
                .spacing(8)
                .align_y(Alignment::Center);
            if !names.menus.contains(name) {
                line = line.push(text("missing menu").size(12).color(ERROR_COLOR));
            }
            line.into()
        }
    }
}

/// List of simultaneous actions, each with its own editor and a remove button.
fn multi_editor<'a>(
    list: &'a [ButtonAction],
    default_button: Button,
    on_change: OnAction<'a>,
    field: KeyField,
    names: &Names,
) -> Element<'a, Message> {
    let with = |f: &dyn Fn(&mut Vec<ButtonAction>)| {
        let mut v = list.to_vec();
        f(&mut v);
        on_change(ButtonAction::Multi(v))
    };
    let mut col = column![].spacing(6);
    for (i, sub) in list.iter().enumerate() {
        let parent = on_change.clone();
        let entry: OnAction<'a> = Rc::new(move |a| {
            let mut v = list.to_vec();
            v[i] = a;
            parent(ButtonAction::Multi(v))
        });
        col = col.push(
            row![
                action_editor(sub, default_button, MULTI_ENTRY_KINDS, entry, field.child(i), names),
                button(text("✕").size(13)).style(button::secondary).on_press(with(&|v| {
                    v.remove(i);
                })),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    col.push(
        button(text("+ Add output").size(13))
            .style(button::secondary)
            .on_press(with(&|v| v.push(ButtonAction::Keys(Vec::new())))),
    )
    .into()
}

/// Text input for evdev key names (without the `KEY_` prefix) that turns red when invalid.
/// Also offers a button that opens the on-screen keyboard (`open_picker`).
fn key_input<'a>(
    value: &str,
    placeholder: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    open_picker: Message,
) -> Element<'a, Message> {
    let valid = value.is_empty()
        || value.split('+').all(|k| KeyCode::from_str(&format!("KEY_{k}")).is_ok());
    let input = field(placeholder, value).on_input(on_input).width(180);
    let pick = button(text("⌨").size(14)).style(button::secondary).on_press(open_picker);
    let mut r = row![input, pick].spacing(6).align_y(Alignment::Center);
    if !valid {
        r = r.push(text("unknown key").size(12).color(ERROR_COLOR));
    }
    r.into()
}

fn short_key(k: &str) -> &str {
    k.strip_prefix("KEY_").unwrap_or(k)
}

fn keys_to_text(keys: &[String]) -> String {
    keys.iter().map(|k| short_key(k)).collect::<Vec<_>>().join("+")
}

fn text_to_keys(s: &str) -> Vec<String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();
    if s.is_empty() {
        return Vec::new();
    }
    s.split('+').map(|k| format!("KEY_{k}")).collect()
}

fn single_key(s: &str) -> String {
    text_to_keys(s).into_iter().next().unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StickKind {
    Disabled,
    Gamepad,
    Mouse,
    Scroll,
    Keys,
}

impl fmt::Display for StickKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StickKind::Disabled => "Disabled",
            StickKind::Gamepad => "Gamepad stick",
            StickKind::Mouse => "Mouse pointer",
            StickKind::Scroll => "Scroll wheel",
            StickKind::Keys => "Direction keys",
        })
    }
}

fn stick_editor<'a>(s: Stick, cfg: &'a StickConfig, names: &Names) -> Element<'a, Message> {
    let kind = match cfg.action {
        StickAction::Disabled => StickKind::Disabled,
        StickAction::Gamepad { .. } => StickKind::Gamepad,
        StickAction::Mouse { .. } => StickKind::Mouse,
        StickAction::Scroll { .. } => StickKind::Scroll,
        StickAction::Keys { .. } => StickKind::Keys,
    };
    let with = move |action: StickAction| {
        let mut c = cfg.clone();
        c.action = action;
        Message::SetStick(s, c)
    };
    let kinds = [
        StickKind::Disabled,
        StickKind::Gamepad,
        StickKind::Mouse,
        StickKind::Scroll,
        StickKind::Keys,
    ];
    let picker = dropdown(kinds, Some(kind), move |k| {
        with(match k {
            StickKind::Disabled => StickAction::Disabled,
            StickKind::Gamepad => StickAction::Gamepad { stick: s, invert_y: false },
            StickKind::Mouse => StickAction::Mouse { speed: 1200.0 },
            StickKind::Scroll => StickAction::Scroll { speed: 15.0 },
            StickKind::Keys => wasd(),
        })
    })
    .width(170);

    let mut rows = column![labeled(s.to_string(), picker.into())].spacing(8);

    match &cfg.action {
        StickAction::Gamepad { stick, invert_y } => {
            let invert_y = *invert_y;
            let stick = *stick;
            rows = rows.push(labeled(
                "    Output",
                row![
                    dropdown([Stick::Left, Stick::Right], Some(stick), move |t| {
                        with(StickAction::Gamepad { stick: t, invert_y })
                    })
                    .width(170),
                    checkbox(invert_y)
                        .label("Invert Y")
                        .on_toggle(move |inv| with(StickAction::Gamepad { stick, invert_y: inv })),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
            ));
        }
        StickAction::Mouse { speed } => {
            rows = rows.push(value_slider("    Speed", 100.0..=4000.0, *speed, 50.0, "px/s", move |v| {
                with(StickAction::Mouse { speed: v })
            }));
        }
        StickAction::Scroll { speed } => {
            rows = rows.push(value_slider("    Speed", 1.0..=60.0, *speed, 1.0, "notches/s", move |v| {
                with(StickAction::Scroll { speed: v })
            }));
        }
        StickAction::Keys { up, down, left, right } => {
            let full = [up, down, left, right].map(|k| k.clone());
            let keys = full.clone().map(|k| short_key(&k).to_string());
            let current = keys.clone();
            let set = move |i: usize, v: String| {
                let mut k = current.clone().map(|k| format!("KEY_{k}"));
                k[i] = single_key(&v);
                let [up, down, left, right] = k;
                with(StickAction::Keys { up, down, left, right })
            };
            let field = |i: usize, label: &'static str| {
                let set = set.clone();
                let open = Message::OpenKeyPicker(KeyField::StickDir { stick: s, dir: i }, vec![full[i].clone()], true);
                column![text(label).size(12).color(MUTED_COLOR), key_input(&keys[i], label, move |v| set(i, v), open)]
                    .spacing(2)
            };
            rows = rows.push(labeled(
                "    Keys",
                column![
                    row![field(0, "Up"), field(1, "Down")].spacing(8),
                    row![field(2, "Left"), field(3, "Right")].spacing(8),
                    row![
                        button(text("WASD").size(13)).style(button::secondary).on_press(with(wasd())),
                        button(text("Arrows").size(13)).style(button::secondary).on_press(with(arrows())),
                    ]
                    .spacing(8),
                ]
                .spacing(6)
                .into(),
            ));
        }
        StickAction::Disabled => {}
    }

    if !matches!(cfg.action, StickAction::Disabled) {
        rows = rows.push(value_slider("    Deadzone", 0.0..=0.5, cfg.deadzone, 0.01, "", move |v| {
            let mut c = cfg.clone();
            c.deadzone = v;
            Message::SetStick(s, c)
        }));
    }
    if matches!(cfg.action, StickAction::Mouse { .. } | StickAction::Scroll { .. }) {
        rows = rows.push(value_slider("    Curve", 1.0..=3.0, cfg.curve, 0.1, "(1 = linear)", move |v| {
            let mut c = cfg.clone();
            c.curve = v;
            Message::SetStick(s, c)
        }));
    }
    // Used by direction keys and by the stick's direction buttons below.
    rows = rows.push(value_slider("    Directions press at", 0.05..=0.95, cfg.key_threshold, 0.05, "", move |v| {
        let mut c = cfg.clone();
        c.key_threshold = v;
        Message::SetStick(s, c)
    }));
    rows.push(zone_editor(Analog::Stick(s), &cfg.zones, names)).into()
}

/// Extra actions held while the stick/trigger is within a range of travel.
fn zone_editor<'a>(analog: Analog, zones: &'a [Zone], names: &Names) -> Element<'a, Message> {
    let what = match analog {
        Analog::Stick(_) => "pushed",
        Analog::Trigger(_) => "pulled",
    };
    let zones_help = help(format!(
        "Zones: extra outputs held while it is {what} within a range (0 = just past rest, 1 = all the \
         way), on top of its main action."
    ));
    let mut col = column![].spacing(8);

    for (i, zone) in zones.iter().enumerate() {
        let (min, max) = (zone.min, zone.max);
        let range = row![
            text("From").size(13),
            slider(0.0..=1.0, min, move |v| Message::SetZoneRange(analog, i, v, max)).step(0.05_f32).width(140),
            text(format!("{min:.2}")).size(13),
            text("to").size(13),
            slider(0.0..=1.0, max, move |v| Message::SetZoneRange(analog, i, min, v)).step(0.05_f32).width(140),
            text(format!("{max:.2}")).size(13),
            space::horizontal(),
            button(text("✕").size(13)).style(button::secondary).on_press(Message::RemoveZone(analog, i)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let mut body = column![
            range,
            action_editor(&zone.action, Button::South, &ACTION_KINDS, set_action(Target::Zone(analog, i)), KeyField::root(Target::Zone(analog, i)), names),
        ]
        .spacing(8);
        if min >= max {
            body = body.push(text("The range must start below where it ends.").size(12).color(ERROR_COLOR));
        }
        col = col.push(container(body).padding(10).style(style::inset));
    }

    let mut buttons = row![
        button(text("+ Add zone").size(13)).style(button::secondary).on_press(Message::AddZone(analog, ZonePreset::Empty)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if matches!(analog, Analog::Stick(_)) {
        buttons = buttons.push(
            button(text("+ Walk modifier (Shift on partial push)").size(13))
                .style(button::secondary)
                .on_press(Message::AddZone(analog, ZonePreset::Walk)),
        );
    }
    labeled("    Zones", col.push(buttons.push(zones_help)).into())
}

fn wasd() -> StickAction {
    StickAction::Keys {
        up: "KEY_W".into(),
        down: "KEY_S".into(),
        left: "KEY_A".into(),
        right: "KEY_D".into(),
    }
}

fn arrows() -> StickAction {
    StickAction::Keys {
        up: "KEY_UP".into(),
        down: "KEY_DOWN".into(),
        left: "KEY_LEFT".into(),
        right: "KEY_RIGHT".into(),
    }
}

fn value_slider<'a>(
    label: &'a str,
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    step: f32,
    unit: &'a str,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    let shown = if step >= 1.0 { format!("{value:.0} {unit}") } else { format!("{value:.2} {unit}") };
    labeled(
        label,
        row![slider(range, value, on_change).step(step).width(300), text(shown).size(13)]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
    )
}

/// What a trigger does, in one list: pass through as an analog trigger, or act as a button
/// with any button action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerChoice {
    Analog(Trigger),
    Action(ActionKind),
}

impl fmt::Display for TriggerChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TriggerChoice::Analog(Trigger::Left) => f.write_str("Gamepad LT (analog)"),
            TriggerChoice::Analog(Trigger::Right) => f.write_str("Gamepad RT (analog)"),
            TriggerChoice::Action(k) => k.fmt(f),
        }
    }
}

fn trigger_editor<'a>(
    t: Trigger,
    action: &'a TriggerAction,
    zones: &'a [Zone],
    analog: bool,
    names: &Names,
) -> Element<'a, Message> {
    let choice = match action {
        TriggerAction::Disabled => TriggerChoice::Action(ActionKind::Disabled),
        TriggerAction::Gamepad(out) => TriggerChoice::Analog(*out),
        TriggerAction::Button { action, .. } => TriggerChoice::Action(action_kind(action)),
    };
    // Analog outputs first, then everything a button can do.
    let other = if t == Trigger::Left { Trigger::Right } else { Trigger::Left };
    let mut choices = vec![TriggerChoice::Action(ActionKind::Disabled), TriggerChoice::Analog(t), TriggerChoice::Analog(other)];
    choices.extend(ACTION_KINDS.into_iter().skip(1).map(TriggerChoice::Action));
    // A trigger acting as a button presses the matching bumper by default.
    let default_button = if t == Trigger::Left { Button::LeftBumper } else { Button::RightBumper };
    let (threshold, current) = match action {
        TriggerAction::Button { action, threshold } => (*threshold, action.clone()),
        _ => (0.5, ButtonAction::Disabled),
    };
    let picker = {
        let names = names.clone();
        dropdown(choices, Some(choice), move |c| {
            Message::SetTrigger(
                t,
                match c {
                    TriggerChoice::Analog(out) => TriggerAction::Gamepad(out),
                    TriggerChoice::Action(ActionKind::Disabled) => TriggerAction::Disabled,
                    TriggerChoice::Action(k) => {
                        TriggerAction::Button { action: new_action(k, default_button, &current, &names), threshold }
                    }
                },
            )
        })
        .width(215)
    };

    let mut line = row![text(t.to_string()).width(LABEL_WIDTH), picker].spacing(10).align_y(Alignment::Start);
    let mut rows = column![].spacing(8);
    if let TriggerAction::Button { action: inner, threshold } = action {
        let threshold = *threshold;
        let on_change: OnAction<'a> = Rc::new(move |a| Message::SetTrigger(t, TriggerAction::Button { action: a, threshold }));
        line = line.push(action_value(inner, default_button, on_change, KeyField::root(Target::Trigger(t)), names));
        rows = rows.push(line);
        if analog {
            let inner = inner.clone();
            rows = rows.push(value_slider("    Presses at", 0.05..=0.95, threshold, 0.05, "", move |v| {
                Message::SetTrigger(t, TriggerAction::Button { action: inner.clone(), threshold: v })
            }));
        }
    } else {
        rows = rows.push(line);
    }
    if analog {
        return rows.push(zone_editor(Analog::Trigger(t), zones, names)).into();
    }
    rows = rows.push(labeled(
        "",
        text("Your controller's triggers are on/off only, so pull thresholds and zones don't apply.")
            .size(12)
            .color(MUTED_COLOR)
            .into(),
    ));
    // Keep existing zones reachable so they can be removed rather than silently lingering.
    if !zones.is_empty() {
        rows = rows
            .push(labeled(
                "",
                text("This profile still has zones for this trigger; a full pull acts as the top zone.")
                    .size(12)
                    .color(ERROR_COLOR)
                    .into(),
            ))
            .push(zone_editor(Analog::Trigger(t), zones, names));
    }
    rows.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The app on General's page, editing its "Gamepad" profile.
    fn app() -> App {
        let mut app = App::boot().0;
        let _ = app.update(Message::SelectPage(Page::Game(None)));
        app.editing = 0;
        app
    }

    /// The app with a game "Doom" (profiles "Play" and "Menus") shown.
    fn with_game() -> App {
        let mut app = app();
        let mut game = Game::new("Doom", vec![Profile::pc_action("Play"), Profile::desktop("Menus")]);
        game.rules.push(Rule::new(RuleKind::Executable, "doom.exe", "Play"));
        app.config.games.push(game);
        let _ = app.update(Message::SelectPage(Page::Game(Some("Doom".into()))));
        app
    }

    fn pick(app: &mut App, field: KeyField, single: bool, keys: &[&'static str]) {
        let _ = app.update(Message::OpenKeyPicker(field, Vec::new(), single));
        for k in keys {
            let _ = app.update(Message::PickerKey(k));
        }
        if !single {
            let _ = app.update(Message::PickerClose { apply: true });
        }
    }

    #[test]
    fn picker_writes_chord_into_nested_multi_entry() {
        let mut app = app();
        let multi = ButtonAction::Multi(vec![ButtonAction::Gamepad(Button::South), ButtonAction::Keys(vec![])]);
        app.config.general.profiles[0].set_button(Button::South, multi);
        let field = KeyField::root(Target::Button(Button::South)).child(1);
        // Toggling a key twice removes it again.
        pick(&mut app, field, false, &["KEY_LEFTSHIFT", "KEY_A", "KEY_B", "KEY_B"]);
        assert_eq!(
            app.config.general.profiles[0].button(Button::South),
            &ButtonAction::Multi(vec![
                ButtonAction::Gamepad(Button::South),
                ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into(), "KEY_A".into()]),
            ])
        );
        assert!(app.picker.is_none());
    }

    #[test]
    fn single_picker_sets_stick_direction() {
        let mut app = app();
        app.config.general.profiles[0].left_stick.action = wasd();
        let _ = app.update(Message::OpenKeyPicker(KeyField::StickDir { stick: Stick::Left, dir: 2 }, vec![], true));
        // Single mode closes itself by scheduling PickerClose; run it like the runtime would.
        let _ = app.update(Message::PickerKey("KEY_LEFT"));
        let _ = app.update(Message::PickerClose { apply: true });
        let StickAction::Keys { left, up, .. } = &app.config.general.profiles[0].left_stick.action else { panic!() };
        assert_eq!((left.as_str(), up.as_str()), ("KEY_LEFT", "KEY_W"));
    }

    fn device(name: &str, analog_triggers: bool, ignored: bool) -> ipc::DeviceInfo {
        ipc::DeviceInfo { name: name.into(), path: String::new(), managed: !ignored, ignored, analog_triggers, rumble: true, gyro: false }
    }

    #[test]
    fn trigger_depth_settings_hide_only_when_all_pads_are_digital() {
        let mut app = app();
        let status = |devices| Status { enabled: true, active_profile: "Gamepad".into(), devices, ..Default::default() };
        assert!(app.analog_triggers(), "no daemon: assume analog");

        app.status = Some(status(vec![device("Pro Controller", false, false)]));
        assert!(!app.analog_triggers());

        app.status = Some(status(vec![device("Pro Controller", false, false), device("Xbox", true, false)]));
        assert!(app.analog_triggers());

        // An ignored Xbox pad doesn't count.
        app.status = Some(status(vec![device("Pro Controller", false, false), device("Xbox", true, true)]));
        assert!(!app.analog_triggers());

        app.status = Some(status(vec![]));
        assert!(app.analog_triggers());
    }

    #[test]
    fn rules_from_windows_prefer_the_most_specific_identifier() {
        let w = |class: &str, exe: &str, steam: Option<&str>| WindowInfo {
            class: class.into(),
            exe: exe.into(),
            steam_app_id: steam.map(Into::into),
            ..Default::default()
        };
        let pick = |w: WindowInfo| {
            let r = rule_for_window(&w, "P".into());
            (r.kind, r.value)
        };
        assert_eq!(pick(w("steam_app_1", "game.exe", Some("1"))), (RuleKind::SteamAppId, "1".into()));
        assert_eq!(pick(w("wine", "game.exe", None)), (RuleKind::Executable, "game.exe".into()));
        // Native exe names can be generic (java, python), so the class is preferred.
        assert_eq!(pick(w("Minecraft", "java", None)), (RuleKind::WindowClass, "Minecraft".into()));
        assert_eq!(pick(w("", "factorio", None)), (RuleKind::Executable, "factorio".into()));
    }

    #[test]
    fn renaming_a_profile_updates_rules_default_and_the_active_one() {
        let mut app = with_game();
        app.config.auto_switch.default_profile = Some(ProfileRef::new(Some("Doom"), "Play"));
        // The daemon says Doom › Play is active.
        let status = Status { active_profile: "Play".into(), active_game: Some("Doom".into()), ..Default::default() };
        let _ = app.update(Message::StatusLoaded(Ok(status.clone())));
        let _ = app.update(Message::RenameProfile("Fight".into()));
        assert_eq!(app.game().rules[0].profile, "Fight");
        assert_eq!(app.config.auto_switch.default_profile, Some(ProfileRef::new(Some("Doom"), "Fight")));
        // Until saved, the daemon keeps reporting the old name; it's followed.
        let _ = app.update(Message::StatusLoaded(Ok(status)));
        assert_eq!(app.config.active, ProfileRef::new(Some("Doom"), "Fight"));
        assert_eq!(app.saved.active, ProfileRef::new(Some("Doom"), "Play"));
        assert_eq!(app.validate(), None);

        let _ = app.update(Message::SetDefaultProfile(DefaultChoice(None)));
        assert_eq!(app.config.auto_switch.default_profile, None);
    }

    #[test]
    fn rules_must_point_at_existing_profiles() {
        let mut app = with_game();
        let _ = app.update(Message::AddRule(None));
        assert!(app.validate().unwrap().contains("no Executable to match"), "{:?}", app.validate());
        let _ = app.update(Message::SetRuleValue(1, "x".into()));
        let _ = app.update(Message::SetRuleProfile(1, "Gone".into()));
        let err = app.validate().unwrap();
        assert!(err.starts_with("Doom: ") && err.contains("missing profile"), "{err}");
        let _ = app.update(Message::RemoveRule(1));
        let _ = app.update(Message::SetRuleEnabled(0, false));
        assert!(!app.game().rules[0].enabled);
        assert_eq!(app.validate(), None);
    }

    #[test]
    fn new_profile_from_template_gets_unique_name_and_is_edited() {
        let mut app = app();
        let _ = app.update(Message::AddProfile(Template::Action));
        let _ = app.update(Message::AddProfile(Template::Action));
        let names: Vec<&str> = app.config.general.profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Gamepad", "Desktop", "PC Action", "PC Action 2"]);
        assert_eq!(app.profile().unwrap().name, "PC Action 2");
        assert_eq!(app.validate(), None);
    }

    #[test]
    fn wrapping_in_toggle_keeps_the_action_and_picker_writes_inside_it() {
        let mut app = app();
        app.config.general.profiles[0].set_button(Button::RightStick, ButtonAction::Keys(vec!["KEY_C".into()]));
        // What choosing "Toggle" in the kind list produces for an existing key action.
        let wrapped = ButtonAction::Toggle(Box::new(ButtonAction::Keys(vec!["KEY_C".into()])));
        let _ = app.update(Message::SetAction(Target::Button(Button::RightStick), wrapped));
        let field = KeyField::root(Target::Button(Button::RightStick)).child(0);
        pick(&mut app, field, false, &["KEY_LEFTCTRL"]);
        assert_eq!(
            app.config.general.profiles[0].button(Button::RightStick),
            &ButtonAction::Toggle(Box::new(ButtonAction::Keys(vec!["KEY_LEFTCTRL".into()])))
        );
    }

    #[test]
    fn motion_rule_command_installs_the_shipped_rule() {
        let cmd = motion_rule_command();
        assert!(cmd.contains("ENV{ID_INPUT_ACCELEROMETER}==\"1\""), "{cmd}");
        assert!(cmd.contains("TAG+=\"uaccess\""), "{cmd}");
        // Single-quoted for the shell, so the rule itself must not contain a quote.
        let quoted = cmd.split('\'').nth(1).unwrap();
        assert!(!quoted.is_empty() && !quoted.contains('#'), "comments are left out: {quoted}");
        assert_eq!(cmd.matches('\'').count(), 2, "{cmd}");
    }

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

        let mapped = ButtonAction::Toggle(Box::new(ButtonAction::Macro { name: "Macro".into(), repeat: true }));
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
            &ButtonAction::Toggle(Box::new(ButtonAction::Macro { name: "Jump spam".into(), repeat: true }))
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
    fn stick_direction_buttons_are_editable_validated_and_usable_in_combos() {
        let mut app = app();
        let _ = app.update(Message::NewMacro);
        let up = Target::Button(Button::RightStickUp);
        let _ = app.update(Message::SetAction(up, ButtonAction::Macro { name: "Macro".into(), repeat: false }));
        let _ = app.update(Message::RenameMacro(0, "Hadouken".into()));
        assert_eq!(
            app.config.general.profiles[0].button(Button::RightStickUp),
            &ButtonAction::Macro { name: "Hadouken".into(), repeat: false }
        );
        // The key picker writes into a direction, and validation sees direction keys.
        let down = Target::Button(Button::RightStickDown);
        let _ = app.update(Message::SetAction(down, ButtonAction::Keys(vec![])));
        pick(&mut app, KeyField::root(down), false, &["KEY_C"]);
        assert_eq!(app.config.general.profiles[0].button(Button::RightStickDown), &ButtonAction::Keys(vec!["KEY_C".into()]));
        // Combos accept stick directions as members.
        let _ = app.update(Message::AddCombo);
        let _ = app.update(Message::RemoveComboButton(0, Button::RightBumper));
        let _ = app.update(Message::AddComboButton(0, Button::RightStickRight));
        assert_eq!(app.config.general.profiles[0].combos[0].buttons, [Button::LeftBumper, Button::RightStickRight]);

        let _ = app.update(Message::SetAction(
            Target::Button(Button::RightStickLeft),
            ButtonAction::Keys(vec!["KEY_NOPE".into()]),
        ));
        app.config.general.macros[0].steps = vec![MacroStep::Wait(10)];
        assert!(app.validate().unwrap().contains("NOPE"), "{:?}", app.validate());
    }

    #[test]
    fn summaries_read_like_the_mapping() {
        let keys = |k: &[&str]| ButtonAction::Keys(k.iter().map(|s| s.to_string()).collect());
        assert_eq!(summarize(&keys(&["KEY_LEFTCTRL", "KEY_C"])), "Left Ctrl + C");
        assert_eq!(summarize(&ButtonAction::Disabled), "—");
        assert_eq!(
            summarize(&ButtonAction::Toggle(Box::new(ButtonAction::Turbo {
                action: Box::new(ButtonAction::Mouse(MouseButton::Left)),
                rate: 12.0
            }))),
            "Toggle Turbo Left click (12/s)"
        );
        assert_eq!(summarize(&ButtonAction::Macro { name: "QCF".into(), repeat: true }), "Macro “QCF” (repeat)");
        assert_eq!(summarize(&ButtonAction::Gamepad(Button::RightStickRight)), "Pad RS→");
    }

    #[test]
    fn problems_are_found_inside_nested_actions_and_flag_their_section() {
        let names = Names { macros: vec!["Known".to_string()], ..Names::default() };
        let nested = ButtonAction::Multi(vec![
            ButtonAction::Mouse(MouseButton::Left),
            ButtonAction::Toggle(Box::new(ButtonAction::Keys(vec!["KEY_NOPE".into()]))),
        ]);
        assert_eq!(action_problem(&nested, &names).as_deref(), Some("unknown key \"NOPE\""));
        let missing = ButtonAction::Macro { name: "Gone".into(), repeat: false };
        assert_eq!(action_problem(&missing, &names).as_deref(), Some("missing macro \"Gone\""));
        assert_eq!(action_problem(&ButtonAction::Macro { name: "Known".into(), repeat: false }, &names), None);

        let mut p = Profile::passthrough("p");
        assert!(ProfileTab::ALL.iter().all(|t| !section_has_problem(&p, *t, &names)));
        p.set_button(Button::RightStickUp, missing);
        assert!(section_has_problem(&p, ProfileTab::Sticks, &names), "stick directions live on the Sticks tab");
        assert!(!section_has_problem(&p, ProfileTab::Buttons, &names));
        p.combos.push(Combo { buttons: vec![Button::South], action: ButtonAction::Disabled });
        assert!(section_has_problem(&p, ProfileTab::Combos, &names));
    }

    fn snapshot(buttons: &[Button], right_stick: (f32, f32)) -> InputSnapshot {
        InputSnapshot {
            device: "Pad".into(),
            buttons: buttons.to_vec(),
            left_stick: (0.0, 0.0),
            right_stick,
            left_trigger: 0.0,
            right_trigger: 0.0,
            gyro: None,
        }
    }

    #[test]
    fn find_by_pressing_detects_new_presses_and_firm_stick_pushes() {
        let held = snapshot(&[Button::South], (0.0, 0.0));
        assert_eq!(newly_pressed(None, &held), Some(Button::South));
        // Still held from before: not new. A second button is.
        assert_eq!(newly_pressed(Some(&held), &held), None);
        assert_eq!(newly_pressed(Some(&held), &snapshot(&[Button::South, Button::West], (0.0, 0.0))), Some(Button::West));
        // A light touch on a stick doesn't count; a firm push does.
        assert_eq!(newly_pressed(Some(&held), &snapshot(&[Button::South], (0.4, 0.0))), None);
        assert_eq!(newly_pressed(Some(&held), &snapshot(&[Button::South], (0.0, -0.9))), Some(Button::RightStickUp));
    }

    #[test]
    fn find_by_pressing_opens_and_highlights_the_row() {
        let mut app = app();
        let _ = app.update(Message::StartFind);
        assert!(app.finding);
        let _ = app.update(Message::LiveInput(Some(snapshot(&[Button::West], (0.0, 0.0)))));
        assert!(!app.finding, "one press ends find mode");
        assert_eq!((app.game_tab, app.profile_tab, app.found), (GameTab::Profiles, ProfileTab::Buttons, Some(Button::West)));
        let names = Names::default();
        let ui = Ui { names: &names, expanded: &app.expanded, found: app.found, analog_triggers: true, any_gyro: false, layer: None };
        assert!(ui.is_open(Target::Button(Button::West)));
        // Presses while not finding don't move the editor.
        let _ = app.update(Message::LiveInput(Some(snapshot(&[Button::West, Button::North], (0.0, 0.0)))));
        assert_eq!(app.found, Some(Button::West));
        // A stick push jumps to the Sticks & triggers tab.
        let _ = app.update(Message::StartFind);
        let _ = app.update(Message::LiveInput(Some(snapshot(&[], (0.95, 0.0)))));
        assert_eq!((app.profile_tab, app.found), (ProfileTab::Sticks, Some(Button::RightStickRight)));
    }

    #[test]
    fn rows_expand_and_collapse() {
        let mut app = app();
        let a = Target::Button(Button::South);
        let _ = app.update(Message::ToggleExpanded(a));
        assert!(app.expanded.contains(&a));
        let _ = app.update(Message::ToggleExpanded(a));
        assert!(!app.expanded.contains(&a));
        let _ = app.update(Message::ExpandAll(true));
        assert_eq!(app.expanded.len(), Button::ALL.len());
        let _ = app.update(Message::SelectProfileTab(ProfileTab::Sticks));
        let _ = app.update(Message::ExpandAll(true));
        assert_eq!(app.expanded.len(), Button::ALL.len() + 8);
        let _ = app.update(Message::ExpandAll(false));
        assert_eq!(app.expanded.len(), Button::ALL.len(), "collapse all only touches the current section");
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
    fn games_are_renamed_deleted_and_added_empty() {
        let mut app = with_game();
        app.config.auto_switch.default_profile = Some(ProfileRef::new(Some("Doom"), "Menus"));
        let _ = app.update(Message::RenameGame("Doom II".into()));
        assert_eq!(app.page, Page::Game(Some("Doom II".into())));
        assert_eq!(app.config.auto_switch.default_profile, Some(ProfileRef::new(Some("Doom II"), "Menus")));
        assert_eq!(app.game().name, "Doom II");

        let _ = app.update(Message::AskDeleteGame);
        assert!(matches!(app.dialog, Some(Dialog::DeleteGame(ref n)) if n == "Doom II"));
        let _ = app.update(Message::ConfirmDeleteGame);
        assert!(app.config.games.is_empty() && app.dialog.is_none());
        assert_eq!(app.page, Page::Game(None));
        assert_eq!(app.config.auto_switch.default_profile, None);

        let _ = app.update(Message::AddEmptyGame(Template::Strategy));
        assert_eq!(app.page, Page::Game(Some("New game".into())));
        assert_eq!(app.game().profiles[0].name, "Strategy");
        assert_eq!(app.game_tab, GameTab::Details);
        let _ = app.update(Message::AddEmptyGame(Template::Gamepad));
        assert_eq!(app.config.games[1].name, "New game 2");
    }

    #[test]
    fn importing_a_pack_file_previews_then_adds_the_game() {
        let mut app = with_game();
        app.config.shared.macros.push(Macro { name: "Heal".into(), steps: vec![MacroStep::Wait(10)] });
        app.config.games[0].profiles[0].set_button(Button::North, ButtonAction::Macro { name: "Heal".into(), repeat: false });
        let text = pack::export(&app.config.games[0], &app.config.shared, &pack::draft(&app.config.games[0], false))
            .pack
            .to_toml()
            .unwrap();

        let _ = app.update(Message::PackFileRead(Some(Ok(text))));
        let Some(Dialog::Import { plan, .. }) = &app.dialog else { panic!("no preview") };
        assert!(plan.name_clash && plan.shared_clashes.len() == 1 && plan.rule_clashes.len() == 1);
        assert_eq!(app.config.games.len(), 1, "nothing changes before confirming");
        let _ = app.update(Message::SetKeepMine(0, true));
        let _ = app.update(Message::ConfirmImport);
        assert_eq!(app.page, Page::Game(Some("Doom (2)".into())));
        let imported = app.game();
        assert_eq!(imported.macros[0].name, "Heal (2)");
        assert!(!imported.rules[0].enabled && app.config.games[0].rules[0].enabled, "kept mine");
        assert_eq!(app.validate(), None);

        let _ = app.update(Message::PackFileRead(Some(Ok("format = 9".into()))));
        assert!(app.message.as_ref().is_some_and(|(m, err)| *err && m.contains("newer version")));
    }

    #[test]
    fn export_drafts_fork_and_remember_their_details() {
        let mut app = with_game();
        let _ = app.update(Message::OpenExport);
        let Some(Dialog::Export { info, .. }) = &app.dialog else { panic!("no export dialog") };
        let id = info.id.clone();
        assert_eq!(info.version, "1.0");
        let _ = app.update(Message::SetPackField(PackField::Author, "me".into()));
        let _ = app.update(Message::Exported(Some(Ok("/tmp/doom.padpack".into()))));
        assert!(app.dialog.is_none());
        assert_eq!((app.game().pack.id.as_str(), app.game().pack.author.as_str()), (id.as_str(), "me"));
        let _ = app.update(Message::OpenExport);
        let Some(Dialog::Export { info, .. }) = &app.dialog else { panic!() };
        assert_eq!(info.id, id, "my own pack keeps its ID");

        // General has no Details tab, so nothing to export.
        let _ = app.update(Message::CloseDialog);
        let _ = app.update(Message::SelectPage(Page::Game(None)));
        let _ = app.update(Message::OpenExport);
        assert!(app.dialog.is_none());
    }

    #[test]
    fn items_are_copied_from_other_games() {
        let mut app = with_game();
        let mut quake = Game::new("Quake", vec![Profile::passthrough("P")]);
        quake.info.push(InfoOverlay { name: "Controls".into(), always: true, style: OverlayStyle::info(), rows: vec![] });
        app.config.games.push(quake);
        app.config.games[0].info.push(InfoOverlay { name: "Controls".into(), always: false, style: OverlayStyle::info(), rows: vec![] });
        let _ = app.update(Message::OpenBrowse(ItemKind::Info));
        let _ = app.update(Message::BrowseFrom(BrowseSource::Game(Some("Quake".into()))));
        let _ = app.update(Message::CopyItem(0));
        assert!(app.dialog.is_none());
        let names: Vec<&str> = app.game().info.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["Controls (2)", "Controls"]);
        assert!(app.game().info[0].always);
        assert!(app.open_infos.contains(&0));
    }

    #[test]
    fn every_page_tab_and_dialog_builds() {
        let mut app = with_game();
        app.config.shared.menus.push(Menu { name: "Wheel".into(), kind: MenuKind::List, items: vec![], cancel: None, style: OverlayStyle::default() });
        app.config.games[0].info.push(InfoOverlay { name: "Controls".into(), always: true, style: OverlayStyle::info(), rows: vec![vec!["{south}".into()]] });
        app.config.games[0].macros.push(Macro { name: "Dodge".into(), steps: vec![MacroStep::Wait(5)] });
        app.config.games[0].layers.push(crate::config::Layer::new("Hotkeys"));
        app.status = Some(Status { devices: vec![device("Pad", true, false)], active_layers: vec!["Hotkeys".into()], ..Default::default() });
        let _ = app.view();
        for page in [Page::Overview, Page::Settings, Page::Game(None), Page::Game(Some("Doom".into()))] {
            let _ = app.update(Message::SelectPage(page));
            for tab in GameTab::ALL {
                let _ = app.update(Message::SelectGameTab(tab));
                let _ = app.update(Message::ToggleMacro(0));
                let _ = app.update(Message::ToggleMenu(0));
                let _ = app.update(Message::ToggleInfo(0));
                let _ = app.update(Message::ToggleSharedSection);
                let _ = app.view();
            }
        }
        let _ = app.update(Message::SetSharedView(true));
        let _ = app.view();
        let _ = app.update(Message::SelectPage(Page::Game(Some("Doom".into()))));
        let text = pack::export(app.game(), &app.config.shared, &pack::draft(app.game(), false)).pack.to_toml().unwrap();
        let dialogs = [
            Message::OpenAddGame,
            Message::Installed(launchers::Installed::default()),
            Message::PackFileRead(Some(Ok(text))),
            Message::OpenExport,
            Message::AskDeleteGame,
            Message::OpenBrowse(ItemKind::Menu),
            Message::BrowseFrom(BrowseSource::Game(None)),
        ];
        for m in dialogs {
            let _ = app.update(m);
            let _ = app.view();
        }
        let _ = app.update(Message::BrowseOpen(0));
        let _ = app.view();
    }

    #[test]
    fn copying_a_menu_brings_the_macros_it_runs() {
        let mut app = with_game();
        let heal = |steps: usize| Macro { name: "Heal".into(), steps: vec![MacroStep::Wait(1); steps] };
        let mut quake = Game::new("Quake", vec![Profile::passthrough("P")]);
        quake.macros = vec![heal(1), Macro { name: "Taunt".into(), steps: vec![] }];
        quake.menus.push(Menu {
            name: "Wheel".into(),
            kind: MenuKind::List,
            items: ["Heal", "Taunt"].map(|n| MenuItem { label: n.into(), action: ButtonAction::Macro { name: n.into(), repeat: false }, button: None }).into(),
            cancel: None,
            style: OverlayStyle::default(),
        });
        app.config.games.push(quake);
        // Doom has its own "Taunt", which the copy can use, but no "Heal".
        app.config.games[0].macros.push(Macro { name: "Taunt".into(), steps: vec![MacroStep::Wait(9)] });
        app.config.games[0].menus.push(Menu { name: "Wheel".into(), kind: MenuKind::List, items: vec![], cancel: None, style: OverlayStyle::default() });
        let _ = app.update(Message::SelectGameTab(GameTab::Menus));
        let _ = app.update(Message::OpenBrowse(ItemKind::Menu));
        let _ = app.update(Message::BrowseFrom(BrowseSource::Game(Some("Quake".into()))));
        let _ = app.update(Message::CopyItem(0));
        let doom = app.game();
        assert_eq!(doom.menus.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Wheel (2)", "Wheel"]);
        assert_eq!(doom.macros.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Heal", "Taunt"]);
        assert!(app.open_menus.contains(&0));
        assert!(app.message.as_ref().is_some_and(|(m, _)| m.contains("with macro “Heal”")), "{:?}", app.message);
        assert_eq!(app.validate(), None);
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
    fn editing_a_layer_records_only_overrides() {
        let mut app = with_game();
        let _ = app.update(Message::SelectGameTab(GameTab::Layers));
        let _ = app.update(Message::NewLayer);
        assert_eq!(app.game().layers[0].name, "Layer");
        assert!(app.editing_layer());
        let base_south = app.game().profiles[0].button(Button::South).clone();

        // Editing a button through the profile editor overrides it in the layer only.
        let f1 = ButtonAction::Keys(vec!["KEY_F1".into()]);
        let _ = app.update(Message::SetAction(Target::Button(Button::South), f1.clone()));
        let layer = &app.game().layers[0];
        assert_eq!(layer.buttons.get(&Button::South), Some(&f1));
        assert_eq!(layer.buttons.len(), 1);
        assert_eq!(app.game().profiles[0].button(Button::South), &base_south);
        assert_eq!(app.profile().unwrap().button(Button::South), &f1, "the editor shows the layer over the profile");

        // A gesture added in the layer, and the key picker writing into the layer.
        let _ = app.update(Message::AddGesture(Button::North, GestureKind::DoubleTap));
        assert!(app.game().layers[0].gestures.get(&Button::North).is_some_and(|g| g.double_tap.is_some()));
        pick(&mut app, KeyField::root(Target::Gesture(Button::North, GestureKind::DoubleTap)), false, &["KEY_F2"]);
        assert_eq!(
            app.game().layers[0].gestures[&Button::North].double_tap,
            Some(ButtonAction::Keys(vec!["KEY_F2".into()]))
        );

        // Override copies the profile's stick; editing it changes the layer's copy.
        let _ = app.update(Message::OverrideInput(LayerPart::Stick(Stick::Right)));
        assert_eq!(app.game().layers[0].right_stick.as_ref(), Some(&app.game().profiles[0].right_stick));
        let scroll = StickConfig::new(StickAction::Scroll { speed: 15.0 }, 0.15, 2.0);
        let _ = app.update(Message::SetStick(Stick::Right, scroll.clone()));
        assert_eq!(app.game().layers[0].right_stick, Some(scroll));
        assert_ne!(app.game().profiles[0].right_stick.action, StickAction::Scroll { speed: 15.0 });
        let _ = app.update(Message::RevertInput(LayerPart::Stick(Stick::Right)));
        let _ = app.update(Message::RevertInput(LayerPart::Button(Button::North)));
        assert!(app.game().layers[0].right_stick.is_none() && app.game().layers[0].gestures.is_empty());

        // Its own combos, and switching off one of the profile's.
        app.config.games[0].profiles[0].combos.push(Combo { buttons: vec![Button::LeftBumper, Button::RightBumper], action: ButtonAction::Disabled });
        let _ = app.update(Message::AddCombo);
        assert_eq!(app.game().layers[0].combos.len(), 1);
        assert_eq!(app.game().profiles[0].combos.len(), 1);
        let _ = app.update(Message::ToggleBaseCombo(vec![Button::LeftBumper, Button::RightBumper]));
        assert_eq!(app.game().layers[0].disabled_combos, [vec![Button::LeftBumper, Button::RightBumper]]);
        let _ = app.update(Message::RemoveCombo(0));
        assert_eq!(app.validate(), None);
        let _ = app.view();
    }

    #[test]
    fn layer_actions_make_follow_and_check_layers() {
        let mut app = with_game();
        // "+ New layer" in a button's picker makes one and points the button at it.
        let _ = app.update(Message::SetAction(Target::Button(Button::LeftBumper), ButtonAction::Layer(NEW_LAYER.into())));
        assert_eq!(app.game().layers[0].name, "Layer");
        assert_eq!(app.game().profiles[0].button(Button::LeftBumper), &ButtonAction::Layer("Layer".into()));
        // Renaming the layer follows.
        let _ = app.update(Message::SelectGameTab(GameTab::Layers));
        let _ = app.update(Message::RenameLayer("Hotkeys".into()));
        assert_eq!(app.game().profiles[0].button(Button::LeftBumper), &ButtonAction::Layer("Hotkeys".into()));
        assert_eq!(app.validate(), None);

        // A menu item can toggle a layer, but not hold one.
        let _ = app.update(Message::SelectGameTab(GameTab::Menus));
        let _ = app.update(Message::NewMenu(MenuKindTag::List));
        let _ = app.update(Message::SetAction(Target::MenuItem(0, 0), ButtonAction::Layer("Hotkeys".into())));
        assert!(app.validate().unwrap().contains("can only toggle a layer"), "{:?}", app.validate());
        let _ = app.update(Message::SetAction(Target::MenuItem(0, 0), ButtonAction::Toggle(Box::new(ButtonAction::Layer("Hotkeys".into())))));
        assert_eq!(app.validate(), None);

        // Shared items can't use layers at all.
        app.config.shared.menus.push(Menu {
            name: "Everywhere".into(),
            kind: MenuKind::List,
            items: vec![MenuItem { label: "x".into(), action: ButtonAction::Toggle(Box::new(ButtonAction::Layer("Hotkeys".into()))), button: None }],
            cancel: None,
            style: OverlayStyle::default(),
        });
        assert!(app.validate().unwrap().contains("shared items can't use layers"), "{:?}", app.validate());
        app.config.shared.menus.clear();

        // Deleting it leaves the mappings flagged.
        let _ = app.update(Message::SelectGameTab(GameTab::Layers));
        let _ = app.update(Message::DeleteLayer);
        assert!(app.validate().unwrap().contains("missing layer"), "{:?}", app.validate());
    }

    #[test]
    fn layers_are_copied_from_other_games_with_what_they_use() {
        let mut app = with_game();
        let mut quake = Game::new("Quake", vec![Profile::passthrough("P")]);
        quake.macros.push(Macro { name: "Lean".into(), steps: vec![MacroStep::Wait(5)] });
        let mut lean = crate::config::Layer::new("Lean");
        lean.buttons.insert(Button::West, ButtonAction::Macro { name: "Lean".into(), repeat: false });
        lean.indicator = crate::config::Indicator::Off;
        quake.layers.push(lean);
        app.config.games.push(quake);
        let _ = app.update(Message::SelectGameTab(GameTab::Layers));
        let _ = app.update(Message::OpenBrowse(ItemKind::Layer));
        let _ = app.update(Message::BrowseFrom(BrowseSource::Game(Some("Quake".into()))));
        let _ = app.view();
        let _ = app.update(Message::CopyItem(0));
        let doom = app.game();
        assert_eq!(doom.layers[0].name, "Lean");
        assert_eq!(doom.macros[0].name, "Lean");
        assert_eq!(app.validate(), None);
    }

    #[test]
    fn the_add_game_picker_and_page_changes_close_dialogs() {
        let mut app = app();
        let _ = app.update(Message::OpenAddGame);
        let _ = app.update(Message::SetLibrarySearch("doo".into()));
        assert!(matches!(&app.dialog, Some(Dialog::AddGame { search, .. }) if search == "doo"));
        let _ = app.update(Message::SelectPage(Page::Settings));
        assert!(app.dialog.is_none() && app.page == Page::Settings);
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
        assert_eq!(app.validate(), None);
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

    #[test]
    fn cancel_leaves_config_untouched() {
        let mut app = app();
        let before = app.config.clone();
        let _ = app.update(Message::OpenKeyPicker(KeyField::root(Target::Button(Button::North)), vec![], false));
        let _ = app.update(Message::PickerKey("KEY_Q"));
        let _ = app.update(Message::PickerClose { apply: false });
        assert_eq!(app.config, before);
    }

    #[test]
    fn key_text_roundtrip() {
        for s in ["", "LEFTCTRL+C", "LEFTCTRL+", "A"] {
            assert_eq!(keys_to_text(&text_to_keys(s)), s);
        }
        assert_eq!(text_to_keys("leftctrl + c"), vec!["KEY_LEFTCTRL", "KEY_C"]);
    }
}

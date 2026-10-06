use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Normalized gamepad button, following the Linux gamepad spec (Documentation/input/gamepad.rst).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Button {
    South,
    East,
    North,
    West,
    LeftBumper,
    RightBumper,
    Select,
    Start,
    Guide,
    LeftStick,
    RightStick,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    /// Virtual buttons: a stick pushed past its press threshold in one direction. They
    /// work like any button (actions, gestures, combos), and as gamepad outputs they push
    /// the virtual stick that way.
    LeftStickUp,
    LeftStickDown,
    LeftStickLeft,
    LeftStickRight,
    RightStickUp,
    RightStickDown,
    RightStickLeft,
    RightStickRight,
}

impl Button {
    pub const ALL: [Button; 15] = [
        Button::South,
        Button::East,
        Button::North,
        Button::West,
        Button::LeftBumper,
        Button::RightBumper,
        Button::Select,
        Button::Start,
        Button::Guide,
        Button::LeftStick,
        Button::RightStick,
        Button::DpadUp,
        Button::DpadDown,
        Button::DpadLeft,
        Button::DpadRight,
    ];

    /// The physical buttons plus the stick-direction virtual buttons.
    pub const EVERY: [Button; 23] = [
        Button::South,
        Button::East,
        Button::North,
        Button::West,
        Button::LeftBumper,
        Button::RightBumper,
        Button::Select,
        Button::Start,
        Button::Guide,
        Button::LeftStick,
        Button::RightStick,
        Button::DpadUp,
        Button::DpadDown,
        Button::DpadLeft,
        Button::DpadRight,
        Button::LeftStickUp,
        Button::LeftStickDown,
        Button::LeftStickLeft,
        Button::LeftStickRight,
        Button::RightStickUp,
        Button::RightStickDown,
        Button::RightStickLeft,
        Button::RightStickRight,
    ];

    /// A stick's direction buttons: up, down, left, right.
    pub fn stick_directions(s: Stick) -> [Button; 4] {
        match s {
            Stick::Left => [Button::LeftStickUp, Button::LeftStickDown, Button::LeftStickLeft, Button::LeftStickRight],
            Stick::Right => [Button::RightStickUp, Button::RightStickDown, Button::RightStickLeft, Button::RightStickRight],
        }
    }

    /// For a stick-direction button: its stick and unit direction (y positive is down).
    pub fn stick_direction(self) -> Option<(Stick, (f32, f32))> {
        Some(match self {
            Button::LeftStickUp => (Stick::Left, (0.0, -1.0)),
            Button::LeftStickDown => (Stick::Left, (0.0, 1.0)),
            Button::LeftStickLeft => (Stick::Left, (-1.0, 0.0)),
            Button::LeftStickRight => (Stick::Left, (1.0, 0.0)),
            Button::RightStickUp => (Stick::Right, (0.0, -1.0)),
            Button::RightStickDown => (Stick::Right, (0.0, 1.0)),
            Button::RightStickLeft => (Stick::Right, (-1.0, 0.0)),
            Button::RightStickRight => (Stick::Right, (1.0, 0.0)),
            _ => return None,
        })
    }
}

impl fmt::Display for Button {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Button::South => "A / Cross (South)",
            Button::East => "B / Circle (East)",
            Button::North => "Y / Triangle (North)",
            Button::West => "X / Square (West)",
            Button::LeftBumper => "Left Bumper",
            Button::RightBumper => "Right Bumper",
            Button::Select => "Select / Back",
            Button::Start => "Start",
            Button::Guide => "Guide / Home",
            Button::LeftStick => "Left Stick Click",
            Button::RightStick => "Right Stick Click",
            Button::DpadUp => "D-pad Up",
            Button::DpadDown => "D-pad Down",
            Button::DpadLeft => "D-pad Left",
            Button::DpadRight => "D-pad Right",
            Button::LeftStickUp => "Left Stick Up",
            Button::LeftStickDown => "Left Stick Down",
            Button::LeftStickLeft => "Left Stick Left",
            Button::LeftStickRight => "Left Stick Right",
            Button::RightStickUp => "Right Stick Up",
            Button::RightStickDown => "Right Stick Down",
            Button::RightStickLeft => "Right Stick Left",
            Button::RightStickRight => "Right Stick Right",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stick {
    Left,
    Right,
}

impl fmt::Display for Stick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stick::Left => "Left Stick",
            Stick::Right => "Right Stick",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Trigger {
    Left,
    Right,
}

impl fmt::Display for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Trigger::Left => "Left Trigger",
            Trigger::Right => "Right Trigger",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl MouseButton {
    pub const ALL: [MouseButton; 5] = [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
    ];
}

impl fmt::Display for MouseButton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// What a digital input produces. Keys are evdev names such as `KEY_A` or `KEY_LEFTCTRL`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonAction {
    Disabled,
    Gamepad(Button),
    /// Pressed together, released together (e.g. `["KEY_LEFTCTRL", "KEY_C"]`).
    Keys(Vec<String>),
    Mouse(MouseButton),
    /// One wheel notch on press; keeps scrolling while held (after a short delay).
    Wheel(WheelDirection),
    NextProfile,
    /// Several actions at once, pressed in order and released in reverse.
    Multi(Vec<ButtonAction>),
    /// First press holds the inner action down, the next press releases it.
    Toggle(Toggled),
    /// While held, presses and releases the inner action `rate` times per second.
    Turbo { action: Box<ButtonAction>, rate: f32 },
    /// Plays the macro named `name`: once per press, or looping while held if `repeat`.
    Macro { name: String, #[serde(default)] repeat: bool },
    /// Opens or closes the on-screen overlay (keyboard). While it is open the controller
    /// drives the overlay; holding East closes it.
    ToggleOverlay,
    /// Opens or closes the on-screen numpad (works like the keyboard).
    ToggleNumpad,
    /// Shows the info overlay named here while held (wrap in Toggle to keep it up).
    ShowInfo(String),
    /// Shows the menu named here. As a menu item's action it opens a submenu.
    OpenMenu(String),
    /// While held, the game's layer named here applies on top of the active profile (wrap in
    /// Toggle to keep it on).
    Layer(String),
}

/// A Toggle's inner action, and whether it switches on by itself when the game starts.
/// Written as just the inner action unless it starts on, so configs and packs without that
/// read and write as before.
#[derive(Debug, Clone, PartialEq)]
pub struct Toggled {
    pub action: Box<ButtonAction>,
    /// On when the game starts (its first focus after launching), as if pressed.
    pub start_on: bool,
}

impl Serialize for Toggled {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Full<'a> {
            action: &'a ButtonAction,
            start_on: bool,
        }
        if self.start_on {
            Full { action: &self.action, start_on: true }.serialize(s)
        } else {
            self.action.serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for Toggled {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Full {
                action: Box<ButtonAction>,
                #[serde(default)]
                start_on: bool,
            },
            Plain(Box<ButtonAction>),
        }
        Ok(match Repr::deserialize(d)? {
            Repr::Full { action, start_on } => Toggled { action, start_on },
            Repr::Plain(action) => Toggled { action, start_on: false },
        })
    }
}

impl ButtonAction {
    /// A Toggle of `inner` that starts off.
    pub fn toggle(inner: ButtonAction) -> Self {
        ButtonAction::Toggle(Toggled { action: Box::new(inner), start_on: false })
    }
}

/// An on-screen action menu, usable by every profile of its game (or, when shared, of every
/// game) and opened with `ButtonAction::OpenMenu`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Menu {
    pub name: String,
    pub kind: MenuKind,
    pub items: Vec<MenuItem>,
    /// Button that backs out of the menu (one level for submenus). Defaults to East, or to
    /// Select for a face-button directional menu, where East is one of the slots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel: Option<Button>,
    #[serde(default)]
    pub style: OverlayStyle,
}

impl Menu {
    pub fn cancel_button(&self) -> Button {
        self.cancel.unwrap_or(match self.kind {
            MenuKind::Directional { cluster: Cluster::FaceButtons } => Button::Select,
            _ => Button::East,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MenuItem {
    pub label: String,
    pub action: ButtonAction,
    /// Quick-select button (button menus).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<Button>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuKind {
    /// Shown while the opening input is held: aim `stick` at an item, release to choose it.
    Radial { stick: Stick },
    /// Four slots (up, right, down, left) on the D-pad or face buttons. A slot fires its
    /// action; give it "Open menu" to make it a submenu.
    #[serde(alias = "cascade")]
    Directional { cluster: Cluster },
    /// A list moved through with the D-pad or left stick; A chooses.
    List,
    /// A list whose items can also be chosen directly with their quick-select button.
    Buttons,
    /// A row of items cycled with `controls`; A chooses.
    Carousel { controls: CarouselControls },
}

impl MenuKind {
    pub fn default_for(kind: MenuKindTag) -> Self {
        match kind {
            MenuKindTag::Radial => MenuKind::Radial { stick: Stick::Right },
            MenuKindTag::Directional => MenuKind::Directional { cluster: Cluster::DPad },
            MenuKindTag::List => MenuKind::List,
            MenuKindTag::Buttons => MenuKind::Buttons,
            MenuKindTag::Carousel => MenuKind::Carousel { controls: CarouselControls::Bumpers },
        }
    }

    pub fn tag(self) -> MenuKindTag {
        match self {
            MenuKind::Radial { .. } => MenuKindTag::Radial,
            MenuKind::Directional { .. } => MenuKindTag::Directional,
            MenuKind::List => MenuKindTag::List,
            MenuKind::Buttons => MenuKindTag::Buttons,
            MenuKind::Carousel { .. } => MenuKindTag::Carousel,
        }
    }
}

/// Menu kinds without their settings, for pickers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKindTag {
    Radial,
    Directional,
    List,
    Buttons,
    Carousel,
}

impl MenuKindTag {
    pub const ALL: [MenuKindTag; 5] =
        [MenuKindTag::Radial, MenuKindTag::Directional, MenuKindTag::List, MenuKindTag::Buttons, MenuKindTag::Carousel];
}

impl MenuKindTag {
    /// A one-word name, for summaries.
    pub fn short(self) -> &'static str {
        match self {
            MenuKindTag::Radial => "Radial",
            MenuKindTag::Directional => "Directional",
            MenuKindTag::List => "List",
            MenuKindTag::Buttons => "Button menu",
            MenuKindTag::Carousel => "Carousel",
        }
    }
}

impl fmt::Display for MenuKindTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MenuKindTag::Radial => "Radial (hold, aim, release)",
            MenuKindTag::Directional => "Directional (D-pad / face buttons)",
            MenuKindTag::List => "List",
            MenuKindTag::Buttons => "Button menu (list + quick buttons)",
            MenuKindTag::Carousel => "Carousel",
        })
    }
}

/// Where on screen an overlay sits, relative to the screen so it suits any size or shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenPosition {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    #[default]
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl ScreenPosition {
    /// In reading order, for a 3×3 picker.
    pub const GRID: [ScreenPosition; 9] = [
        ScreenPosition::TopLeft,
        ScreenPosition::TopCenter,
        ScreenPosition::TopRight,
        ScreenPosition::CenterLeft,
        ScreenPosition::Center,
        ScreenPosition::CenterRight,
        ScreenPosition::BottomLeft,
        ScreenPosition::BottomCenter,
        ScreenPosition::BottomRight,
    ];

    /// Column and row in the 3×3 grid: 0 = left/top, 1 = center, 2 = right/bottom.
    pub fn cell(self) -> (usize, usize) {
        let i = Self::GRID.iter().position(|p| *p == self).unwrap_or(4);
        (i % 3, i / 3)
    }
}

impl fmt::Display for ScreenPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ScreenPosition::TopLeft => "Top left",
            ScreenPosition::TopCenter => "Top center",
            ScreenPosition::TopRight => "Top right",
            ScreenPosition::CenterLeft => "Center left",
            ScreenPosition::Center => "Center",
            ScreenPosition::CenterRight => "Center right",
            ScreenPosition::BottomLeft => "Bottom left",
            ScreenPosition::BottomCenter => "Bottom center",
            ScreenPosition::BottomRight => "Bottom right",
        })
    }
}

/// A color (`#rrggbb`) with an opacity (0..1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paint {
    pub color: String,
    pub opacity: f32,
}

impl Paint {
    pub fn new(color: &str, opacity: f32) -> Self {
        Paint { color: color.into(), opacity }
    }

    /// The color as RGB, if `color` is a valid `#rrggbb`.
    pub fn rgb(&self) -> Option<[u8; 3]> {
        parse_hex(&self.color)
    }
}

pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let hex = s.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// How an overlay (a menu, or the on-screen keyboard) looks and where it sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayStyle {
    #[serde(default)]
    pub position: ScreenPosition,
    /// Size relative to the default, 0.5..2.
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default = "default_background")]
    pub background: Paint,
    #[serde(default = "default_items")]
    pub items: Paint,
    #[serde(default = "default_selected")]
    pub selected: Paint,
}

fn default_scale() -> f32 {
    1.0
}

fn default_background() -> Paint {
    Paint::new("#16181c", 0.92)
}

fn default_items() -> Paint {
    Paint::new("#30343c", 0.95)
}

fn default_selected() -> Paint {
    Paint::new("#2f5db0", 1.0)
}

impl Default for OverlayStyle {
    fn default() -> Self {
        OverlayStyle {
            position: ScreenPosition::Center,
            scale: default_scale(),
            background: default_background(),
            items: default_items(),
            selected: default_selected(),
        }
    }
}

impl OverlayStyle {
    /// The on-screen keyboard's default: along the bottom of the screen.
    pub fn keyboard() -> Self {
        OverlayStyle { position: ScreenPosition::BottomCenter, ..OverlayStyle::default() }
    }
}

impl OverlayStyle {
    /// The on-screen numpad's default: out of the way in the bottom-right corner.
    pub fn numpad() -> Self {
        OverlayStyle { position: ScreenPosition::BottomRight, ..OverlayStyle::default() }
    }

    /// A layer's name label: top center, small and see-through.
    pub fn indicator() -> Self {
        OverlayStyle {
            position: ScreenPosition::TopCenter,
            scale: 0.7,
            background: Paint::new("#16181c", 0.7),
            ..OverlayStyle::default()
        }
    }

    /// Info overlays: top right, smaller and see-through so they can stay up during play.
    pub fn info() -> Self {
        OverlayStyle {
            position: ScreenPosition::TopRight,
            scale: 0.8,
            background: Paint::new("#16181c", 0.7),
            ..OverlayStyle::default()
        }
    }
}

fn default_keyboard_style() -> OverlayStyle {
    OverlayStyle::keyboard()
}

fn default_numpad_style() -> OverlayStyle {
    OverlayStyle::numpad()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cluster {
    DPad,
    FaceButtons,
}

impl Cluster {
    /// The four slot buttons: up, right, down, left.
    pub fn slots(self) -> [Button; 4] {
        match self {
            Cluster::DPad => [Button::DpadUp, Button::DpadRight, Button::DpadDown, Button::DpadLeft],
            Cluster::FaceButtons => [Button::North, Button::East, Button::South, Button::West],
        }
    }
}

impl fmt::Display for Cluster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Cluster::DPad => "D-pad",
            Cluster::FaceButtons => "Face buttons",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CarouselControls {
    Bumpers,
    Triggers,
    DPad,
    LeftStick,
    RightStick,
}

impl CarouselControls {
    pub const ALL: [CarouselControls; 5] = [
        CarouselControls::Bumpers,
        CarouselControls::Triggers,
        CarouselControls::DPad,
        CarouselControls::LeftStick,
        CarouselControls::RightStick,
    ];
}

impl fmt::Display for CarouselControls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CarouselControls::Bumpers => "Bumpers (LB / RB)",
            CarouselControls::Triggers => "Triggers (LT / RT)",
            CarouselControls::DPad => "D-pad left / right",
            CarouselControls::LeftStick => "Left stick",
            CarouselControls::RightStick => "Right stick",
        })
    }
}

/// A named sequence of inputs, usable by every profile of its game (or, when shared, of
/// every game).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub steps: Vec<MacroStep>,
}

/// An on-screen panel of text laid out in a grid, e.g. a game's button mappings. Cells may
/// hold `{tokens}` for controller glyphs and live values (see `crate::info`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoOverlay {
    pub name: String,
    /// Shown whenever a profile of its game is active, rather than only by `ShowInfo`.
    #[serde(default)]
    pub always: bool,
    /// Also shown for this many seconds when the game starts (its first focus after
    /// launching), fading out at the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_start: Option<f32>,
    /// Seconds it stays after the action showing it lets go (released, or toggled off),
    /// fading out at the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linger: Option<f32>,
    #[serde(default = "OverlayStyle::info")]
    pub style: OverlayStyle,
    /// Rows of cells; cells line up in columns.
    #[serde(default)]
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacroStep {
    /// Press, hold for `hold_ms`, release.
    Tap { action: ButtonAction, hold_ms: u64 },
    /// Press and keep held (until a matching Release or the macro ends).
    Press(ButtonAction),
    Release(ButtonAction),
    /// Pause, in milliseconds.
    Wait(u64),
    /// Moves a virtual-pad stick to (x, y), -1..1 with y positive down, until another step
    /// moves it; it returns to center when the macro ends.
    Stick { stick: Stick, x: f32, y: f32 },
}

impl MacroStep {
    pub fn action(&self) -> Option<&ButtonAction> {
        match self {
            MacroStep::Tap { action, .. } | MacroStep::Press(action) | MacroStep::Release(action) => Some(action),
            MacroStep::Wait(_) | MacroStep::Stick { .. } => None,
        }
    }

    pub fn action_mut(&mut self) -> Option<&mut ButtonAction> {
        match self {
            MacroStep::Tap { action, .. } | MacroStep::Press(action) | MacroStep::Release(action) => Some(action),
            MacroStep::Wait(_) | MacroStep::Stick { .. } => None,
        }
    }

    /// Time this step takes, in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        match self {
            MacroStep::Tap { hold_ms, .. } => *hold_ms,
            MacroStep::Wait(ms) => *ms,
            MacroStep::Press(_) | MacroStep::Release(_) | MacroStep::Stick { .. } => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WheelDirection {
    Up,
    Down,
    Left,
    Right,
}

impl WheelDirection {
    pub const ALL: [WheelDirection; 4] =
        [WheelDirection::Up, WheelDirection::Down, WheelDirection::Left, WheelDirection::Right];

    /// Notches as (horizontal, vertical); up and right are positive, like REL_WHEEL/HWHEEL.
    pub fn vector(self) -> (f32, f32) {
        match self {
            WheelDirection::Up => (0.0, 1.0),
            WheelDirection::Down => (0.0, -1.0),
            WheelDirection::Left => (-1.0, 0.0),
            WheelDirection::Right => (1.0, 0.0),
        }
    }
}

impl fmt::Display for WheelDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WheelDirection::Up => "Scroll up",
            WheelDirection::Down => "Scroll down",
            WheelDirection::Left => "Scroll left",
            WheelDirection::Right => "Scroll right",
        })
    }
}

impl ButtonAction {
    /// Every key name used by this action, including inside `Multi`.
    /// Wheel directions this action scrolls in while held, including inside `Multi`. Toggle and
    /// Turbo are left out: they press their inner action on their own schedule.
    pub fn wheel_directions(&self) -> Vec<WheelDirection> {
        match self {
            ButtonAction::Wheel(d) => vec![*d],
            ButtonAction::Multi(actions) => actions.iter().flat_map(|a| a.wheel_directions()).collect(),
            _ => Vec::new(),
        }
    }

    pub fn key_names(&self) -> Vec<&String> {
        match self {
            ButtonAction::Keys(keys) => keys.iter().collect(),
            ButtonAction::Multi(actions) => actions.iter().flat_map(|a| a.key_names()).collect(),
            ButtonAction::Toggle(Toggled { action: inner, .. }) | ButtonAction::Turbo { action: inner, .. } => inner.key_names(),
            _ => Vec::new(),
        }
    }

    /// This action and every action nested inside it.
    pub fn walk(&self, f: &mut dyn FnMut(&ButtonAction)) {
        f(self);
        match self {
            ButtonAction::Multi(actions) => actions.iter().for_each(|a| a.walk(f)),
            ButtonAction::Toggle(Toggled { action: inner, .. }) | ButtonAction::Turbo { action: inner, .. } => inner.walk(f),
            _ => {}
        }
    }

    pub fn walk_mut(&mut self, f: &mut dyn FnMut(&mut ButtonAction)) {
        f(self);
        match self {
            ButtonAction::Multi(actions) => actions.iter_mut().for_each(|a| a.walk_mut(f)),
            ButtonAction::Toggle(Toggled { action: inner, .. }) | ButtonAction::Turbo { action: inner, .. } => inner.walk_mut(f),
            _ => {}
        }
    }
}

/// Buttons pressed together that act as their own input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Combo {
    pub buttons: Vec<Button>,
    pub action: ButtonAction,
}

fn default_combo_window_ms() -> u64 {
    60
}

fn default_tap_window_ms() -> u64 {
    250
}

fn default_long_press_ms() -> u64 {
    500
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GestureKind {
    DoubleTap,
    TripleTap,
    LongPress,
}

impl GestureKind {
    pub const ALL: [GestureKind; 3] = [GestureKind::DoubleTap, GestureKind::TripleTap, GestureKind::LongPress];
}

impl fmt::Display for GestureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            GestureKind::DoubleTap => "Double tap",
            GestureKind::TripleTap => "Triple tap",
            GestureKind::LongPress => "Long press",
        })
    }
}

/// Extra actions for one button beyond its normal (single press) action.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Gestures {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub double_tap: Option<ButtonAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triple_tap: Option<ButtonAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_press: Option<ButtonAction>,
}

impl Gestures {
    pub fn get(&self, kind: GestureKind) -> Option<&ButtonAction> {
        match kind {
            GestureKind::DoubleTap => self.double_tap.as_ref(),
            GestureKind::TripleTap => self.triple_tap.as_ref(),
            GestureKind::LongPress => self.long_press.as_ref(),
        }
    }

    pub fn slot(&mut self, kind: GestureKind) -> &mut Option<ButtonAction> {
        match kind {
            GestureKind::DoubleTap => &mut self.double_tap,
            GestureKind::TripleTap => &mut self.triple_tap,
            GestureKind::LongPress => &mut self.long_press,
        }
    }

    pub fn is_empty(&self) -> bool {
        GestureKind::ALL.iter().all(|k| self.get(*k).is_none())
    }

    /// Most presses in a tap sequence that mean something (1 = no multi-tap gestures).
    pub fn max_taps(&self) -> u8 {
        if self.triple_tap.is_some() {
            3
        } else if self.double_tap.is_some() {
            2
        } else {
            1
        }
    }

    /// Action for a sequence of `taps` presses; `None` for 1 (the normal action) or unset ones.
    pub fn for_taps(&self, taps: u8) -> Option<&ButtonAction> {
        match taps {
            2 => self.double_tap.as_ref(),
            3 => self.triple_tap.as_ref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StickAction {
    Disabled,
    Gamepad {
        stick: Stick,
        #[serde(default)]
        invert_y: bool,
    },
    /// `speed` is pixels/second at full deflection.
    Mouse { speed: f32 },
    /// `speed` is wheel notches/second at full deflection.
    Scroll { speed: f32 },
    /// Directional keys, e.g. WASD or arrows.
    Keys {
        up: String,
        down: String,
        left: String,
        right: String,
    },
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StickConfig {
    pub action: StickAction,
    /// Radial deadzone, 0.0..1.0.
    pub deadzone: f32,
    /// Response curve exponent for mouse/scroll (1.0 = linear).
    #[serde(default = "default_curve")]
    pub curve: f32,
    /// Deflection (after the deadzone) at which direction keys and the stick's direction
    /// buttons (e.g. `Button::LeftStickUp`) press.
    #[serde(default = "default_key_threshold")]
    pub key_threshold: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<Zone>,
}

fn default_curve() -> f32 {
    2.0
}

fn default_key_threshold() -> f32 {
    0.5
}

/// Extra action held while a stick's deflection or a trigger's pull is within
/// `min..max` (0.0..1.0). Only active once the input has left its rest position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub min: f32,
    pub max: f32,
    pub action: ButtonAction,
}

/// What a layer shows on screen while it's active.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Indicator {
    /// A small label with the layer's name.
    #[default]
    Name,
    /// One of the game's (or the shared) info overlays.
    Info(String),
    Off,
}

/// A set of overrides on top of whichever of its game's profiles is active, while an input
/// holds it (or a toggle keeps it on). Whatever it doesn't set falls through to the layers
/// below it and then the profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    #[serde(default)]
    pub indicator: Indicator,
    /// How the name label looks, for `Indicator::Name`.
    #[serde(default = "OverlayStyle::indicator")]
    pub indicator_style: OverlayStyle,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub buttons: BTreeMap<Button, ButtonAction>,
    /// Per button, its gestures replace the profile's (empty: none while the layer is on).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gestures: BTreeMap<Button, Gestures>,
    /// Added to the profile's combos; one with the same buttons replaces the profile's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub combos: Vec<Combo>,
    /// The profile's combos (by their buttons) switched off while the layer is on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_combos: Vec<Vec<Button>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_stick: Option<StickConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_stick: Option<StickConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_trigger: Option<TriggerConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_trigger: Option<TriggerConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gyro: Option<GyroConfig>,
}

/// A combo's buttons in a canonical order, to compare combos.
pub fn combo_key(buttons: &[Button]) -> Vec<Button> {
    let mut key = buttons.to_vec();
    key.sort();
    key.dedup();
    key
}

impl Layer {
    pub fn new(name: &str) -> Self {
        Layer {
            name: name.into(),
            indicator: Indicator::Name,
            indicator_style: OverlayStyle::indicator(),
            buttons: BTreeMap::new(),
            gestures: BTreeMap::new(),
            combos: Vec::new(),
            disabled_combos: Vec::new(),
            left_stick: None,
            right_stick: None,
            left_trigger: None,
            right_trigger: None,
            gyro: None,
        }
    }

    /// Puts this layer's overrides on top of `p`.
    pub fn apply(&self, p: &mut Profile) {
        for (b, a) in &self.buttons {
            p.buttons.insert(*b, a.clone());
        }
        for (b, g) in &self.gestures {
            if g.is_empty() {
                p.gestures.remove(b);
            } else {
                p.gestures.insert(*b, g.clone());
            }
        }
        let replaced = |c: &Combo| {
            let key = combo_key(&c.buttons);
            self.disabled_combos.iter().any(|d| combo_key(d) == key) || self.combos.iter().any(|l| combo_key(&l.buttons) == key)
        };
        p.combos.retain(|c| !replaced(c));
        p.combos.extend(self.combos.iter().cloned());
        let overrides = [(&self.left_stick, &mut p.left_stick), (&self.right_stick, &mut p.right_stick)];
        for (layer, profile) in overrides {
            if let Some(s) = layer {
                *profile = s.clone();
            }
        }
        for (layer, profile) in [(&self.left_trigger, &mut p.left_trigger), (&self.right_trigger, &mut p.right_trigger)] {
            if let Some(t) = layer {
                *profile = t.clone();
            }
        }
        if let Some(g) = &self.gyro {
            p.gyro = g.clone();
        }
    }

    /// Every action it sets: buttons, gestures, combos, triggers and zones.
    pub fn actions(&self) -> Vec<&ButtonAction> {
        let mut all: Vec<&ButtonAction> = self.buttons.values().collect();
        all.extend(self.gestures.values().flat_map(|g| GestureKind::ALL.into_iter().filter_map(|k| g.get(k))));
        all.extend(self.combos.iter().map(|c| &c.action));
        for t in [&self.left_trigger, &self.right_trigger].into_iter().flatten() {
            if let TriggerAction::Button { action, .. } = &t.action {
                all.push(action);
            }
            all.extend(t.zones.iter().map(|z| &z.action));
        }
        for s in [&self.left_stick, &self.right_stick].into_iter().flatten() {
            all.extend(s.zones.iter().map(|z| &z.action));
        }
        all
    }

    pub fn actions_mut(&mut self) -> Vec<&mut ButtonAction> {
        let mut all: Vec<&mut ButtonAction> = self.buttons.values_mut().collect();
        for g in self.gestures.values_mut() {
            all.extend([&mut g.double_tap, &mut g.triple_tap, &mut g.long_press].into_iter().filter_map(|s| s.as_mut()));
        }
        all.extend(self.combos.iter_mut().map(|c| &mut c.action));
        for t in [&mut self.left_trigger, &mut self.right_trigger].into_iter().flatten() {
            if let TriggerAction::Button { action, .. } = &mut t.action {
                all.push(action);
            }
            all.extend(t.zones.iter_mut().map(|z| &mut z.action));
        }
        for s in [&mut self.left_stick, &mut self.right_stick].into_iter().flatten() {
            all.extend(s.zones.iter_mut().map(|z| &mut z.action));
        }
        all
    }

    /// How many things it overrides, for summaries.
    pub fn overrides(&self) -> usize {
        self.buttons.len()
            + self.gestures.len()
            + self.combos.len()
            + self.disabled_combos.len()
            + [self.left_stick.is_some(), self.right_stick.is_some(), self.left_trigger.is_some(), self.right_trigger.is_some(), self.gyro.is_some()]
                .into_iter()
                .filter(|o| *o)
                .count()
    }
}

/// Something that can switch gyro on/off or recenter it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroInput {
    Button(Button),
    /// Pulled past halfway.
    LeftTrigger,
    RightTrigger,
}

impl GyroInput {
    pub fn all() -> Vec<GyroInput> {
        let mut all: Vec<GyroInput> = Button::ALL.into_iter().map(GyroInput::Button).collect();
        all.extend([GyroInput::LeftTrigger, GyroInput::RightTrigger]);
        all
    }
}

impl fmt::Display for GyroInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GyroInput::Button(b) => write!(f, "{b}"),
            GyroInput::LeftTrigger => f.write_str("Left Trigger"),
            GyroInput::RightTrigger => f.write_str("Right Trigger"),
        }
    }
}

/// When gyro output is live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroActivation {
    Always,
    WhileHeld(GyroInput),
    /// Off while held: a "clutch" for repositioning the controller without moving the aim.
    UnlessHeld(GyroInput),
    /// Each press switches gyro on or off.
    Toggle(GyroInput),
}

/// Which rotation drives horizontal aim. Pitch always drives vertical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroHorizontal {
    /// Turning the controller left/right like a flashlight.
    Yaw,
    /// Tilting it like a steering wheel.
    Roll,
    /// Both added together, so either motion works.
    YawAndRoll,
}

impl GyroHorizontal {
    pub const ALL: [GyroHorizontal; 3] = [GyroHorizontal::Yaw, GyroHorizontal::Roll, GyroHorizontal::YawAndRoll];
}

impl fmt::Display for GyroHorizontal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            GyroHorizontal::Yaw => "Yaw (turn)",
            GyroHorizontal::Roll => "Roll (tilt)",
            GyroHorizontal::YawAndRoll => "Yaw + roll",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroMode {
    Off,
    /// Rotation moves the mouse; `sensitivity` is pixels per degree turned.
    Mouse { sensitivity: f32 },
    /// Rotation speed deflects a virtual-pad stick (added to the physical stick). Turning at
    /// `full_rate` degrees/second is full deflection; `anti_deadzone` is the smallest
    /// deflection sent, to get past a game's own stick deadzone.
    Stick { stick: Stick, full_rate: f32, anti_deadzone: f32 },
    /// Tilting like a steering wheel moves a stick left/right; `max_angle` degrees from
    /// center is full lock.
    Steering { stick: Stick, max_angle: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GyroConfig {
    pub mode: GyroMode,
    pub horizontal: GyroHorizontal,
    #[serde(default)]
    pub invert_x: bool,
    #[serde(default)]
    pub invert_y: bool,
    pub activation: GyroActivation,
    /// Sets the current tilt as straight ahead (Steering) and clears accumulated motion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recenter: Option<GyroInput>,
    /// Rotation slower than this (degrees/second) is scaled down, hiding jitter and drift.
    pub noise_threshold: f32,
}

impl Default for GyroConfig {
    fn default() -> Self {
        GyroConfig {
            mode: GyroMode::Off,
            horizontal: GyroHorizontal::Yaw,
            invert_x: false,
            invert_y: false,
            activation: GyroActivation::Always,
            recenter: None,
            noise_threshold: 1.0,
        }
    }
}

/// A stick or trigger, for addressing zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Analog {
    Stick(Stick),
    Trigger(Trigger),
}

fn wasd() -> StickAction {
    StickAction::Keys { up: "KEY_W".into(), down: "KEY_S".into(), left: "KEY_A".into(), right: "KEY_D".into() }
}

fn arrows() -> StickAction {
    StickAction::Keys {
        up: "KEY_UP".into(),
        down: "KEY_DOWN".into(),
        left: "KEY_LEFT".into(),
        right: "KEY_RIGHT".into(),
    }
}

impl StickConfig {
    pub fn new(action: StickAction, deadzone: f32, curve: f32) -> Self {
        StickConfig { action, deadzone, curve, key_threshold: default_key_threshold(), zones: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerAction {
    Disabled,
    Gamepad(Trigger),
    /// Acts as a digital button once pulled past `threshold` (0.0..1.0).
    Button { action: ButtonAction, threshold: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "TriggerConfigRepr")]
pub struct TriggerConfig {
    pub action: TriggerAction,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<Zone>,
}

impl From<TriggerAction> for TriggerConfig {
    fn from(action: TriggerAction) -> Self {
        TriggerConfig { action, zones: Vec::new() }
    }
}

/// Accepts both the current `{ action, zones }` table and the older bare `TriggerAction`.
#[derive(Deserialize)]
#[serde(untagged)]
enum TriggerConfigRepr {
    Full {
        action: TriggerAction,
        #[serde(default)]
        zones: Vec<Zone>,
    },
    Legacy(TriggerAction),
}

impl From<TriggerConfigRepr> for TriggerConfig {
    fn from(repr: TriggerConfigRepr) -> Self {
        match repr {
            TriggerConfigRepr::Full { action, zones } => TriggerConfig { action, zones },
            TriggerConfigRepr::Legacy(action) => action.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub buttons: BTreeMap<Button, ButtonAction>,
    pub left_stick: StickConfig,
    pub right_stick: StickConfig,
    pub left_trigger: TriggerConfig,
    pub right_trigger: TriggerConfig,
    #[serde(default)]
    pub combos: Vec<Combo>,
    /// How long a combo member waits for the rest of its combo before acting on its own.
    #[serde(default = "default_combo_window_ms")]
    pub combo_window_ms: u64,
    #[serde(default)]
    pub gestures: BTreeMap<Button, Gestures>,
    /// Longest gap between presses that still continues a double/triple tap.
    #[serde(default = "default_tap_window_ms")]
    pub tap_window_ms: u64,
    /// Hold time before a press counts as a long press.
    #[serde(default = "default_long_press_ms")]
    pub long_press_ms: u64,
    /// Only used by controllers with motion sensors (PlayStation, Switch).
    #[serde(default)]
    pub gyro: GyroConfig,
}

impl Profile {
    /// True if `b` belongs to a combo (2+ buttons), so its own action is held back briefly.
    pub fn in_combo(&self, b: Button) -> bool {
        self.combos.iter().any(|c| c.buttons.len() >= 2 && c.buttons.contains(&b))
    }

    pub fn button(&self, b: Button) -> &ButtonAction {
        self.buttons.get(&b).unwrap_or(&ButtonAction::Disabled)
    }

    /// The button's gestures, if it has any set.
    /// Every top-level action in the profile: buttons, gestures, combos, triggers and zones.
    pub fn actions(&self) -> Vec<&ButtonAction> {
        let mut all: Vec<&ButtonAction> = self.buttons.values().collect();
        all.extend(self.gestures.values().flat_map(|g| GestureKind::ALL.into_iter().filter_map(|k| g.get(k))));
        all.extend(self.combos.iter().map(|c| &c.action));
        for t in [&self.left_trigger, &self.right_trigger] {
            if let TriggerAction::Button { action, .. } = &t.action {
                all.push(action);
            }
            all.extend(t.zones.iter().map(|z| &z.action));
        }
        all.extend(self.left_stick.zones.iter().chain(&self.right_stick.zones).map(|z| &z.action));
        all
    }

    pub fn actions_mut(&mut self) -> Vec<&mut ButtonAction> {
        let mut all: Vec<&mut ButtonAction> = self.buttons.values_mut().collect();
        for g in self.gestures.values_mut() {
            all.extend([&mut g.double_tap, &mut g.triple_tap, &mut g.long_press].into_iter().filter_map(|s| s.as_mut()));
        }
        all.extend(self.combos.iter_mut().map(|c| &mut c.action));
        for t in [&mut self.left_trigger, &mut self.right_trigger] {
            if let TriggerAction::Button { action, .. } = &mut t.action {
                all.push(action);
            }
            all.extend(t.zones.iter_mut().map(|z| &mut z.action));
        }
        for stick in [&mut self.left_stick, &mut self.right_stick] {
            all.extend(stick.zones.iter_mut().map(|z| &mut z.action));
        }
        all
    }

    pub fn gestures(&self, b: Button) -> Option<&Gestures> {
        self.gestures.get(&b).filter(|g| !g.is_empty())
    }

    pub fn set_button(&mut self, b: Button, action: ButtonAction) {
        self.buttons.insert(b, action);
    }

    pub fn stick(&self, s: Stick) -> &StickConfig {
        match s {
            Stick::Left => &self.left_stick,
            Stick::Right => &self.right_stick,
        }
    }

    pub fn stick_mut(&mut self, s: Stick) -> &mut StickConfig {
        match s {
            Stick::Left => &mut self.left_stick,
            Stick::Right => &mut self.right_stick,
        }
    }

    pub fn trigger(&self, t: Trigger) -> &TriggerAction {
        match t {
            Trigger::Left => &self.left_trigger.action,
            Trigger::Right => &self.right_trigger.action,
        }
    }

    pub fn trigger_mut(&mut self, t: Trigger) -> &mut TriggerAction {
        match t {
            Trigger::Left => &mut self.left_trigger.action,
            Trigger::Right => &mut self.right_trigger.action,
        }
    }

    pub fn zones(&self, a: Analog) -> &[Zone] {
        match a {
            Analog::Stick(s) => &self.stick(s).zones,
            Analog::Trigger(Trigger::Left) => &self.left_trigger.zones,
            Analog::Trigger(Trigger::Right) => &self.right_trigger.zones,
        }
    }

    pub fn zones_mut(&mut self, a: Analog) -> &mut Vec<Zone> {
        match a {
            Analog::Stick(s) => &mut self.stick_mut(s).zones,
            Analog::Trigger(Trigger::Left) => &mut self.left_trigger.zones,
            Analog::Trigger(Trigger::Right) => &mut self.right_trigger.zones,
        }
    }

    /// This profile with `layers` on top, oldest first: later ones win.
    pub fn with_layers<'a>(&self, layers: impl IntoIterator<Item = &'a Layer>) -> Profile {
        let mut p = self.clone();
        for layer in layers {
            layer.apply(&mut p);
        }
        p
    }

    /// 1:1 virtual gamepad. Guide cycles profiles.
    pub fn passthrough(name: &str) -> Self {
        let buttons = Button::ALL
            .iter()
            .map(|&b| {
                let action = if b == Button::Guide {
                    ButtonAction::NextProfile
                } else {
                    ButtonAction::Gamepad(b)
                };
                (b, action)
            })
            .collect();
        Profile {
            name: name.into(),
            buttons,
            left_stick: StickConfig::new(StickAction::Gamepad { stick: Stick::Left, invert_y: false }, 0.05, 1.0),
            right_stick: StickConfig::new(StickAction::Gamepad { stick: Stick::Right, invert_y: false }, 0.05, 1.0),
            left_trigger: TriggerAction::Gamepad(Trigger::Left).into(),
            right_trigger: TriggerAction::Gamepad(Trigger::Right).into(),
            combos: Vec::new(),
            combo_window_ms: default_combo_window_ms(),
            gestures: BTreeMap::new(),
            tap_window_ms: default_tap_window_ms(),
            long_press_ms: default_long_press_ms(),
            gyro: GyroConfig::default(),
        }
    }

    /// Keyboard-and-mouse action games (shooters, third-person action): WASD movement with
    /// sprint at full push, mouse look, Mouse 1/2 on the triggers.
    pub fn pc_action(name: &str) -> Self {
        use ButtonAction::*;
        let key = |k: &str| Keys(vec![k.into()]);
        let mut left_stick = StickConfig::new(wasd(), 0.12, 1.0);
        left_stick.key_threshold = 0.25;
        left_stick.zones.push(Zone { min: 0.9, max: 1.0, action: key("KEY_LEFTSHIFT") });
        Profile {
            buttons: BTreeMap::from([
                (Button::South, key("KEY_E")),
                (Button::East, key("KEY_SPACE")),
                (Button::West, key("KEY_R")),
                (Button::North, key("KEY_F")),
                (Button::LeftBumper, key("KEY_Q")),
                (Button::RightBumper, key("KEY_G")),
                (Button::LeftStick, key("KEY_LEFTCTRL")),
                (Button::RightStick, key("KEY_V")),
                (Button::DpadUp, key("KEY_1")),
                (Button::DpadRight, key("KEY_2")),
                (Button::DpadDown, key("KEY_3")),
                (Button::DpadLeft, key("KEY_4")),
                (Button::Start, key("KEY_ESC")),
                (Button::Select, key("KEY_TAB")),
                (Button::Guide, NextProfile),
            ]),
            left_stick,
            right_stick: StickConfig::new(StickAction::Mouse { speed: 1600.0 }, 0.1, 2.0),
            left_trigger: TriggerAction::Button { action: Mouse(MouseButton::Right), threshold: 0.3 }.into(),
            right_trigger: TriggerAction::Button { action: Mouse(MouseButton::Left), threshold: 0.3 }.into(),
            // Gyro aiming while aiming down sights (LT), on pads that have a gyro.
            gyro: GyroConfig {
                mode: GyroMode::Mouse { sensitivity: 15.0 },
                activation: GyroActivation::WhileHeld(GyroInput::LeftTrigger),
                ..GyroConfig::default()
            },
            ..Profile::passthrough(name)
        }
    }

    /// Mouse-driven strategy games: pointer on the left stick, arrow-key camera pan on the
    /// right, zoom on the stick clicks, held Ctrl/Shift modifiers on the bumpers and control
    /// groups on the D-pad.
    pub fn strategy(name: &str) -> Self {
        use ButtonAction::*;
        let key = |k: &str| Keys(vec![k.into()]);
        let mut right_stick = StickConfig::new(arrows(), 0.15, 1.0);
        right_stick.key_threshold = 0.35;
        Profile {
            buttons: BTreeMap::from([
                (Button::South, Mouse(MouseButton::Left)),
                (Button::East, Mouse(MouseButton::Right)),
                (Button::West, Mouse(MouseButton::Middle)),
                (Button::North, key("KEY_SPACE")),
                (Button::LeftBumper, key("KEY_LEFTCTRL")),
                (Button::RightBumper, key("KEY_LEFTSHIFT")),
                // Camera zoom, which strategy games put on the wheel.
                (Button::LeftStick, Wheel(WheelDirection::Up)),
                (Button::RightStick, Wheel(WheelDirection::Down)),
                (Button::DpadUp, key("KEY_1")),
                (Button::DpadRight, key("KEY_2")),
                (Button::DpadDown, key("KEY_3")),
                (Button::DpadLeft, key("KEY_4")),
                (Button::Start, key("KEY_ESC")),
                (Button::Select, key("KEY_TAB")),
                (Button::Guide, NextProfile),
            ]),
            left_stick: StickConfig::new(StickAction::Mouse { speed: 1400.0 }, 0.12, 2.2),
            right_stick,
            left_trigger: TriggerAction::Button { action: key("KEY_LEFTALT"), threshold: 0.4 }.into(),
            // Hold and move the pointer to drag a selection box.
            right_trigger: TriggerAction::Button { action: Mouse(MouseButton::Left), threshold: 0.4 }.into(),
            ..Profile::passthrough(name)
        }
    }

    /// Keyboard-only retro games, indie platformers and emulators: arrows plus Z/X/C/V.
    pub fn platformer(name: &str) -> Self {
        use ButtonAction::*;
        let key = |k: &str| Keys(vec![k.into()]);
        let mut left_stick = StickConfig::new(arrows(), 0.15, 1.0);
        left_stick.key_threshold = 0.4;
        Profile {
            buttons: BTreeMap::from([
                (Button::South, key("KEY_Z")),
                (Button::East, key("KEY_X")),
                (Button::West, key("KEY_C")),
                (Button::North, key("KEY_V")),
                (Button::LeftBumper, key("KEY_A")),
                (Button::RightBumper, key("KEY_S")),
                (Button::LeftStick, Disabled),
                (Button::RightStick, Disabled),
                (Button::DpadUp, key("KEY_UP")),
                (Button::DpadDown, key("KEY_DOWN")),
                (Button::DpadLeft, key("KEY_LEFT")),
                (Button::DpadRight, key("KEY_RIGHT")),
                (Button::Start, key("KEY_ENTER")),
                (Button::Select, key("KEY_ESC")),
                (Button::Guide, NextProfile),
            ]),
            left_stick,
            right_stick: StickConfig::new(StickAction::Disabled, 0.15, 1.0),
            left_trigger: TriggerAction::Button { action: key("KEY_LEFTSHIFT"), threshold: 0.5 }.into(),
            right_trigger: TriggerAction::Button { action: key("KEY_SPACE"), threshold: 0.5 }.into(),
            ..Profile::passthrough(name)
        }
    }

    /// Desktop navigation: left stick = mouse, right stick = scroll.
    pub fn desktop(name: &str) -> Self {
        use ButtonAction::*;
        let key = |k: &str| Keys(vec![k.into()]);
        let buttons = BTreeMap::from([
            (Button::South, Mouse(MouseButton::Left)),
            (Button::East, Mouse(MouseButton::Right)),
            (Button::North, key("KEY_ESC")),
            (Button::West, Mouse(MouseButton::Middle)),
            (Button::LeftBumper, Mouse(MouseButton::Back)),
            (Button::RightBumper, Mouse(MouseButton::Forward)),
            (Button::Select, Keys(vec!["KEY_LEFTMETA".into()])),
            (Button::Start, key("KEY_ENTER")),
            (Button::Guide, NextProfile),
            (Button::LeftStick, Disabled),
            (Button::RightStick, Disabled),
            (Button::DpadUp, key("KEY_UP")),
            (Button::DpadDown, key("KEY_DOWN")),
            (Button::DpadLeft, key("KEY_LEFT")),
            (Button::DpadRight, key("KEY_RIGHT")),
        ]);
        Profile {
            name: name.into(),
            buttons,
            left_stick: StickConfig::new(StickAction::Mouse { speed: 1200.0 }, 0.12, 2.0),
            right_stick: StickConfig::new(StickAction::Scroll { speed: 15.0 }, 0.15, 2.0),
            left_trigger: TriggerAction::Button {
                action: Keys(vec!["KEY_LEFTSHIFT".into()]),
                threshold: 0.5,
            }
            .into(),
            right_trigger: TriggerAction::Button {
                action: Mouse(MouseButton::Left),
                threshold: 0.5,
            }
            .into(),
            combos: vec![Combo {
                buttons: vec![Button::LeftBumper, Button::RightBumper],
                action: Keys(vec!["KEY_LEFTALT".into(), "KEY_TAB".into()]),
            }],
            combo_window_ms: default_combo_window_ms(),
            // Hold Guide for the on-screen keyboard.
            gestures: BTreeMap::from([(
                Button::Guide,
                Gestures { long_press: Some(ToggleOverlay), ..Gestures::default() },
            )]),
            tap_window_ms: default_tap_window_ms(),
            long_press_ms: default_long_press_ms(),
            gyro: GyroConfig::default(),
        }
    }
}

/// What a per-game rule compares against the focused window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// Executable file name, e.g. `factorio` or (Wine/Proton) `eldenring.exe`.
    Executable,
    SteamAppId,
    WindowClass,
}

impl RuleKind {
    pub const ALL: [RuleKind; 3] = [RuleKind::Executable, RuleKind::SteamAppId, RuleKind::WindowClass];
}

impl fmt::Display for RuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RuleKind::Executable => "Executable",
            RuleKind::SteamAppId => "Steam App ID",
            RuleKind::WindowClass => "Window class",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub kind: RuleKind,
    pub value: String,
    /// One of its game's profiles.
    pub profile: String,
    /// Off when an imported game's rule for the same window took over.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

impl Rule {
    pub fn new(kind: RuleKind, value: impl Into<String>, profile: impl Into<String>) -> Self {
        Rule { kind, value: value.into(), profile: profile.into(), enabled: true }
    }

    /// Same window match (kind and value), whichever profile it picks.
    pub fn same_match(&self, other: &Rule) -> bool {
        self.kind == other.kind && self.value.trim().eq_ignore_ascii_case(other.value.trim())
    }
}

/// A profile, by its game (`None`: General) and name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ProfileRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    pub profile: String,
}

impl ProfileRef {
    pub fn new(game: Option<&str>, profile: &str) -> Self {
        ProfileRef { game: game.map(str::to_string), profile: profile.to_string() }
    }
}

impl fmt::Display for ProfileRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.game {
            Some(game) => write!(f, "{game} › {}", self.profile),
            None => f.write_str(&self.profile),
        }
    }
}

/// Per-game profile switching. The rules themselves belong to games.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoSwitch {
    pub enabled: bool,
    /// Profile for windows no rule matches; `None` leaves the current profile alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<ProfileRef>,
}

impl Default for AutoSwitch {
    fn default() -> Self {
        AutoSwitch { enabled: true, default_profile: None }
    }
}

/// A pack this game was made from, and how to tell whether it has been edited since.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Origin {
    pub id: String,
    /// The pack's own name, which the game may have been renamed from.
    #[serde(default)]
    pub name: String,
    pub version: String,
    /// From the built-in library rather than a file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub library: bool,
    /// Content hash of each item at import (see `crate::pack::item_hashes`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hashes: BTreeMap<String, String>,
}

/// Another pack, credited by a fork.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackRef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub version: String,
}

/// What a game's next export says about itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackInfo {
    /// Empty until the first export.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub made_with: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<PackRef>,
}

impl PackInfo {
    fn is_empty(&self) -> bool {
        *self == PackInfo::default()
    }
}

/// A game: its profiles, the macros, menus and info overlays they use, and the rules that
/// switch to it. General is a game too, with no rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub name: String,
    #[serde(default, skip_serializing_if = "PackInfo::is_empty")]
    pub pack: PackInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    pub profiles: Vec<Profile>,
    /// Mapped with `ButtonAction::Macro`.
    #[serde(default)]
    pub macros: Vec<Macro>,
    /// Opened with `ButtonAction::OpenMenu`.
    #[serde(default)]
    pub menus: Vec<Menu>,
    /// Shown by `ButtonAction::ShowInfo`, or always while the game is active.
    #[serde(default, rename = "info_overlays")]
    pub info: Vec<InfoOverlay>,
    /// Held or toggled on with `ButtonAction::Layer`, over whichever profile is active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
}

impl Game {
    pub fn new(name: &str, profiles: Vec<Profile>) -> Self {
        Game {
            name: name.into(),
            pack: PackInfo::default(),
            origin: None,
            rules: Vec::new(),
            profiles,
            macros: Vec::new(),
            menus: Vec::new(),
            info: Vec::new(),
            layers: Vec::new(),
        }
    }

    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// Points every reference to the macro, menu or info overlay `old` (of `kind`) at `new`:
    /// in profiles, menu items and macro steps.
    pub fn rename_refs(&mut self, kind: ItemKind, old: &str, new: &str) {
        rename_in(&mut self.profiles, &mut self.menus, &mut self.macros, &mut self.layers, kind, old, new);
    }

    /// The names of its items of one kind.
    pub fn names(&self, kind: ItemKind) -> Vec<&str> {
        match kind {
            ItemKind::Layer => self.layers.iter().map(|l| l.name.as_str()).collect(),
            _ => item_names(&self.macros, &self.menus, &self.info, kind),
        }
    }

    /// Renames its item `old` of `kind` to `new`, without following references (see
    /// [`Game::rename_refs`]).
    pub fn rename_item(&mut self, kind: ItemKind, old: &str, new: &str) {
        let new = new.to_string();
        match kind {
            ItemKind::Macro => self.macros.iter_mut().filter(|m| m.name == old).for_each(|m| m.name = new.clone()),
            ItemKind::Menu => self.menus.iter_mut().filter(|m| m.name == old).for_each(|m| m.name = new.clone()),
            ItemKind::Info => self.info.iter_mut().filter(|o| o.name == old).for_each(|o| o.name = new.clone()),
            ItemKind::Layer => self.layers.iter_mut().filter(|l| l.name == old).for_each(|l| l.name = new.clone()),
        }
    }

    /// Its layers named in `active` (oldest first), skipping names it has none of.
    pub fn layers_named<'a>(&'a self, active: &'a [String]) -> impl Iterator<Item = &'a Layer> {
        active.iter().filter_map(|n| self.layers.iter().find(|l| &l.name == n))
    }
}

/// Macros, menus and info overlays every profile of every game can use.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Shared {
    #[serde(default)]
    pub macros: Vec<Macro>,
    #[serde(default)]
    pub menus: Vec<Menu>,
    #[serde(default, rename = "info_overlays")]
    pub info: Vec<InfoOverlay>,
}

impl Shared {
    /// Like [`Game::rename_refs`], within the shared items.
    pub fn rename_refs(&mut self, kind: ItemKind, old: &str, new: &str) {
        rename_in(&mut [], &mut self.menus, &mut self.macros, &mut [], kind, old, new);
    }

    pub fn names(&self, kind: ItemKind) -> Vec<&str> {
        item_names(&self.macros, &self.menus, &self.info, kind)
    }
}

fn item_names<'a>(macros: &'a [Macro], menus: &'a [Menu], info: &'a [InfoOverlay], kind: ItemKind) -> Vec<&'a str> {
    match kind {
        ItemKind::Macro => macros.iter().map(|m| m.name.as_str()).collect(),
        ItemKind::Menu => menus.iter().map(|m| m.name.as_str()).collect(),
        ItemKind::Info => info.iter().map(|o| o.name.as_str()).collect(),
        // Layers are always a game's own, never shared.
        ItemKind::Layer => Vec::new(),
    }
}

/// `base` if it's free, else `base (2)`, `base (3)`, …: how an item or game is renamed when
/// its name is taken.
pub fn free_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    (2..).map(|i| format!("{base} ({i})")).find(|n| !taken(n)).unwrap()
}

/// The kinds of named item a profile refers to. Layers are never shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ItemKind {
    Macro,
    Menu,
    Info,
    Layer,
}

impl ItemKind {
    pub const ALL: [ItemKind; 4] = [ItemKind::Macro, ItemKind::Menu, ItemKind::Info, ItemKind::Layer];

    pub fn noun(self) -> &'static str {
        match self {
            ItemKind::Macro => "macro",
            ItemKind::Menu => "menu",
            ItemKind::Info => "info overlay",
            ItemKind::Layer => "layer",
        }
    }

    /// The name `action` refers to, if it's a reference to this kind.
    pub fn name_in(self, action: &ButtonAction) -> Option<&String> {
        match (self, action) {
            (ItemKind::Macro, ButtonAction::Macro { name, .. })
            | (ItemKind::Menu, ButtonAction::OpenMenu(name))
            | (ItemKind::Info, ButtonAction::ShowInfo(name))
            | (ItemKind::Layer, ButtonAction::Layer(name)) => Some(name),
            _ => None,
        }
    }

    fn name_in_mut(self, action: &mut ButtonAction) -> Option<&mut String> {
        match (self, action) {
            (ItemKind::Macro, ButtonAction::Macro { name, .. })
            | (ItemKind::Menu, ButtonAction::OpenMenu(name))
            | (ItemKind::Info, ButtonAction::ShowInfo(name))
            | (ItemKind::Layer, ButtonAction::Layer(name)) => Some(name),
            _ => None,
        }
    }
}

fn rename_in(
    profiles: &mut [Profile],
    menus: &mut [Menu],
    macros: &mut [Macro],
    layers: &mut [Layer],
    kind: ItemKind,
    old: &str,
    new: &str,
) {
    let mut follow = |a: &mut ButtonAction| {
        a.walk_mut(&mut |a| {
            if let Some(name) = kind.name_in_mut(a)
                && name == old
            {
                *name = new.to_string();
            }
        })
    };
    for p in profiles {
        p.actions_mut().into_iter().for_each(&mut follow);
    }
    for m in menus {
        m.items.iter_mut().for_each(|item| follow(&mut item.action));
    }
    for m in macros {
        m.steps.iter_mut().filter_map(MacroStep::action_mut).for_each(&mut follow);
    }
    for l in layers.iter_mut() {
        l.actions_mut().into_iter().for_each(&mut follow);
    }
    // An info overlay used as a layer's indicator follows its rename too.
    if kind == ItemKind::Info {
        for l in layers.iter_mut() {
            if let Indicator::Info(name) = &mut l.indicator
                && name == old
            {
                *name = new.to_string();
            }
        }
    }
}

/// What the active profile can use: its game's items first, then shared ones.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scope {
    pub macros: Vec<Macro>,
    pub menus: Vec<Menu>,
    pub info: Vec<InfoOverlay>,
    /// The game's own (layers are never shared).
    pub layers: Vec<Layer>,
}

/// Like [`Scope`], borrowed: what a game's profiles (or the shared items) can refer to.
#[derive(Debug, Clone, Default)]
pub struct ScopeRef<'a> {
    pub macros: Vec<&'a Macro>,
    pub menus: Vec<&'a Menu>,
    pub info: Vec<&'a InfoOverlay>,
    pub layers: Vec<&'a Layer>,
}

impl ScopeRef<'_> {
    pub fn names(&self, kind: ItemKind) -> Vec<&str> {
        match kind {
            ItemKind::Macro => self.macros.iter().map(|m| m.name.as_str()).collect(),
            ItemKind::Menu => self.menus.iter().map(|m| m.name.as_str()).collect(),
            ItemKind::Info => self.info.iter().map(|o| o.name.as_str()).collect(),
            ItemKind::Layer => self.layers.iter().map(|l| l.name.as_str()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub enabled: bool,
    #[serde(default)]
    pub active: ProfileRef,
    /// Device names the daemon should leave alone.
    #[serde(default)]
    pub ignored_devices: Vec<String>,
    #[serde(default)]
    pub auto_switch: AutoSwitch,
    /// Gyro drift (degrees/second per raw sensor axis) measured by "Calibrate gyro", per controller.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gyro_calibration: BTreeMap<String, [f32; 3]>,
    #[serde(default = "default_keyboard_style")]
    pub keyboard_style: OverlayStyle,
    #[serde(default = "default_numpad_style")]
    pub numpad_style: OverlayStyle,
    /// Whose button glyphs info overlays use when the controller in use isn't recognized.
    #[serde(default)]
    pub info_glyphs: crate::info::PadFamily,
    /// Profiles that aren't for a particular game (desktop, plain gamepad).
    pub general: Game,
    #[serde(default)]
    pub shared: Shared,
    #[serde(default)]
    pub games: Vec<Game>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            active: ProfileRef::new(None, "Gamepad"),
            ignored_devices: Vec::new(),
            auto_switch: AutoSwitch::default(),
            gyro_calibration: BTreeMap::new(),
            keyboard_style: OverlayStyle::keyboard(),
            numpad_style: OverlayStyle::numpad(),
            info_glyphs: crate::info::PadFamily::default(),
            general: Game::new("General", vec![Profile::passthrough("Gamepad"), Profile::desktop("Desktop")]),
            shared: Shared::default(),
            games: Vec::new(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("controller_app")
            .join("config.toml")
    }

    /// Loads the config, writing the default one if none exists yet. A config from before
    /// games existed is converted (see [`Config::from_legacy`]), keeping the original as
    /// `config.toml.old` (or `.old.2`, … if that's taken).
    pub fn load() -> Result<Self> {
        let path = Self::path();
        if !path.exists() {
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        match toml::from_str(&text) {
            Ok(config) => Ok(config),
            Err(e) => {
                let old_format = toml::from_str::<toml::Table>(&text)
                    .is_ok_and(|t| t.contains_key("profiles") && !t.contains_key("general"));
                if !old_format {
                    return Err(e).with_context(|| format!("parsing {}", path.display()));
                }
                let config = Config::from_legacy(&text).with_context(|| format!("converting {}", path.display()))?;
                let backup = (1..)
                    .map(|i| if i == 1 { path.with_extension("toml.old") } else { path.with_extension(format!("toml.old.{i}")) })
                    .find(|p| !p.exists())
                    .unwrap();
                std::fs::copy(&path, &backup).with_context(|| format!("keeping a copy at {}", backup.display()))?;
                config.save()?;
                Ok(config)
            }
        }
    }

    /// Converts a config from before games existed. Each profile an auto-switch rule points
    /// to becomes a game of its own (named after it, with its rules); the other profiles go to
    /// General. Macros, menus and info overlays shared by all profiles become shared items;
    /// one limited to some profiles goes with them if they all ended up in the same game (or
    /// General), and is shared otherwise.
    pub fn from_legacy(text: &str) -> Result<Config> {
        #[derive(Deserialize)]
        struct Legacy {
            #[serde(default = "yes")]
            enabled: bool,
            #[serde(default)]
            active_profile: String,
            #[serde(default)]
            ignored_devices: Vec<String>,
            #[serde(default)]
            auto_switch: LegacyAutoSwitch,
            #[serde(default)]
            gyro_calibration: BTreeMap<String, [f32; 3]>,
            #[serde(default = "default_keyboard_style")]
            keyboard_style: OverlayStyle,
            #[serde(default = "default_numpad_style")]
            numpad_style: OverlayStyle,
            #[serde(default)]
            info_glyphs: crate::info::PadFamily,
            #[serde(default)]
            macros: Vec<toml::Table>,
            #[serde(default)]
            menus: Vec<toml::Table>,
            #[serde(default)]
            info_overlays: Vec<toml::Table>,
            profiles: Vec<Profile>,
        }
        #[derive(Deserialize)]
        struct LegacyAutoSwitch {
            #[serde(default = "yes")]
            enabled: bool,
            #[serde(default)]
            default_profile: Option<String>,
            #[serde(default)]
            rules: Vec<Rule>,
        }
        impl Default for LegacyAutoSwitch {
            fn default() -> Self {
                LegacyAutoSwitch { enabled: true, default_profile: None, rules: Vec::new() }
            }
        }

        let old: Legacy = toml::from_str(text)?;
        let ruled = |name: &str| old.auto_switch.rules.iter().any(|r| r.profile == name);
        // Where each old profile goes: its own game, or General (`None`).
        let home = |name: &str| -> Option<Option<String>> {
            old.profiles.iter().any(|p| p.name == name).then(|| ruled(name).then(|| name.to_string()))
        };

        let mut config = Config {
            enabled: old.enabled,
            active: ProfileRef::default(),
            ignored_devices: old.ignored_devices,
            auto_switch: AutoSwitch { enabled: old.auto_switch.enabled, default_profile: None },
            gyro_calibration: old.gyro_calibration,
            keyboard_style: old.keyboard_style,
            numpad_style: old.numpad_style,
            info_glyphs: old.info_glyphs,
            general: Game::new("General", Vec::new()),
            shared: Shared::default(),
            games: Vec::new(),
        };
        for p in &old.profiles {
            if ruled(&p.name) {
                let mut game = Game::new(&p.name, vec![p.clone()]);
                game.rules = old.auto_switch.rules.iter().filter(|r| r.profile == p.name).cloned().collect();
                config.games.push(game);
            } else {
                config.general.profiles.push(p.clone());
            }
        }
        if config.general.profiles.is_empty() {
            let name = free_name("Gamepad", |n| old.profiles.iter().any(|p| p.name == n));
            config.general.profiles.push(Profile::passthrough(&name));
        }

        // Each item goes where all of its profiles went, or is shared.
        let place = |mut table: toml::Table| -> (Option<Option<String>>, toml::Table) {
            let homes: BTreeSet<Option<String>> = match table.remove("profiles") {
                None => BTreeSet::new(),
                Some(list) => list
                    .as_array()
                    .map(|l| l.iter().filter_map(|v| v.as_str()).filter_map(&home).collect())
                    .unwrap_or_default(),
            };
            let target = (homes.len() == 1).then(|| homes.into_iter().next().unwrap());
            (target, table)
        };
        for table in old.macros {
            let (target, table) = place(table);
            let item: Macro = table.try_into()?;
            match target {
                Some(game) => config.game_mut(game.as_deref()).unwrap().macros.push(item),
                None => config.shared.macros.push(item),
            }
        }
        for table in old.menus {
            let (target, table) = place(table);
            let item: Menu = table.try_into()?;
            match target {
                Some(game) => config.game_mut(game.as_deref()).unwrap().menus.push(item),
                None => config.shared.menus.push(item),
            }
        }
        for table in old.info_overlays {
            let (target, table) = place(table);
            let item: InfoOverlay = table.try_into()?;
            match target {
                Some(game) => config.game_mut(game.as_deref()).unwrap().info.push(item),
                None => config.shared.info.push(item),
            }
        }

        let at = |name: &str| home(name).map(|game| ProfileRef { game, profile: name.to_string() });
        config.active = at(&old.active_profile).unwrap_or_else(|| config.fallback_profile());
        config.auto_switch.default_profile = old.auto_switch.default_profile.as_deref().and_then(at);
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write-then-rename so the daemon never reads a half-written file.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// The game named `game`, or General for `None`.
    pub fn game(&self, game: Option<&str>) -> Option<&Game> {
        match game {
            None => Some(&self.general),
            Some(name) => self.games.iter().find(|g| g.name == name),
        }
    }

    pub fn game_mut(&mut self, game: Option<&str>) -> Option<&mut Game> {
        match game {
            None => Some(&mut self.general),
            Some(name) => self.games.iter_mut().find(|g| g.name == name),
        }
    }

    /// General, then every game, each with its `ProfileRef` game part.
    pub fn all_games(&self) -> impl Iterator<Item = (Option<&str>, &Game)> {
        std::iter::once((None, &self.general)).chain(self.games.iter().map(|g| (Some(g.name.as_str()), g)))
    }

    pub fn profile(&self, at: &ProfileRef) -> Option<&Profile> {
        self.game(at.game.as_deref())?.profile(&at.profile)
    }

    /// The active profile's game, or General if it's gone.
    pub fn active_game(&self) -> &Game {
        self.game(self.active.game.as_deref())
            .filter(|g| !g.profiles.is_empty())
            .unwrap_or(&self.general)
    }

    /// The active profile, or the first one of its game (or of General) if it's gone.
    pub fn active(&self) -> Option<&Profile> {
        let game = self.active_game();
        game.profile(&self.active.profile).or(game.profiles.first())
    }

    /// Where the active profile really is, after the fallbacks of [`Config::active`].
    pub fn active_ref(&self) -> ProfileRef {
        let game = self.active_game();
        let name = self.active().map(|p| p.name.as_str()).unwrap_or_default();
        let game = (!std::ptr::eq(game, &self.general)).then_some(game.name.as_str());
        ProfileRef::new(game, name)
    }

    /// Where to go when the active profile disappears (e.g. its game was deleted): the
    /// auto-switch default if it still exists, else General's first profile.
    pub fn fallback_profile(&self) -> ProfileRef {
        let default = self.auto_switch.default_profile.clone().filter(|d| self.profile(d).is_some());
        default.unwrap_or_else(|| ProfileRef::new(None, self.general.profiles.first().map_or("", |p| &p.name)))
    }

    /// Settles which profile is active in this config, which replaces one where `previous`
    /// was: still `previous` if it exists, else this config's own `active` (where the editor
    /// says a renamed profile or game went), else the fallback (it was deleted).
    pub fn carry_active(&mut self, previous: &ProfileRef) {
        self.active = if self.profile(previous).is_some() {
            previous.clone()
        } else if self.profile(&self.active).is_some() {
            self.active.clone()
        } else {
            self.fallback_profile()
        };
    }

    /// The next profile of the active game, wrapping around.
    pub fn next_profile(&self) -> Option<ProfileRef> {
        let at = self.active_ref();
        let profiles = &self.active_game().profiles;
        let idx = profiles.iter().position(|p| p.name == at.profile)?;
        let next = &profiles[(idx + 1) % profiles.len()];
        Some(ProfileRef { profile: next.name.clone(), ..at })
    }

    /// The items the active profile can use: its game's first, then shared ones it doesn't
    /// shadow.
    pub fn scope(&self) -> Scope {
        let s = self.scope_of(Some(self.active_game()));
        Scope {
            macros: s.macros.into_iter().cloned().collect(),
            menus: s.menus.into_iter().cloned().collect(),
            info: s.info.into_iter().cloned().collect(),
            layers: s.layers.into_iter().cloned().collect(),
        }
    }

    /// What a game's profiles and items can use: its own items, then the shared ones it
    /// doesn't shadow. For `None`, what shared items can use: only each other.
    pub fn scope_of<'a>(&'a self, game: Option<&'a Game>) -> ScopeRef<'a> {
        fn merge<'a, T>(own: &'a [T], shared: &'a [T], name: impl Fn(&T) -> &str) -> Vec<&'a T> {
            let mut all: Vec<&T> = own.iter().collect();
            all.extend(shared.iter().filter(|s| !own.iter().any(|o| name(o) == name(s))));
            all
        }
        let (macros, menus, info) = match game {
            Some(g) => (g.macros.as_slice(), g.menus.as_slice(), g.info.as_slice()),
            None => (&[][..], &[][..], &[][..]),
        };
        ScopeRef {
            macros: merge(macros, &self.shared.macros, |m| &m.name),
            menus: merge(menus, &self.shared.menus, |m| &m.name),
            info: merge(info, &self.shared.info, |o| &o.name),
            layers: game.map(|g| g.layers.iter().collect()).unwrap_or_default(),
        }
    }

    /// Finds a profile by name alone (for the command line): in the active game, then
    /// General, then the first game that has one.
    pub fn find_profile(&self, name: &str) -> Option<ProfileRef> {
        let active = self.active_ref();
        let in_active = self.game(active.game.as_deref()).and_then(|g| g.profile(name)).map(|_| active.game.clone());
        let anywhere = || self.all_games().find(|(_, g)| g.profile(name).is_some()).map(|(id, _)| id.map(str::to_string));
        in_active.or_else(anywhere).map(|game| ProfileRef { game, profile: name.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_roundtrips_through_toml() {
        let config = Config::default();
        let text = toml::to_string_pretty(&config).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn multi_and_combos_roundtrip_through_toml() {
        let mut config = Config::default();
        config.general.profiles[0].set_button(
            Button::South,
            ButtonAction::Multi(vec![
                ButtonAction::Gamepad(Button::South),
                ButtonAction::Keys(vec!["KEY_X".into()]),
                ButtonAction::Mouse(MouseButton::Left),
            ]),
        );
        config.general.profiles[0].combos.push(Combo {
            buttons: vec![Button::Select, Button::Start],
            action: ButtonAction::NextProfile,
        });
        config.general.profiles[0].gestures.insert(
            Button::North,
            Gestures {
                double_tap: Some(ButtonAction::Keys(vec!["KEY_F5".into()])),
                triple_tap: None,
                long_press: Some(ButtonAction::NextProfile),
            },
        );
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn zones_roundtrip_and_legacy_trigger_format_loads() {
        let mut config = Config::default();
        let p = &mut config.general.profiles[0];
        p.left_stick.zones.push(Zone { min: 0.0, max: 0.75, action: ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into()]) });
        p.right_trigger.zones.push(Zone { min: 0.95, max: 1.0, action: ButtonAction::Mouse(MouseButton::Left) });
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);

        // Before zones existed, a trigger was stored as a bare TriggerAction.
        let mut value = toml::Value::try_from(Config::default()).unwrap();
        for profile in value["general"]["profiles"].as_array_mut().unwrap() {
            let table = profile.as_table_mut().unwrap();
            let action = table["left_trigger"]["action"].clone();
            table.insert("left_trigger".into(), action);
        }
        let config: Config = toml::from_str(&toml::to_string(&value).unwrap()).unwrap();
        assert_eq!(config.general.profiles[0].left_trigger, TriggerAction::Gamepad(Trigger::Left).into());
        assert_eq!(config.general.profiles[1].trigger(Trigger::Left), Config::default().general.profiles[1].trigger(Trigger::Left));
    }

    #[test]
    fn old_configs_without_combos_still_load() {
        let mut value = toml::Value::try_from(Config::default()).unwrap();
        for profile in value["general"]["profiles"].as_array_mut().unwrap() {
            let table = profile.as_table_mut().unwrap();
            table.remove("combos");
            table.remove("combo_window_ms");
            table.remove("gestures");
            table.remove("tap_window_ms");
            table.remove("long_press_ms");
        }
        let config: Config = toml::from_str(&toml::to_string(&value).unwrap()).unwrap();
        assert!(config.general.profiles[0].combos.is_empty());
        assert_eq!(config.general.profiles[0].combo_window_ms, 60);
    }

    #[test]
    fn auto_switch_roundtrips_and_is_optional() {
        let mut config = Config::default();
        config.auto_switch.default_profile = Some(ProfileRef::new(None, "Desktop"));
        let mut game = Game::new("Elden Ring", vec![Profile::pc_action("Gameplay")]);
        game.rules.push(Rule::new(RuleKind::SteamAppId, "1245620", "Gameplay"));
        game.rules.push(Rule { enabled: false, ..Rule::new(RuleKind::Executable, "eldenring.exe", "Gameplay") });
        config.games.push(game);
        let text = toml::to_string_pretty(&config).unwrap();
        assert_eq!(text.matches("enabled = false").count(), 1, "rules only note being off");
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(config, back);

        let mut value = toml::Value::try_from(Config::default()).unwrap();
        value.as_table_mut().unwrap().remove("auto_switch");
        let old: Config = toml::from_str(&toml::to_string(&value).unwrap()).unwrap();
        assert_eq!(old.auto_switch, AutoSwitch::default());
    }

    fn templates() -> Vec<Profile> {
        vec![
            Profile::passthrough("a"),
            Profile::desktop("b"),
            Profile::pc_action("c"),
            Profile::strategy("d"),
            Profile::platformer("e"),
        ]
    }

    #[test]
    fn templates_map_every_button_with_valid_keys() {
        use std::str::FromStr;
        for p in templates() {
            assert_eq!(p.buttons.len(), Button::ALL.len(), "{}: unmapped buttons", p.name);
            let mut keys: Vec<&String> = p.buttons.values().flat_map(|a| a.key_names()).collect();
            for t in [Trigger::Left, Trigger::Right] {
                if let TriggerAction::Button { action, .. } = p.trigger(t) {
                    keys.extend(action.key_names());
                }
            }
            for s in [Stick::Left, Stick::Right] {
                let cfg = p.stick(s);
                if let StickAction::Keys { up, down, left, right } = &cfg.action {
                    keys.extend([up, down, left, right]);
                }
                keys.extend(cfg.zones.iter().flat_map(|z| z.action.key_names()));
            }
            for k in keys {
                assert!(evdev::KeyCode::from_str(k).is_ok(), "{}: bad key {k}", p.name);
            }
            // Guide always cycles profiles, so no template can trap you in it.
            assert_eq!(p.button(Button::Guide), &ButtonAction::NextProfile, "{}", p.name);
        }
    }

    #[test]
    fn toggle_and_turbo_roundtrip_through_toml() {
        let mut config = Config::default();
        let crouch = ButtonAction::toggle(ButtonAction::Keys(vec!["KEY_C".into()]));
        let auto_fire = ButtonAction::toggle(ButtonAction::Turbo {
            action: Box::new(ButtonAction::Mouse(MouseButton::Left)),
            rate: 12.0,
        });
        config.general.profiles[0].set_button(Button::RightStick, crouch);
        config.general.profiles[0].set_button(Button::West, auto_fire);
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);
        assert_eq!(config.general.profiles[0].button(Button::RightStick).key_names(), [&"KEY_C".to_string()]);
    }

    #[test]
    fn toggles_that_start_on_add_a_field_others_read_as_before() {
        let plain = ButtonAction::toggle(ButtonAction::ShowInfo("Controls".into()));
        let starting = ButtonAction::Toggle(Toggled { action: Box::new(ButtonAction::ShowInfo("Controls".into())), start_on: true });
        let mut config = Config::default();
        config.general.profiles[0].set_button(Button::North, plain.clone());
        config.general.profiles[0].set_button(Button::West, starting.clone());
        let text = toml::to_string_pretty(&config).unwrap();
        assert!(text.contains("[general.profiles.buttons.North.toggle]\nshow_info = \"Controls\""), "{text}");
        assert!(text.contains("start_on = true"));
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.general.profiles[0].button(Button::North), &plain);
        assert_eq!(back.general.profiles[0].button(Button::West), &starting);
        // Over IPC too.
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), config);
    }

    #[test]
    fn gyro_config_roundtrips_and_is_optional() {
        let mut config = Config::default();
        config.general.profiles[0].gyro = GyroConfig {
            mode: GyroMode::Stick { stick: Stick::Right, full_rate: 300.0, anti_deadzone: 0.2 },
            horizontal: GyroHorizontal::YawAndRoll,
            invert_x: true,
            invert_y: false,
            activation: GyroActivation::Toggle(GyroInput::Button(Button::RightStick)),
            recenter: Some(GyroInput::Button(Button::Select)),
            noise_threshold: 2.0,
        };
        config.gyro_calibration.insert("Pad".into(), [0.1, -0.2, 0.05]);
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);

        let mut value = toml::Value::try_from(Config::default()).unwrap();
        for profile in value["general"]["profiles"].as_array_mut().unwrap() {
            profile.as_table_mut().unwrap().remove("gyro");
        }
        let old: Config = toml::from_str(&toml::to_string(&value).unwrap()).unwrap();
        assert_eq!(old.general.profiles[0].gyro, GyroConfig::default());
    }

    #[test]
    fn macros_roundtrip_and_walk_finds_nested_mappings() {
        let mut config = Config::default();
        let key = |k: &str| ButtonAction::Keys(vec![k.into()]);
        config.shared.macros.push(Macro {
            name: "Combo".into(),
            steps: vec![
                MacroStep::Tap { action: key("KEY_A"), hold_ms: 40 },
                MacroStep::Wait(100),
                MacroStep::Press(key("KEY_LEFTSHIFT")),
                MacroStep::Tap { action: ButtonAction::Mouse(MouseButton::Left), hold_ms: 20 },
                MacroStep::Release(key("KEY_LEFTSHIFT")),
            ],
        });
        let mapped = ButtonAction::toggle(ButtonAction::Macro { name: "Combo".into(), repeat: true });
        config.general.profiles[0].set_button(Button::West, mapped);
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);

        let mut names = Vec::new();
        for a in config.general.profiles[0].actions() {
            a.walk(&mut |a| {
                if let ButtonAction::Macro { name, .. } = a {
                    names.push(name.clone());
                }
            });
        }
        assert_eq!(names, ["Combo"]);
        assert_eq!(config.shared.macros[0].steps.iter().map(MacroStep::duration_ms).sum::<u64>(), 160);
    }

    #[test]
    fn stick_direction_buttons_and_macro_stick_steps_roundtrip() {
        let mut config = Config::default();
        config.general.profiles[0].set_button(Button::RightStickUp, ButtonAction::Macro { name: "Jump".into(), repeat: false });
        config.general.profiles[0].set_button(Button::DpadRight, ButtonAction::Gamepad(Button::LeftStickRight));
        config.general.profiles[0].combos.push(Combo {
            buttons: vec![Button::LeftBumper, Button::RightStickRight],
            action: ButtonAction::Keys(vec!["KEY_F".into()]),
        });
        config.shared.macros.push(Macro {
            name: "Jump".into(),
            steps: vec![MacroStep::Stick { stick: Stick::Left, x: 0.0, y: -1.0 }, MacroStep::Wait(17)],
        });
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);
        let found = config.general.profiles[0].actions().into_iter().any(|a| matches!(a, ButtonAction::Macro { .. }));
        assert!(found, "stick-direction mappings are visited like any button");
    }

    #[test]
    fn stick_direction_buttons_cover_every_direction_once() {
        assert_eq!(Button::EVERY.len(), Button::ALL.len() + 8);
        for s in [Stick::Left, Stick::Right] {
            let dirs = Button::stick_directions(s);
            assert!(dirs.iter().all(|b| b.stick_direction().is_some_and(|(stick, _)| stick == s)));
        }
        assert!(Button::ALL.iter().all(|b| b.stick_direction().is_none()), "ALL stays physical buttons only");
    }

    #[test]
    fn menus_roundtrip_and_pick_a_sensible_cancel_button() {
        let item = |label: &str, action| MenuItem { label: label.into(), action, button: None };
        let shared = Shared {
            menus: vec![
            Menu {
                name: "Weapons".into(),
                    kind: MenuKind::Radial { stick: Stick::Right },
                items: (1..=4).map(|n| item(&format!("Slot {n}"), ButtonAction::Keys(vec![format!("KEY_{n}")]))).collect(),
                cancel: None,
                style: OverlayStyle::default(),
            },
            Menu {
                name: "Pause".into(),
                    kind: MenuKind::Buttons,
                items: vec![MenuItem { button: Some(Button::LeftBumper), ..item("Map", ButtonAction::Keys(vec!["KEY_M".into()])) }],
                cancel: Some(Button::Start),
                style: OverlayStyle { position: ScreenPosition::TopRight, scale: 1.5, ..OverlayStyle::default() },
            },
            Menu {
                name: "Faces".into(),
                    kind: MenuKind::Directional { cluster: Cluster::FaceButtons },
                items: vec![item("More", ButtonAction::OpenMenu("Pause".into()))],
                cancel: None,
                style: OverlayStyle::default(),
            },
            ],
            ..Shared::default()
        };
        let mut config = Config { shared, ..Config::default() };
        config.general.profiles[0].set_button(Button::Select, ButtonAction::OpenMenu("Pause".into()));
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);
        assert_eq!(config.shared.menus[0].cancel_button(), Button::East);
        assert_eq!(config.shared.menus[1].cancel_button(), Button::Start);
        assert_eq!(config.shared.menus[2].cancel_button(), Button::Select, "East is a slot in a face-button directional menu");
    }

    #[test]
    fn old_cascade_menus_and_unstyled_menus_still_load() {
        let text = r#"
            name = "Old"
            items = []
            [kind.cascade]
            cluster = "DPad"
        "#;
        let menu: Menu = toml::from_str(text).unwrap();
        assert_eq!(menu.kind, MenuKind::Directional { cluster: Cluster::DPad });
        assert_eq!(menu.style, OverlayStyle::default());
        assert_eq!(Config::default().keyboard_style.position, ScreenPosition::BottomCenter);
        assert_eq!(Config::default().numpad_style.position, ScreenPosition::BottomRight);
    }

    #[test]
    fn hex_colors_parse_strictly() {
        assert_eq!(parse_hex("#2f5db0"), Some([0x2f, 0x5d, 0xb0]));
        assert_eq!(parse_hex(" #FFFFFF "), Some([255, 255, 255]));
        for bad in ["2f5db0", "#2f5db", "#2f5dbz", "", "#"] {
            assert_eq!(parse_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn templates_roundtrip_through_toml() {
        let config = Config { general: Game::new("General", templates()), ..Config::default() };
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn next_profile_wraps_within_the_game() {
        let mut config = Config::default();
        assert_eq!(config.next_profile(), Some(ProfileRef::new(None, "Desktop")));
        config.active = ProfileRef::new(None, "Desktop");
        assert_eq!(config.next_profile(), Some(ProfileRef::new(None, "Gamepad")));
        config.games.push(Game::new("Doom", vec![Profile::pc_action("Play"), Profile::desktop("Menus")]));
        config.active = ProfileRef::new(Some("Doom"), "Menus");
        assert_eq!(config.next_profile(), Some(ProfileRef::new(Some("Doom"), "Play")));
    }

    #[test]
    fn configs_from_before_games_are_converted() {
        let old = r#"
enabled = true
active_profile = "Souls"
ignored_devices = ["Wheel"]

[auto_switch]
enabled = true
default_profile = "Desktop"

[[auto_switch.rules]]
kind = "steam_app_id"
value = "1245620"
profile = "Souls"

[[macros]]
name = "Roll"
profiles = ["Souls"]
steps = [{ wait = 10 }]

[[macros]]
name = "Screenshot"
steps = [{ wait = 20 }]

[[macros]]
name = "Both"
profiles = ["Souls", "Desktop"]
steps = []

[[info_overlays]]
name = "Help"
profiles = ["Desktop"]
always = true
"#;
        let mut value: toml::Table = toml::from_str(old).unwrap();
        let profiles = [Profile::desktop("Desktop"), Profile::pc_action("Souls")];
        value.insert("profiles".into(), toml::Value::try_from(profiles).unwrap());
        let config = Config::from_legacy(&toml::to_string(&value).unwrap()).unwrap();

        assert_eq!(config.general.profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Desktop"]);
        assert_eq!(config.games.len(), 1);
        let souls = &config.games[0];
        assert_eq!((souls.name.as_str(), souls.profiles[0].name.as_str()), ("Souls", "Souls"));
        assert_eq!(souls.rules, [Rule::new(RuleKind::SteamAppId, "1245620", "Souls")]);
        assert_eq!(souls.names(ItemKind::Macro), ["Roll"]);
        assert_eq!(config.shared.names(ItemKind::Macro), ["Screenshot", "Both"], "unscoped, or spread over games");
        assert_eq!(config.general.names(ItemKind::Info), ["Help"]);
        assert!(config.general.info[0].always);
        assert_eq!(config.active, ProfileRef::new(Some("Souls"), "Souls"));
        assert_eq!(config.auto_switch.default_profile, Some(ProfileRef::new(None, "Desktop")));
        assert_eq!(config.ignored_devices, ["Wheel"]);
        // And it saves in the new format.
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn a_converted_config_always_has_a_general_profile() {
        let mut value = toml::Table::new();
        value.insert("active_profile".into(), "Gamepad".into());
        value.insert("profiles".into(), toml::Value::try_from([Profile::passthrough("Gamepad")]).unwrap());
        let rule = toml::Value::try_from(Rule::new(RuleKind::Executable, "game.exe", "Gamepad")).unwrap();
        let mut auto = toml::Table::new();
        auto.insert("rules".into(), toml::Value::Array(vec![rule]));
        value.insert("auto_switch".into(), auto.into());
        let config = Config::from_legacy(&toml::to_string(&value).unwrap()).unwrap();
        assert_eq!(config.games[0].name, "Gamepad");
        assert_eq!(config.general.profiles[0].name, "Gamepad (2)", "named apart from the game's profile");
        assert!(config.auto_switch.enabled);
        assert_eq!(config.active, ProfileRef::new(Some("Gamepad"), "Gamepad"));
    }

    #[test]
    fn replacing_the_config_keeps_follows_or_replaces_the_active_profile() {
        let mut config = Config::default();
        config.games.push(Game::new("Doom", vec![Profile::pc_action("Play")]));
        let playing = ProfileRef::new(Some("Doom"), "Play");
        // Unchanged: stays, whatever the editor's copy says.
        let mut new = config.clone();
        new.active = ProfileRef::new(None, "Desktop");
        new.carry_active(&playing);
        assert_eq!(new.active, playing);
        // Renamed: the editor's copy says where it went.
        let mut renamed = config.clone();
        renamed.games[0].name = "Doom II".into();
        renamed.active = ProfileRef::new(Some("Doom II"), "Play");
        renamed.carry_active(&playing);
        assert_eq!(renamed.active, ProfileRef::new(Some("Doom II"), "Play"));
        // Deleted: the default profile, else General's first.
        let mut deleted = config.clone();
        deleted.games.clear();
        deleted.active = playing.clone();
        deleted.auto_switch.default_profile = Some(ProfileRef::new(None, "Desktop"));
        deleted.carry_active(&playing);
        assert_eq!(deleted.active, ProfileRef::new(None, "Desktop"));
        deleted.auto_switch.default_profile = None;
        deleted.carry_active(&ProfileRef::new(Some("Gone"), "x"));
        assert_eq!(deleted.active, ProfileRef::new(None, "Desktop"), "Desktop exists, so it's kept");
        deleted.active = ProfileRef::new(Some("Gone"), "x");
        deleted.carry_active(&ProfileRef::new(Some("Gone"), "x"));
        assert_eq!(deleted.active, ProfileRef::new(None, "Gamepad"));
    }

    #[test]
    fn a_missing_active_profile_falls_back_to_its_game_then_general() {
        let mut config = Config::default();
        config.games.push(Game::new("Doom", vec![Profile::pc_action("Play")]));
        config.active = ProfileRef::new(Some("Doom"), "Gone");
        assert_eq!(config.active_ref(), ProfileRef::new(Some("Doom"), "Play"));
        config.active = ProfileRef::new(Some("No such game"), "Play");
        assert_eq!(config.active_ref(), ProfileRef::new(None, "Gamepad"));
    }

    #[test]
    fn scope_puts_the_games_items_before_shared_ones() {
        let mut config = Config::default();
        let m = |name: &str, steps: usize| Macro { name: name.into(), steps: vec![MacroStep::Wait(1); steps] };
        config.shared.macros = vec![m("Dodge", 1), m("Screenshot", 1)];
        let mut game = Game::new("Doom", vec![Profile::pc_action("Play")]);
        game.macros = vec![m("Dodge", 2)];
        config.games.push(game);
        assert_eq!(config.scope().macros, [m("Dodge", 1), m("Screenshot", 1)], "General sees shared ones");
        config.active = ProfileRef::new(Some("Doom"), "Play");
        assert_eq!(config.scope().macros, [m("Dodge", 2), m("Screenshot", 1)]);
    }

    #[test]
    fn finding_a_profile_by_name_prefers_the_active_game() {
        let mut config = Config::default();
        config.games.push(Game::new("A", vec![Profile::desktop("Desktop"), Profile::pc_action("Play")]));
        config.games.push(Game::new("B", vec![Profile::pc_action("Play")]));
        assert_eq!(config.find_profile("Desktop"), Some(ProfileRef::new(None, "Desktop")));
        assert_eq!(config.find_profile("Play"), Some(ProfileRef::new(Some("A"), "Play")));
        config.active = ProfileRef::new(Some("B"), "Play");
        assert_eq!(config.find_profile("Play"), Some(ProfileRef::new(Some("B"), "Play")));
        assert_eq!(config.find_profile("Nope"), None);
    }

    #[test]
    fn layers_override_on_top_of_a_profile_newest_last() {
        let mut base = Profile::desktop("Desktop");
        base.gestures.insert(Button::North, Gestures { double_tap: Some(ButtonAction::NextProfile), ..Gestures::default() });
        let f = |k: &str| ButtonAction::Keys(vec![k.into()]);
        let mut a = Layer::new("A");
        a.buttons.insert(Button::South, f("KEY_F1"));
        a.gestures.insert(Button::North, Gestures::default());
        a.disabled_combos.push(vec![Button::RightBumper, Button::LeftBumper]);
        a.right_stick = Some(StickConfig::new(StickAction::Disabled, 0.1, 1.0));
        let mut b = Layer::new("B");
        b.buttons.insert(Button::South, f("KEY_F2"));
        b.combos.push(Combo { buttons: vec![Button::Select, Button::Start], action: f("KEY_F3") });

        let p = base.with_layers([&a, &b]);
        assert_eq!(p.button(Button::South), &f("KEY_F2"), "the newest layer wins");
        assert_eq!(p.button(Button::East), base.button(Button::East), "unset inputs fall through");
        assert!(p.gestures(Button::North).is_none(), "empty gestures switch the profile's off");
        assert_eq!(p.combos.len(), 1, "LB+RB switched off (in any order), Select+Start added");
        assert_eq!(p.combos[0].buttons, [Button::Select, Button::Start]);
        assert_eq!(p.right_stick.action, StickAction::Disabled);
        assert_eq!(p.left_stick, base.left_stick);
        assert_eq!(a.overrides(), 4);

        // Layers belong to games and roundtrip with them.
        let mut config = Config::default();
        config.general.layers = vec![a, b];
        config.general.profiles[0].set_button(Button::LeftBumper, ButtonAction::Layer("A".into()));
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(back, config);
        assert_eq!(config.scope().layers.len(), 2);
        assert_eq!(config.general.layers_named(&["B".into(), "Nope".into()]).map(|l| l.name.as_str()).collect::<Vec<_>>(), ["B"]);
        config.general.rename_refs(ItemKind::Layer, "A", "Alpha");
        assert_eq!(config.general.profiles[0].button(Button::LeftBumper), &ButtonAction::Layer("Alpha".into()));
    }

    #[test]
    fn renames_follow_into_profiles_menus_and_macros() {
        let mut game = Game::new("G", vec![Profile::passthrough("P")]);
        game.profiles[0].set_button(Button::West, ButtonAction::toggle(ButtonAction::Macro { name: "Old".into(), repeat: false }));
        game.menus.push(Menu {
            name: "M".into(),
            kind: MenuKind::List,
            items: vec![MenuItem { label: "x".into(), action: ButtonAction::Macro { name: "Old".into(), repeat: false }, button: None }],
            cancel: None,
            style: OverlayStyle::default(),
        });
        game.rename_refs(ItemKind::Macro, "Old", "New");
        game.rename_refs(ItemKind::Menu, "New", "Wrong kind");
        assert_eq!(game.profiles[0].button(Button::West), &ButtonAction::toggle(ButtonAction::Macro { name: "New".into(), repeat: false }));
        assert_eq!(game.menus[0].items[0].action, ButtonAction::Macro { name: "New".into(), repeat: false });
    }
}

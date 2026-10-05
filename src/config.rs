use std::{collections::BTreeMap, fmt, path::PathBuf};

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
    Toggle(Box<ButtonAction>),
    /// While held, presses and releases the inner action `rate` times per second.
    Turbo { action: Box<ButtonAction>, rate: f32 },
    /// Plays the macro named `name`: once per press, or looping while held if `repeat`.
    Macro { name: String, #[serde(default)] repeat: bool },
    /// Opens or closes the on-screen overlay (keyboard). While it is open the controller
    /// drives the overlay; holding East closes it.
    ToggleOverlay,
    /// Opens or closes the on-screen numpad (works like the keyboard).
    ToggleNumpad,
    /// Shows the menu named here. As a menu item's action it opens a submenu.
    OpenMenu(String),
}

/// An on-screen action menu, shared by all profiles and opened with `ButtonAction::OpenMenu`.
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

/// A named sequence of inputs, shared by all profiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub steps: Vec<MacroStep>,
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
            ButtonAction::Toggle(inner) | ButtonAction::Turbo { action: inner, .. } => inner.key_names(),
            _ => Vec::new(),
        }
    }

    /// This action and every action nested inside it.
    pub fn walk(&self, f: &mut dyn FnMut(&ButtonAction)) {
        f(self);
        match self {
            ButtonAction::Multi(actions) => actions.iter().for_each(|a| a.walk(f)),
            ButtonAction::Toggle(inner) | ButtonAction::Turbo { action: inner, .. } => inner.walk(f),
            _ => {}
        }
    }

    pub fn walk_mut(&mut self, f: &mut dyn FnMut(&mut ButtonAction)) {
        f(self);
        match self {
            ButtonAction::Multi(actions) => actions.iter_mut().for_each(|a| a.walk_mut(f)),
            ButtonAction::Toggle(inner) | ButtonAction::Turbo { action: inner, .. } => inner.walk_mut(f),
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
    pub profile: String,
}

/// Per-game profile switching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoSwitch {
    pub enabled: bool,
    /// Profile for windows no rule matches; `None` leaves the current profile alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl Default for AutoSwitch {
    fn default() -> Self {
        AutoSwitch { enabled: true, default_profile: None, rules: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub enabled: bool,
    pub active_profile: String,
    /// Device names the daemon should leave alone.
    #[serde(default)]
    pub ignored_devices: Vec<String>,
    #[serde(default)]
    pub auto_switch: AutoSwitch,
    /// Gyro drift (degrees/second per raw sensor axis) measured by "Calibrate gyro", per controller.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gyro_calibration: BTreeMap<String, [f32; 3]>,
    /// Shared by all profiles; mapped with `ButtonAction::Macro`.
    #[serde(default)]
    pub macros: Vec<Macro>,
    /// On-screen action menus, shared by all profiles; opened with `ButtonAction::OpenMenu`.
    #[serde(default)]
    pub menus: Vec<Menu>,
    #[serde(default = "default_keyboard_style")]
    pub keyboard_style: OverlayStyle,
    #[serde(default = "default_numpad_style")]
    pub numpad_style: OverlayStyle,
    pub profiles: Vec<Profile>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            active_profile: "Gamepad".into(),
            ignored_devices: Vec::new(),
            auto_switch: AutoSwitch::default(),
            gyro_calibration: BTreeMap::new(),
            macros: Vec::new(),
            menus: Vec::new(),
            keyboard_style: OverlayStyle::keyboard(),
            numpad_style: OverlayStyle::numpad(),
            profiles: vec![Profile::passthrough("Gamepad"), Profile::desktop("Desktop")],
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

    /// Loads the config, writing the default one if none exists yet.
    pub fn load() -> Result<Self> {
        let path = Self::path();
        if !path.exists() {
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
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

    pub fn active(&self) -> Option<&Profile> {
        self.profiles
            .iter()
            .find(|p| p.name == self.active_profile)
            .or(self.profiles.first())
    }

    pub fn next_profile_name(&self) -> Option<String> {
        let idx = self.profiles.iter().position(|p| p.name == self.active_profile)?;
        let next = &self.profiles[(idx + 1) % self.profiles.len()];
        Some(next.name.clone())
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
        config.profiles[0].set_button(
            Button::South,
            ButtonAction::Multi(vec![
                ButtonAction::Gamepad(Button::South),
                ButtonAction::Keys(vec!["KEY_X".into()]),
                ButtonAction::Mouse(MouseButton::Left),
            ]),
        );
        config.profiles[0].combos.push(Combo {
            buttons: vec![Button::Select, Button::Start],
            action: ButtonAction::NextProfile,
        });
        config.profiles[0].gestures.insert(
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
        let p = &mut config.profiles[0];
        p.left_stick.zones.push(Zone { min: 0.0, max: 0.75, action: ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into()]) });
        p.right_trigger.zones.push(Zone { min: 0.95, max: 1.0, action: ButtonAction::Mouse(MouseButton::Left) });
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);

        // Before zones existed, a trigger was stored as a bare TriggerAction.
        let mut value = toml::Value::try_from(Config::default()).unwrap();
        for profile in value["profiles"].as_array_mut().unwrap() {
            let table = profile.as_table_mut().unwrap();
            let action = table["left_trigger"]["action"].clone();
            table.insert("left_trigger".into(), action);
        }
        let config: Config = toml::from_str(&toml::to_string(&value).unwrap()).unwrap();
        assert_eq!(config.profiles[0].left_trigger, TriggerAction::Gamepad(Trigger::Left).into());
        assert_eq!(config.profiles[1].trigger(Trigger::Left), Config::default().profiles[1].trigger(Trigger::Left));
    }

    #[test]
    fn old_configs_without_combos_still_load() {
        let mut value = toml::Value::try_from(Config::default()).unwrap();
        for profile in value["profiles"].as_array_mut().unwrap() {
            let table = profile.as_table_mut().unwrap();
            table.remove("combos");
            table.remove("combo_window_ms");
            table.remove("gestures");
            table.remove("tap_window_ms");
            table.remove("long_press_ms");
        }
        let config: Config = toml::from_str(&toml::to_string(&value).unwrap()).unwrap();
        assert!(config.profiles[0].combos.is_empty());
        assert_eq!(config.profiles[0].combo_window_ms, 60);
    }

    #[test]
    fn auto_switch_roundtrips_and_is_optional() {
        let mut config = Config::default();
        config.auto_switch.default_profile = Some("Desktop".into());
        config.auto_switch.rules.push(Rule {
            kind: RuleKind::SteamAppId,
            value: "1245620".into(),
            profile: "Gamepad".into(),
        });
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
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
        let crouch = ButtonAction::Toggle(Box::new(ButtonAction::Keys(vec!["KEY_C".into()])));
        let auto_fire = ButtonAction::Toggle(Box::new(ButtonAction::Turbo {
            action: Box::new(ButtonAction::Mouse(MouseButton::Left)),
            rate: 12.0,
        }));
        config.profiles[0].set_button(Button::RightStick, crouch);
        config.profiles[0].set_button(Button::West, auto_fire);
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);
        assert_eq!(config.profiles[0].button(Button::RightStick).key_names(), [&"KEY_C".to_string()]);
    }

    #[test]
    fn gyro_config_roundtrips_and_is_optional() {
        let mut config = Config::default();
        config.profiles[0].gyro = GyroConfig {
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
        for profile in value["profiles"].as_array_mut().unwrap() {
            profile.as_table_mut().unwrap().remove("gyro");
        }
        let old: Config = toml::from_str(&toml::to_string(&value).unwrap()).unwrap();
        assert_eq!(old.profiles[0].gyro, GyroConfig::default());
    }

    #[test]
    fn macros_roundtrip_and_walk_finds_nested_mappings() {
        let mut config = Config::default();
        let key = |k: &str| ButtonAction::Keys(vec![k.into()]);
        config.macros.push(Macro {
            name: "Combo".into(),
            steps: vec![
                MacroStep::Tap { action: key("KEY_A"), hold_ms: 40 },
                MacroStep::Wait(100),
                MacroStep::Press(key("KEY_LEFTSHIFT")),
                MacroStep::Tap { action: ButtonAction::Mouse(MouseButton::Left), hold_ms: 20 },
                MacroStep::Release(key("KEY_LEFTSHIFT")),
            ],
        });
        let mapped = ButtonAction::Toggle(Box::new(ButtonAction::Macro { name: "Combo".into(), repeat: true }));
        config.profiles[0].set_button(Button::West, mapped);
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);

        let mut names = Vec::new();
        for a in config.profiles[0].actions() {
            a.walk(&mut |a| {
                if let ButtonAction::Macro { name, .. } = a {
                    names.push(name.clone());
                }
            });
        }
        assert_eq!(names, ["Combo"]);
        assert_eq!(config.macros[0].steps.iter().map(MacroStep::duration_ms).sum::<u64>(), 160);
    }

    #[test]
    fn stick_direction_buttons_and_macro_stick_steps_roundtrip() {
        let mut config = Config::default();
        config.profiles[0].set_button(Button::RightStickUp, ButtonAction::Macro { name: "Jump".into(), repeat: false });
        config.profiles[0].set_button(Button::DpadRight, ButtonAction::Gamepad(Button::LeftStickRight));
        config.profiles[0].combos.push(Combo {
            buttons: vec![Button::LeftBumper, Button::RightStickRight],
            action: ButtonAction::Keys(vec!["KEY_F".into()]),
        });
        config.macros.push(Macro {
            name: "Jump".into(),
            steps: vec![MacroStep::Stick { stick: Stick::Left, x: 0.0, y: -1.0 }, MacroStep::Wait(17)],
        });
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);
        let found = config.profiles[0].actions().into_iter().any(|a| matches!(a, ButtonAction::Macro { .. }));
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
        let mut config = Config {
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
            ..Config::default()
        };
        config.profiles[0].set_button(Button::Select, ButtonAction::OpenMenu("Pause".into()));
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);
        assert_eq!(config.menus[0].cancel_button(), Button::East);
        assert_eq!(config.menus[1].cancel_button(), Button::Start);
        assert_eq!(config.menus[2].cancel_button(), Button::Select, "East is a slot in a face-button directional menu");
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
        let config = Config { profiles: templates(), ..Config::default() };
        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn next_profile_wraps() {
        let mut config = Config::default();
        assert_eq!(config.next_profile_name().as_deref(), Some("Desktop"));
        config.active_profile = "Desktop".into();
        assert_eq!(config.next_profile_name().as_deref(), Some("Gamepad"));
    }
}

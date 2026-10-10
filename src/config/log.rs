//! Log overlays: a list of recent entries on screen. So far the one kind of log is the
//! input log (see `crate::inputlog`).

use serde::{Deserialize, Serialize};

use super::{Button, OverlayStyle, Paint, ScreenPosition, Stick, Trigger};

/// An on-screen log, shown like an info overlay: always while its game is active, or while
/// a `ShowLog` action holds it up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogOverlay {
    pub name: String,
    /// Shown whenever a profile of its game is active, rather than only by `ShowLog`.
    #[serde(default)]
    pub always: bool,
    /// Seconds it stays after the action showing it lets go, fading out at the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linger: Option<f32>,
    #[serde(default = "OverlayStyle::log")]
    pub style: OverlayStyle,
    #[serde(default)]
    pub source: LogSource,
}

impl LogOverlay {
    pub fn new(name: &str) -> Self {
        LogOverlay { name: name.into(), always: false, linger: None, style: OverlayStyle::log(), source: LogSource::default() }
    }
}

/// What a log overlay lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogSource {
    /// The buttons pressed, one line per sequence, with what each did.
    Input(InputLogSettings),
}

impl Default for LogSource {
    fn default() -> Self {
        LogSource::Input(InputLogSettings::default())
    }
}

/// Which end of a log the newest line is at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogEnd {
    #[default]
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputLogSettings {
    /// Which inputs, from which controller, and how they group into lines.
    #[serde(flatten)]
    pub tracking: InputTracking,
    /// Lines (sequences) shown.
    pub lines: u8,
    pub newest: LogEnd,
    /// Seconds after a line's last input that it fades away; never when 0. (Not an
    /// `Option`: TOML can't write `None`, and the default would come back on loading.)
    pub fade_after: f32,
    /// The same input pressed again and again in a line shows once, with a count.
    pub merge_repeats: bool,
    /// What each input did (when remapped), under it.
    pub show_labels: bool,
    /// How long each input was held, under it.
    pub show_holds: bool,
}

impl Default for InputLogSettings {
    fn default() -> Self {
        InputLogSettings {
            tracking: InputTracking::default(),
            lines: 8,
            newest: LogEnd::Top,
            fade_after: 5.0,
            merge_repeats: true,
            show_labels: true,
            show_holds: true,
        }
    }
}

/// What an input display follows: shared by log overlays and `{current_input}` cells.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputTracking {
    /// The controller to follow, numbered from 0 in the order they connected; all of them
    /// when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<u8>,
    /// A pause longer than this (milliseconds) starts a new sequence.
    pub gap_ms: u64,
    /// Inputs shown per sequence at most; the oldest are pushed out first.
    pub max_inputs: u8,
    /// Inputs left out, as if never pressed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignored: Vec<TrackedInput>,
}

impl Default for InputTracking {
    fn default() -> Self {
        InputTracking { device: None, gap_ms: DEFAULT_GAP_MS, max_inputs: DEFAULT_MAX_INPUTS, ignored: Vec::new() }
    }
}

impl InputTracking {
    pub fn tracks(&self, input: TrackedInput) -> bool {
        !self.ignored.contains(&input)
    }

    /// Starts or stops following `input`.
    pub fn set_tracked(&mut self, input: TrackedInput, on: bool) {
        self.ignored.retain(|i| *i != input);
        if !on {
            self.ignored.push(input);
        }
    }
}

/// `{current_input}` settings of an info overlay: what its cells follow, and how long a
/// sequence stays after it ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CurrentInput {
    #[serde(flatten)]
    pub tracking: InputTracking,
    pub stay_ms: u64,
}

impl Default for CurrentInput {
    fn default() -> Self {
        CurrentInput { tracking: InputTracking::default(), stay_ms: DEFAULT_STAY_MS }
    }
}

impl CurrentInput {
    pub fn is_default(&self) -> bool {
        *self == CurrentInput::default()
    }
}

/// One input an input display can leave out. The d-pad and each stick count as one, all
/// their directions together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackedInput {
    Button(Button),
    Dpad,
    Stick(Stick),
    Trigger(Trigger),
}

impl TrackedInput {
    pub fn name(self) -> &'static str {
        match self {
            TrackedInput::Button(b) => b.short_name(),
            TrackedInput::Dpad => "D-pad",
            TrackedInput::Stick(Stick::Left) => "Left stick",
            TrackedInput::Stick(Stick::Right) => "Right stick",
            TrackedInput::Trigger(Trigger::Left) => "LT",
            TrackedInput::Trigger(Trigger::Right) => "RT",
        }
    }
}

/// The inputs in groups, as the settings window offers them.
pub const INPUT_GROUPS: [(&str, &[TrackedInput]); 7] = [
    (
        "Face buttons",
        &[
            TrackedInput::Button(Button::South),
            TrackedInput::Button(Button::East),
            TrackedInput::Button(Button::West),
            TrackedInput::Button(Button::North),
        ],
    ),
    ("Bumpers", &[TrackedInput::Button(Button::LeftBumper), TrackedInput::Button(Button::RightBumper)]),
    ("Triggers", &[TrackedInput::Trigger(Trigger::Left), TrackedInput::Trigger(Trigger::Right)]),
    ("D-pad", &[TrackedInput::Dpad]),
    ("Sticks", &[TrackedInput::Stick(Stick::Left), TrackedInput::Stick(Stick::Right)]),
    ("Stick clicks (L3, R3)", &[TrackedInput::Button(Button::LeftStick), TrackedInput::Button(Button::RightStick)]),
    (
        "Start, Select, Guide",
        &[TrackedInput::Button(Button::Start), TrackedInput::Button(Button::Select), TrackedInput::Button(Button::Guide)],
    ),
];

/// A pause longer than this ends a sequence of inputs, unless set otherwise.
pub const DEFAULT_GAP_MS: u64 = 250;
/// Inputs a sequence shows at most, unless set otherwise.
pub const DEFAULT_MAX_INPUTS: u8 = 12;
/// How long `{current_input}` stays after its sequence ends, unless set otherwise.
pub const DEFAULT_STAY_MS: u64 = 1000;

impl OverlayStyle {
    /// Log overlays: middle left, out of the info overlays' corner, small and see-through.
    pub fn log() -> Self {
        OverlayStyle {
            position: ScreenPosition::CenterLeft,
            scale: 0.8,
            background: Paint::new("#16181c", 0.6),
            ..OverlayStyle::default()
        }
    }
}

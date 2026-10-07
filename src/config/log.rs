//! Log overlays: a list of recent entries on screen. So far the one kind of log is the
//! input log (see `crate::inputlog`).

use serde::{Deserialize, Serialize};

use super::{OverlayStyle, Paint, ScreenPosition};

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
    /// The controller to follow, numbered from 0 in the order they connected; all of them
    /// when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<u8>,
    /// Lines (sequences) shown.
    pub lines: u8,
    pub newest: LogEnd,
    /// A pause longer than this (milliseconds) starts a new line.
    pub gap_ms: u64,
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
            device: None,
            lines: 8,
            newest: LogEnd::Top,
            gap_ms: DEFAULT_GAP_MS,
            fade_after: 5.0,
            merge_repeats: true,
            show_labels: true,
            show_holds: true,
        }
    }
}

/// A pause longer than this ends a sequence of inputs, unless set otherwise.
pub const DEFAULT_GAP_MS: u64 = 250;

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

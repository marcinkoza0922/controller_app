//! What a controller model's drawing is made of: where its parts sit and how it looks. The
//! models themselves are in `models`; the generic layout is the fallback.

use super::models::{
    DUALSENSE, DUALSENSE_EDGE, DUALSHOCK_4, GENERIC, JOY_CONS, JOY_CONS_2, PRO_CONTROLLER, STEAM_CONTROLLER, SWITCH_2_PRO, WII_U_PRO,
    XBOX_360, XBOX_ELITE, XBOX_ONE, XBOX_SERIES,
};
use crate::info::PadModel;

/// A round part: x, y and radius.
pub type Dot = (f32, f32, f32);

/// How a model's D-pad is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpadKind {
    /// A plus-shaped cross.
    Cross,
    /// Four separate pointed buttons (PlayStation).
    Arrows,
    /// A cross on a round dish (Xbox Series, Elite, 360).
    Disc,
    /// Four separate round buttons (Joy-Con).
    Buttons,
}

/// How Select and Start are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Small {
    Round,
    /// Upright pills, like Share and Options.
    Pill,
    /// Wide pills, like the Steam Controller's View and Menu.
    Wide,
    /// Round buttons with − and + on them (Nintendo).
    PlusMinus,
}

/// How the guide button is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideKind {
    Plain,
    /// The light Xbox button with its X.
    Xbox,
    /// The 360's chrome button in its ring of light.
    Xbox360,
    /// A Home button with a ring around it.
    Home,
}

/// The shell's color.
#[derive(Debug, Clone, Copy)]
pub enum Shell {
    Dark,
    /// A light shell around a dark middle piece (`panel`, a path), like the DualSense's. With
    /// `light_buttons` the buttons on the light part are light too.
    Light { panel: &'static str, light_buttons: bool },
}

/// How a mark is filled.
#[derive(Debug, Clone, Copy)]
pub enum Tint {
    /// Set into the shell: a dark fill with an edge, like a touchpad.
    Recess,
    /// A raised panel in the shell's own color, with an edge.
    Plate,
    /// A darker area of the shell, with no edge: a textured grip, a well.
    Shade,
    /// Something the pad carries in a color of its own, like the DualSense's light strips.
    Color(&'static str),
}

/// Something drawn on the body that isn't an input: a touchpad, a capture button, a light.
#[derive(Debug, Clone, Copy)]
pub enum Mark {
    /// x, y, width, height, corner radius.
    Rect(f32, f32, f32, f32, f32, Tint),
    /// x, y, radius.
    Circle(f32, f32, f32, Tint),
    /// Any shape, as a path.
    Path(&'static str, Tint),
    /// A shape on the left half and its mirror image on the right.
    Mirrored(&'static str, Tint),
    /// A colored ring: x, y, radius, color.
    Ring(f32, f32, f32, &'static str),
    /// A speaker's holes: top-left x and y, columns, rows, and the distance between holes.
    Grille(f32, f32, u8, u8, f32),
}

#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub outline: &'static str,
    pub shell: Shell,
    pub left_stick: (f32, f32),
    pub right_stick: (f32, f32),
    /// Radius of the stick's well; the cap and its travel scale from it.
    pub stick_r: f32,
    pub dpad: (f32, f32),
    pub dpad_kind: DpadKind,
    /// How far the D-pad reaches from its center.
    pub dpad_reach: f32,
    /// The center of the face buttons' diamond.
    pub face: (f32, f32),
    /// How far each face button sits from `face`.
    pub face_gap: f32,
    pub face_r: f32,
    pub select: Dot,
    pub guide: Dot,
    pub start: Dot,
    pub small: Small,
    pub guide_kind: GuideKind,
    /// The bumpers' and triggers' center x, left and right.
    pub shoulders: (f32, f32),
    /// The bumper's width; the trigger above it is a little narrower (see `TRIGGER_REACH`).
    pub shoulder_w: f32,
    /// Where the body's top edge runs under the shoulders: the bumper is drawn sitting on it.
    pub shoulder_y: f32,
    pub marks: &'static [Mark],
}

impl Layout {
    /// The layout for `model`; a pad of no known model gets the generic one.
    pub fn of(model: Option<PadModel>) -> &'static Layout {
        match model {
            Some(PadModel::DualShock4) => &DUALSHOCK_4,
            Some(PadModel::DualSense) => &DUALSENSE,
            Some(PadModel::DualSenseEdge) => &DUALSENSE_EDGE,
            Some(PadModel::ProController) => &PRO_CONTROLLER,
            Some(PadModel::Switch2Pro) => &SWITCH_2_PRO,
            Some(PadModel::JoyCons) => &JOY_CONS,
            Some(PadModel::JoyCons2) => &JOY_CONS_2,
            Some(PadModel::Xbox360) => &XBOX_360,
            Some(PadModel::XboxOne) => &XBOX_ONE,
            Some(PadModel::XboxSeries) => &XBOX_SERIES,
            Some(PadModel::XboxElite) => &XBOX_ELITE,
            Some(PadModel::SteamController) => &STEAM_CONTROLLER,
            Some(PadModel::WiiUPro) => &WII_U_PRO,
            None => &GENERIC,
        }
    }

    /// The face button offsets from `face`: south, east, west, north.
    pub fn face_offsets(&self) -> [(f32, f32); 4] {
        let g = self.face_gap;
        [(0.0, g), (g, 0.0), (-g, 0.0), (0.0, -g)]
    }

    /// The top of the bumper bar, just above where the body's edge runs.
    pub fn bumper_top(&self) -> f32 {
        self.shoulder_y - super::BUMPER_ABOVE
    }

    /// The top of the trigger: it stands right on the bumper it belongs to.
    pub fn trigger_top(&self) -> f32 {
        self.bumper_top() - super::TRIGGER_HEIGHT
    }
}

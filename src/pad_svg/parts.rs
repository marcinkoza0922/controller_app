//! The parts of a controller drawing: the shell and its marks, the shoulders, sticks, D-pad,
//! Select and Start, and the guide button. A pressed part lights up in `ACTIVE`.

use std::fmt::Write;

use super::{
    ACTIVE, BUMPER_HEIGHT, GUIDE, IDLE, IDLE_EDGE, TRIGGER_HEIGHT, TRIGGER_REACH,
    layout::{DpadKind, GuideKind, Layout, Mark, Shell, Small, Tint},
};
use crate::config::Button;

/// How far the shoulders lean outward with the shell's top corners, in degrees.
const TILT: f32 = 5.0;
/// The light gray of glyphs drawn on dark buttons.
const GLYPH: &str = "#c9d1d9";

/// The fill and edge of the buttons on a part of the shell.
#[derive(Clone, Copy)]
pub(super) struct Ink {
    pub(super) fill: &'static str,
    pub(super) edge: &'static str,
}

const DARK_INK: Ink = Ink { fill: IDLE, edge: IDLE_EDGE };
/// Buttons on a light shell, like the DualSense's.
const LIGHT_INK: Ink = Ink { fill: "#f6f7f9", edge: "#9aa1ab" };

/// The shell's gradient, top and bottom, and its edge.
fn shell_colors(shell: Shell) -> (&'static str, &'static str, &'static str) {
    match shell {
        Shell::Dark => ("#3a404a", "#2a2e36", "#4a505a"),
        Shell::Light { .. } => ("#f4f5f7", "#d2d6dc", "#a3aab3"),
    }
}

/// The ink of the buttons out on the shell's sides: the D-pad, the face buttons, Select and Start.
pub(super) fn wing_ink(l: &Layout) -> Ink {
    match l.shell {
        Shell::Light { light_buttons: true, .. } => LIGHT_INK,
        _ => DARK_INK,
    }
}

/// The gradients and the shell's clip, which the parts refer to by id.
pub(super) fn defs(s: &mut String, l: &Layout) {
    let (top, bottom, _) = shell_colors(l.shell);
    let _ = write!(
        s,
        concat!(
            r##"<defs><linearGradient id="g-shell" x1="0" y1="30" x2="0" y2="270" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient>"##,
            r##"<linearGradient id="g-panel" x1="0" y1="100" x2="0" y2="260" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#30343c"/><stop offset="1" stop-color="#1c1f24"/></linearGradient>"##,
            r##"<linearGradient id="g-shoulder" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#454b56"/><stop offset="1" stop-color="#22252b"/></linearGradient>"##,
            r##"<radialGradient id="g-well"><stop offset="0.6" stop-color="#16181c"/><stop offset="1" stop-color="#24272d"/></radialGradient>"##,
            r##"<radialGradient id="g-cap" cy="0.45"><stop offset="0" stop-color="#2a2e34"/><stop offset="0.7" stop-color="#383d45"/><stop offset="1" stop-color="#4a505a"/></radialGradient>"##,
            r##"<radialGradient id="g-chrome" cx="0.4" cy="0.35"><stop offset="0" stop-color="#f4f6f8"/><stop offset="0.6" stop-color="#aab1ba"/><stop offset="1" stop-color="#6b727c"/></radialGradient>"##,
            r##"<clipPath id="c-shell"><path d="{outline}"/></clipPath></defs>"##
        ),
        top = top,
        bottom = bottom,
        outline = l.outline
    );
}

/// The shell: its fill, the dark middle of a two-tone pad, a soft rim just inside the edge so it
/// reads as molded rather than cut out, and the edge.
pub(super) fn body(s: &mut String, l: &Layout) {
    let (_, _, edge) = shell_colors(l.shell);
    let _ = write!(s, r#"<path d="{}" fill="url(#g-shell)"/>"#, l.outline);
    let rim = match l.shell {
        Shell::Light { panel, .. } => {
            let _ = write!(s, r#"<path d="{panel}" fill="url(#g-panel)"/>"#);
            "#000000"
        }
        Shell::Dark => "#ffffff",
    };
    let _ = write!(
        s,
        r#"<path d="{o}" fill="none" stroke="{rim}" stroke-opacity="0.07" stroke-width="7" clip-path="url(#c-shell)"/><path d="{o}" fill="none" stroke="{edge}" stroke-width="2.5"/>"#,
        o = l.outline
    );
}

/// The marks that tell the models apart: touchpads, lights, grips, speakers.
pub(super) fn marks(s: &mut String, l: &Layout) {
    let (_, _, shell_edge) = shell_colors(l.shell);
    let paint = |tint: Tint| match tint {
        Tint::Recess => format!(r#"fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="1.5""#),
        Tint::Plate => format!(r#"fill="url(#g-shell)" stroke="{shell_edge}" stroke-width="1.5""#),
        Tint::Shade => r##"fill="#000000" fill-opacity="0.16""##.to_string(),
        Tint::Color(c) => format!(r#"fill="{c}""#),
    };
    for mark in l.marks {
        let _ = match *mark {
            Mark::Rect(x, y, w, h, rx, t) => write!(s, r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" {}/>"#, paint(t)),
            Mark::Circle(x, y, r, t) => write!(s, r#"<circle cx="{x}" cy="{y}" r="{r}" {}/>"#, paint(t)),
            Mark::Path(d, t) => write!(s, r#"<path d="{d}" {}/>"#, paint(t)),
            // The canvas is 420 wide, so x becomes 420 - x.
            Mark::Mirrored(d, t) => write!(s, r#"<path d="{d}" {p}/><path d="{d}" transform="matrix(-1 0 0 1 420 0)" {p}/>"#, p = paint(t)),
            Mark::Ring(x, y, r, c) => write!(s, r#"<circle cx="{x}" cy="{y}" r="{r}" fill="none" stroke="{c}" stroke-width="2.5"/>"#),
            Mark::Grille(x, y, cols, rows, pitch) => {
                for (i, j) in (0..rows).flat_map(|j| (0..cols).map(move |i| (i, j))) {
                    let (hx, hy) = (x + f32::from(i) * pitch, y + f32::from(j) * pitch);
                    let _ = write!(s, r##"<circle cx="{hx}" cy="{hy}" r="1.1" fill="#000000" fill-opacity="0.5"/>"##);
                }
                Ok(())
            }
        };
    }
}

/// The triggers and bumpers, seen from the front: each trigger a rounded hump behind its bumper,
/// both leaning out with the shell's corners. A trigger fills from the bottom as it's pulled.
pub(super) fn shoulders(s: &mut String, l: &Layout, sides: [(f32, Button); 2], pressed: impl Fn(Button) -> bool) {
    let w = l.shoulder_w;
    let (trigger_top, bumper_top) = (l.trigger_top(), l.bumper_top());
    // Turning about the trigger's middle keeps it where its label goes.
    let pivot = trigger_top + TRIGGER_HEIGHT / 2.0;
    let sides = sides.into_iter().zip([l.shoulders.0, l.shoulders.1]).zip([(-TILT, "c-lt"), (TILT, "c-rt")]);
    for (((value, bumper), cx), (tilt, clip)) in sides {
        let _ = write!(s, r#"<g transform="rotate({tilt} {cx} {pivot})">"#);
        // Narrower at the top than at its foot, which is hidden behind the bumper.
        let (x0, x1, y0, y1) = (cx - w * TRIGGER_REACH, cx + w * TRIGGER_REACH, trigger_top, bumper_top + 6.0);
        let (a, b, f) = (x0 + 3.0, y0 + 9.0, x1 - 3.0);
        let d = format!("M {x0} {y1} L {a} {b} Q {a} {y0} {} {y0} L {} {y0} Q {f} {y0} {f} {b} L {x1} {y1} Z", x0 + 12.0, x1 - 12.0);
        let _ = write!(
            s,
            r#"<clipPath id="{clip}"><path d="{d}"/></clipPath><path d="{d}" fill="url(#g-shoulder)" stroke="{IDLE_EDGE}" stroke-width="1.5"/>"#
        );
        let h = value.clamp(0.0, 1.0) * TRIGGER_HEIGHT;
        if h > 0.5 {
            let y = y0 + TRIGGER_HEIGHT - h;
            let _ = write!(s, r#"<rect x="{x0}" y="{y}" width="{}" height="{}" fill="{ACTIVE}" clip-path="url(#{clip})"/>"#, x1 - x0, y1 - y);
        }
        let fill = if pressed(bumper) { ACTIVE } else { "url(#g-shoulder)" };
        let x = cx - w / 2.0;
        let _ = write!(
            s,
            r##"<rect x="{x}" y="{bumper_top}" width="{w}" height="{BUMPER_HEIGHT}" rx="9" fill="{fill}" stroke="{IDLE_EDGE}" stroke-width="1.5"/><path d="M {} {} H {}" stroke="#ffffff" stroke-opacity="0.15" stroke-width="1.5" stroke-linecap="round"/></g>"##,
            x + 9.0,
            bumper_top + 2.5,
            x + w - 9.0
        );
    }
}

/// A thumbstick: its well, and the dished cap pushed off center by `(x, y)` (-1 to 1 each way).
pub(super) fn stick(s: &mut String, (cx, cy): (f32, f32), r: f32, (x, y): (f32, f32), clicked: bool) {
    let travel = r * 13.0 / 27.0;
    let (tx, ty) = (cx + x.clamp(-1.0, 1.0) * travel, cy + y.clamp(-1.0, 1.0) * travel);
    let cap_r = r * 2.0 / 3.0;
    let cap = if clicked { ACTIVE } else { "url(#g-cap)" };
    let _ = write!(
        s,
        r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="url(#g-well)" stroke="{IDLE_EDGE}" stroke-width="1.5"/><circle cx="{tx}" cy="{ty}" r="{cap_r}" fill="{cap}" stroke="{IDLE_EDGE}" stroke-width="1.5"/><circle cx="{tx}" cy="{ty}" r="{}" fill="none" stroke="#000000" stroke-opacity="0.3" stroke-width="1.5"/>"##,
        cap_r * 11.0 / 18.0
    );
}

/// The D-pad directions with the angle each is turned from up, clockwise.
const ARMS: [(Button, f32); 4] = [(Button::DpadUp, 0.0), (Button::DpadRight, 90.0), (Button::DpadDown, 180.0), (Button::DpadLeft, 270.0)];

/// The D-pad, in the shape the model has it. Each part is drawn pointing up and turned into place.
pub(super) fn dpad(s: &mut String, l: &Layout, ink: Ink, pressed: impl Fn(Button) -> bool) {
    let (cx, cy) = l.dpad;
    let reach = l.dpad_reach;
    match l.dpad_kind {
        DpadKind::Cross => s.push_str(&cross(l.dpad, reach, reach * 8.0 / 26.0, ink, pressed)),
        DpadKind::Disc => {
            let _ = write!(
                s,
                r#"<circle cx="{cx}" cy="{cy}" r="{reach}" fill="{}" stroke="{}" stroke-width="1.5"/><circle cx="{cx}" cy="{cy}" r="{}" fill="none" stroke="{}" stroke-opacity="0.4"/>"#,
                ink.fill,
                ink.edge,
                reach * 0.9,
                ink.edge
            );
            s.push_str(&cross(l.dpad, reach * 0.82, reach * 0.28, Ink { fill: "#2a2e35", ..ink }, pressed));
        }
        DpadKind::Arrows => {
            let (hw, top, tip) = (reach * 0.3, cy - reach, cy - reach * 0.2);
            let (rr, shoulder) = (hw * 0.45, tip - hw * 0.95);
            let (xl, xr) = (cx - hw, cx + hw);
            for (b, turn) in ARMS {
                let fill = if pressed(b) { ACTIVE } else { ink.fill };
                let _ = write!(
                    s,
                    r#"<path d="M {xl} {} Q {xl} {top} {} {top} H {} Q {xr} {top} {xr} {} V {shoulder} L {cx} {tip} L {xl} {shoulder} Z" transform="rotate({turn} {cx} {cy})" fill="{fill}" stroke="{}" stroke-width="1.5"/>"#,
                    top + rr,
                    xl + rr,
                    xr - rr,
                    top + rr,
                    ink.edge
                );
                s.push_str(&arrow((cx, cy), top + reach * 0.14, reach * 0.09, turn, ink.edge));
            }
        }
        DpadKind::Buttons => {
            let (arm, r) = (reach * 18.0 / 26.0, reach * 0.35);
            for (b, turn) in ARMS {
                let fill = if pressed(b) { ACTIVE } else { ink.fill };
                let _ = write!(
                    s,
                    r#"<circle cx="{cx}" cy="{}" r="{r}" transform="rotate({turn} {cx} {cy})" fill="{fill}" stroke="{}" stroke-width="1.5"/>"#,
                    cy - arm,
                    ink.edge
                );
                s.push_str(&arrow((cx, cy), cy - arm - r * 0.4, r * 0.3, turn, ink.edge));
            }
        }
    }
}

/// A plus-shaped D-pad reaching `reach` from its center, its arms `2 * hw` wide. A pressed arm
/// lights up, and each carries a small arrow.
fn cross((cx, cy): (f32, f32), reach: f32, hw: f32, ink: Ink, pressed: impl Fn(Button) -> bool) -> String {
    let mut s = String::new();
    let rr = hw * 0.5;
    let (xl, xr, yt, yb) = (cx - hw, cx + hw, cy - hw, cy + hw);
    let (n, so, w, e) = (cy - reach, cy + reach, cx - reach, cx + reach);
    let _ = write!(
        s,
        concat!(
            r#"<path d="M {xl} {n1} Q {xl} {n} {xl1} {n} H {xr1} Q {xr} {n} {xr} {n1} V {yt} H {e1} Q {e} {yt} {e} {yt1} V {yb1} Q {e} {yb} {e1} {yb} "#,
            r#"H {xr} V {s1} Q {xr} {so} {xr1} {so} H {xl1} Q {xl} {so} {xl} {s1} V {yb} H {w1} Q {w} {yb} {w} {yb1} V {yt1} Q {w} {yt} {w1} {yt} H {xl} Z" "#,
            r#"fill="{fill}" stroke="{edge}" stroke-width="1.5"/>"#
        ),
        xl = xl,
        xr = xr,
        yt = yt,
        yb = yb,
        n = n,
        so = so,
        w = w,
        e = e,
        n1 = n + rr,
        xl1 = xl + rr,
        xr1 = xr - rr,
        e1 = e - rr,
        yt1 = yt + rr,
        yb1 = yb - rr,
        s1 = so - rr,
        w1 = w + rr,
        fill = ink.fill,
        edge = ink.edge,
    );
    for (b, turn) in ARMS {
        if pressed(b) {
            let _ = write!(
                s,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{rr}" transform="rotate({turn} {cx} {cy})" fill="{ACTIVE}"/>"#,
                xl + 1.0,
                n + 1.0,
                2.0 * hw - 2.0,
                reach - hw - 1.0
            );
        }
        s.push_str(&arrow((cx, cy), n + reach * 0.16, reach * 0.1, turn, ink.edge));
    }
    let _ = write!(s, r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="none" stroke="{}" stroke-opacity="0.35"/>"#, hw * 0.5, ink.edge);
    s
}

/// A small arrow pointing up from its tip at height `tip` above `center`, `half` wide each way,
/// turned `turn` degrees about `center`.
fn arrow((cx, cy): (f32, f32), tip: f32, half: f32, turn: f32, color: &str) -> String {
    let base = tip + half * 1.6;
    format!(
        r#"<path d="M {cx} {tip} L {} {base} H {} Z" transform="rotate({turn} {cx} {cy})" fill="{color}" fill-opacity="0.8"/>"#,
        cx + half,
        cx - half
    )
}

/// Select and Start, in the shape the model has them.
pub(super) fn small_buttons(s: &mut String, l: &Layout, ink: Ink, pressed: impl Fn(Button) -> bool) {
    let glyph = if ink.fill == IDLE { GLYPH } else { ink.edge };
    for (b, (x, y, r)) in [(Button::Select, l.select), (Button::Start, l.start)] {
        let fill = if pressed(b) { ACTIVE } else { ink.fill };
        let _ = match l.small {
            Small::Round | Small::PlusMinus => write!(s, r#"<circle cx="{x}" cy="{y}" r="{r}" fill="{fill}" stroke="{}" stroke-width="1.5"/>"#, ink.edge),
            Small::Pill | Small::Wide => {
                let (w, h) = if l.small == Small::Pill { (r * 1.5, r * 2.9) } else { (r * 4.2, r * 1.6) };
                write!(
                    s,
                    r#"<rect x="{}" y="{}" width="{w}" height="{h}" rx="{}" fill="{fill}" stroke="{}" stroke-width="1.5"/>"#,
                    x - w / 2.0,
                    y - h / 2.0,
                    w.min(h) / 2.0,
                    ink.edge
                )
            }
        };
        if l.small == Small::PlusMinus {
            let g = r * 0.5;
            let upright = if b == Button::Start { format!(" M {x} {} V {}", y - g, y + g) } else { String::new() };
            let _ = write!(
                s,
                r#"<path d="M {} {y} H {}{upright}" stroke="{glyph}" stroke-width="{}" stroke-linecap="round"/>"#,
                x - g,
                x + g,
                r * 0.3
            );
        }
    }
}

/// The guide button: PlayStation's and Steam's plain, Xbox's light with its X, the 360's chrome in
/// its ring of light, Nintendo's Home with a ring.
pub(super) fn guide(s: &mut String, l: &Layout, pressed: bool) {
    let (x, y, r) = l.guide;
    let lit = |idle: &'static str| if pressed { ACTIVE } else { idle };
    let x_mark = |s: &mut String, color: &str, width: f32| {
        let k = r * 0.42;
        let _ = write!(
            s,
            r#"<path d="M {} {} L {} {} M {} {} L {} {}" stroke="{color}" stroke-width="{width}" stroke-linecap="round"/>"#,
            x - k,
            y - k,
            x + k,
            y + k,
            x + k,
            y - k,
            x - k,
            y + k
        );
    };
    match l.guide_kind {
        GuideKind::Plain | GuideKind::Home => {
            let _ = write!(s, r#"<circle cx="{x}" cy="{y}" r="{r}" fill="{}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#, lit(GUIDE));
            if l.guide_kind == GuideKind::Home {
                let _ = write!(s, r#"<circle cx="{x}" cy="{y}" r="{}" fill="none" stroke="{GLYPH}" stroke-opacity="0.45" stroke-width="1.2"/>"#, r * 0.6);
            }
        }
        GuideKind::Xbox => {
            let _ = write!(s, r##"<circle cx="{x}" cy="{y}" r="{r}" fill="{}" stroke="#9aa1ab" stroke-width="1.5"/>"##, lit("#e4e7eb"));
            x_mark(s, "#2a2e35", r * 0.2);
        }
        GuideKind::Xbox360 => {
            let _ = write!(
                s,
                r##"<circle cx="{x}" cy="{y}" r="{}" fill="none" stroke="#7ccf5a" stroke-opacity="0.85" stroke-width="2.5"/><circle cx="{x}" cy="{y}" r="{r}" fill="{}" stroke="{IDLE_EDGE}" stroke-width="1.5"/>"##,
                r + 2.5,
                lit("url(#g-chrome)")
            );
            x_mark(s, "#3d9b35", r * 0.18);
        }
    }
}

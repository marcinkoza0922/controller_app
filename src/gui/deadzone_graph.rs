//! The deadzone in two pictures: where the stick is on its travel, with the dead middle shaded,
//! and the response along one push, flat until the deadzone and then rising to full. When a
//! controller is connected, the live stick is drawn in both: hollow where it is, filled where the
//! deadzone puts it.

use std::fmt::Write;

use iced::widget::{column, row, svg};
use iced::{Element, Length};

use super::*;
use crate::engine::apply_deadzone;

/// Side of each picture, and the margin kept clear inside it, in logical pixels.
const SIDE: f32 = 150.0;
const PAD: f32 = 10.0;
const PANEL: &str = "#1d2026";
const DEAD: &str = "#3d434e";
const LINEAR: &str = "#5d6470";
const STEADY: &str = "#4ea1ff";
const MARK: &str = "#ffffff";

/// The live stick as `(x, y)` in -1..1, pulled back inside the travel circle.
fn inside_travel(live: (f32, f32)) -> (f32, f32) {
    let m = live.0.hypot(live.1);
    if m > 1.0 { (live.0 / m, live.1 / m) } else { live }
}

fn circle(c: f32, r: f32, fill: &str, stroke: &str, dash: &str) -> String {
    format!(r#"<circle cx="{c}" cy="{c}" r="{r:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1" stroke-dasharray="{dash}"/>"#)
}

fn dot(at: (f32, f32), fill: &str) -> String {
    format!(r##"<circle cx="{:.1}" cy="{:.1}" r="4.5" fill="{fill}" stroke="#000" stroke-width="1"/>"##, at.0, at.1)
}

fn ring_dot(at: (f32, f32)) -> String {
    format!(r##"<circle cx="{:.1}" cy="{:.1}" r="4.5" fill="none" stroke="{MARK}" stroke-width="1.5"/>"##, at.0, at.1)
}

fn frame(extra: &str) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{SIDE}" height="{SIDE}" viewBox="0 0 {SIDE} {SIDE}"><rect width="{SIDE}" height="{SIDE}" rx="6" fill="{PANEL}"/>{extra}"#)
}

/// SVG of the stick's travel: the full circle, its dead middle shaded, and the live stick.
fn travel_svg(deadzone: f32, live: Option<(f32, f32)>) -> String {
    let c = SIDE / 2.0;
    let r = c - PAD;
    let mut out = frame(&circle(c, r, "none", LINEAR, "3 3"));
    out.push_str(&circle(c, deadzone * r, DEAD, "none", "none"));
    out.push_str(&circle(c, deadzone * r, "none", LINEAR, "2 2"));
    if let Some(at) = live {
        let (x, y) = inside_travel(at);
        // Screen y points down, as the stick's does.
        let raw = (c + x * r, c + y * r);
        let (mx, my) = apply_deadzone(x, y, deadzone);
        out.push_str(&ring_dot(raw));
        out.push_str(&dot((c + mx * r, c + my * r), STEADY));
    }
    out.push_str("</svg>");
    out
}

/// SVG of the response: the push going in (across) against the output (up). The shaded band is
/// the deadzone, and the dotted line is what a stick with no deadzone would give.
fn response_svg(deadzone: f32, live: Option<(f32, f32)>) -> String {
    let span = SIDE - 2.0 * PAD;
    let x = |m: f32| PAD + m * span;
    let y = |v: f32| SIDE - PAD - v * span;
    let mut out = frame("");
    out.push_str(&format!(r#"<rect x="{PAD}" y="{PAD}" width="{:.1}" height="{span}" fill="{DEAD}"/>"#, deadzone * span));
    out.push_str(&format!(r#"<path d="M{:.1} {:.1} L{:.1} {:.1}" stroke="{LINEAR}" stroke-width="1.5" stroke-dasharray="3 3" fill="none"/>"#, x(0.0), y(0.0), x(1.0), y(1.0)));
    out.push_str(&format!(
        r#"<path d="M{:.1} {:.1} L{:.1} {:.1} L{:.1} {:.1}" stroke="{STEADY}" stroke-width="2" stroke-linejoin="round" fill="none"/>"#,
        x(0.0), y(0.0), x(deadzone), y(0.0), x(1.0), y(1.0)
    ));
    if let Some(at) = live {
        let (x_in, y_in) = inside_travel(at);
        let mag = x_in.hypot(y_in);
        // The output magnitude the deadzone gives this push: the x of a push along one axis.
        let (out_mag, _) = apply_deadzone(mag, 0.0, deadzone);
        let _ = write!(out, r#"<path d="M{0:.1} {1:.1} L{0:.1} {2:.1}" stroke="{MARK}" stroke-width="1" stroke-dasharray="2 2"/>"#, x(mag), y(0.0), y(out_mag));
        out.push_str(&dot((x(mag), y(out_mag)), STEADY));
    }
    out.push_str("</svg>");
    out
}

/// The deadzone's two pictures, side by side, and what they say in words.
pub(super) fn view<'a>(deadzone: f32, live: Option<(f32, f32)>) -> Element<'a, Message> {
    let travel = svg(svg::Handle::from_memory(travel_svg(deadzone, live).into_bytes())).width(SIDE).height(SIDE);
    let response = svg(svg::Handle::from_memory(response_svg(deadzone, live).into_bytes())).width(SIDE).height(SIDE);
    let words = if deadzone <= 0.0 {
        "No deadzone: every push counts.".to_string()
    } else {
        format!("The first {:.0}% of the stick's travel does nothing. Past it, the stick counts from 0 again.", deadzone * 100.0)
    };
    column![
        row![
            column![text("Stick travel").size(13).color(MUTED_COLOR), travel].spacing(6),
            column![text("Response").size(13).color(MUTED_COLOR), response].spacing(6),
        ]
        .spacing(24),
        text(words).size(13).width(Length::Fixed(2.0 * SIDE + 24.0)),
    ]
    .spacing(6)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dead_middle_is_as_wide_as_the_deadzone() {
        let svg = travel_svg(0.2, None);
        assert!(svg.contains(&format!(r#"r="{:.1}""#, 0.2 * (SIDE / 2.0 - PAD))));
        assert_eq!(svg.matches("<circle").count(), 3);
    }

    #[test]
    fn a_live_stick_inside_the_deadzone_maps_to_the_middle() {
        let svg = travel_svg(0.3, Some((0.1, 0.0)));
        // The filled dot sits on the center, the hollow one where the stick is.
        assert!(svg.contains(&format!(r#"cx="{:.1}""#, SIDE / 2.0)));
        assert_eq!(svg.matches("<circle").count(), 5);
    }

    #[test]
    fn the_response_is_flat_until_the_deadzone() {
        let svg = response_svg(0.5, Some((0.25, 0.0)));
        // The live push is in the flat band, so its output marker sits on the baseline.
        assert!(svg.contains(&format!(r#"cy="{:.1}""#, SIDE - PAD)));
    }
}

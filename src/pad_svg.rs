//! Builds an SVG drawing of a gamepad reflecting live input.

use std::fmt::Write;

use crate::{config::Button, ipc::InputSnapshot};

const BODY: &str = "#30343c";
const BODY_EDGE: &str = "#4a505a";
const IDLE: &str = "#1d2026";
const IDLE_EDGE: &str = "#5d6470";
const ACTIVE: &str = "#4ea1ff";
const LABEL: &str = "#c9d1d9";

/// How far (in SVG units) a fully deflected thumbstick cap travels from center.
const STICK_TRAVEL: f32 = 13.0;
const TRIGGER_HEIGHT: f32 = 30.0;

/// Renders the controller. With `None` everything is drawn at rest.
pub fn render(input: Option<&InputSnapshot>) -> String {
    let pressed = |b: Button| input.is_some_and(|i| i.buttons.contains(&b));
    let fill = |b: Button| if pressed(b) { ACTIVE } else { IDLE };
    let (ls, rs, lt, rt) = input
        .map(|i| (i.left_stick, i.right_stick, i.left_trigger, i.right_trigger))
        .unwrap_or(((0.0, 0.0), (0.0, 0.0), 0.0, 0.0));

    let mut s = String::with_capacity(4096);
    s.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 420 275">"#);

    // Triggers: outline with a fill that rises with pressure.
    for (x, value) in [(95.0, lt), (285.0, rt)] {
        let h = value.clamp(0.0, 1.0) * TRIGGER_HEIGHT;
        let _ = write!(
            s,
            r#"<rect x="{x}" y="6" width="40" height="{TRIGGER_HEIGHT}" rx="8" fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#
        );
        if h > 0.5 {
            let _ = write!(
                s,
                r#"<rect x="{x}" y="{y}" width="40" height="{h}" rx="6" fill="{ACTIVE}"/>"#,
                y = 6.0 + TRIGGER_HEIGHT - h
            );
        }
    }
    let _ = write!(
        s,
        r#"<text x="115" y="27" font-size="11" text-anchor="middle" fill="{LABEL}" font-family="sans-serif">LT</text><text x="305" y="27" font-size="11" text-anchor="middle" fill="{LABEL}" font-family="sans-serif">RT</text>"#
    );

    // Bumpers.
    for (x, b) in [(72.0, Button::LeftBumper), (268.0, Button::RightBumper)] {
        let _ = write!(
            s,
            r#"<rect x="{x}" y="40" width="80" height="14" rx="7" fill="{}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#,
            fill(b)
        );
    }

    // Body.
    let _ = write!(
        s,
        r#"<path d="M110 58 C150 49 270 49 310 58 C350 66 372 90 385 140 C400 200 405 245 375 258 C350 268 325 245 305 215 C290 207 130 207 115 215 C95 245 70 268 45 258 C15 245 20 200 35 140 C48 90 70 66 110 58 Z" fill="{BODY}" stroke="{BODY_EDGE}" stroke-width="3"/>"#
    );

    stick(&mut s, 118.0, 118.0, ls, pressed(Button::LeftStick));
    stick(&mut s, 255.0, 168.0, rs, pressed(Button::RightStick));

    // D-pad: a center square with four arms.
    let (cx, cy) = (165.0, 168.0);
    let _ = write!(s, r#"<rect x="{}" y="{}" width="16" height="16" fill="{IDLE}"/>"#, cx - 8.0, cy - 8.0);
    for (dx, dy, w, h, b) in [
        (-8.0, -26.0, 16.0, 18.0, Button::DpadUp),
        (-8.0, 8.0, 16.0, 18.0, Button::DpadDown),
        (-26.0, -8.0, 18.0, 16.0, Button::DpadLeft),
        (8.0, -8.0, 18.0, 16.0, Button::DpadRight),
    ] {
        let _ = write!(
            s,
            r#"<rect x="{}" y="{}" width="{w}" height="{h}" rx="3" fill="{}" stroke="{IDLE_EDGE}" stroke-width="1.5"/>"#,
            cx + dx,
            cy + dy,
            fill(b)
        );
    }

    // Face buttons, colored like an Xbox pad.
    let (fx, fy) = (302.0, 118.0);
    for (dx, dy, b, label, color) in [
        (0.0, 22.0, Button::South, "A", "#3fb950"),
        (22.0, 0.0, Button::East, "B", "#f85149"),
        (-22.0, 0.0, Button::West, "X", "#58a6ff"),
        (0.0, -22.0, Button::North, "Y", "#d29922"),
    ] {
        let (bg, fg) = if pressed(b) { (color, IDLE) } else { (IDLE, color) };
        let (x, y) = (fx + dx, fy + dy);
        let _ = write!(
            s,
            r#"<circle cx="{x}" cy="{y}" r="11" fill="{bg}" stroke="{color}" stroke-width="2"/><text x="{x}" y="{}" font-size="12" font-weight="bold" text-anchor="middle" fill="{fg}" font-family="sans-serif">{label}</text>"#,
            y + 4.0
        );
    }

    // Select, Guide, Start.
    for (x, y, r, b) in [
        (182.0, 118.0, 7.0, Button::Select),
        (210.0, 88.0, 13.0, Button::Guide),
        (238.0, 118.0, 7.0, Button::Start),
    ] {
        let _ = write!(
            s,
            r#"<circle cx="{x}" cy="{y}" r="{r}" fill="{}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#,
            fill(b)
        );
    }

    s.push_str("</svg>");
    s
}

fn stick(s: &mut String, cx: f32, cy: f32, (x, y): (f32, f32), clicked: bool) {
    let (tx, ty) = (cx + x.clamp(-1.0, 1.0) * STICK_TRAVEL, cy + y.clamp(-1.0, 1.0) * STICK_TRAVEL);
    let cap = if clicked { ACTIVE } else { "#3a3f48" };
    let _ = write!(
        s,
        r#"<circle cx="{cx}" cy="{cy}" r="27" fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="2"/><circle cx="{tx}" cy="{ty}" r="18" fill="{cap}" stroke="{IDLE_EDGE}" stroke-width="2"/><circle cx="{tx}" cy="{ty}" r="11" fill="none" stroke="{IDLE_EDGE}" stroke-width="1.5"/>"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressed_button_and_stick_change_the_drawing() {
        let rest = render(None);
        let input = InputSnapshot {
            device: "Pad".into(),
            buttons: vec![Button::South],
            left_stick: (1.0, 0.0),
            right_stick: (0.0, 0.0),
            left_trigger: 0.0,
            right_trigger: 1.0,
        };
        let live = render(Some(&input));
        assert_ne!(rest, live);
        // Left stick cap moved fully right.
        assert!(live.contains(r#"cx="131" cy="118" r="18""#));
        // Right trigger filled.
        assert!(live.contains(r##"height="30" rx="6" fill="#4ea1ff""##));
    }

    #[test]
    fn output_is_well_formed_svg() {
        let svg = render(None);
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
        assert_eq!(svg.matches("<text").count(), svg.matches("</text>").count());
    }
}

//! Builds an SVG drawing of a gamepad reflecting live input. Text (button letters, mapping
//! labels) is not part of the SVG, since iced's SVG renderer may draw no text; the GUI
//! places it on top using the positions exported here.

use std::fmt::Write;

use crate::{
    config::{Button, Trigger},
    ipc::InputSnapshot,
};

/// Something on the drawing that can carry a mapping label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Button(Button),
    Trigger(Trigger),
}

const BODY: &str = "#30343c";
const BODY_EDGE: &str = "#4a505a";
const IDLE: &str = "#1d2026";
const IDLE_EDGE: &str = "#5d6470";
const ACTIVE: &str = "#4ea1ff";

/// How far (in SVG units) a fully deflected thumbstick cap travels from center.
const STICK_TRAVEL: f32 = 13.0;
const TRIGGER_HEIGHT: f32 = 30.0;

/// Margin on each side of the controller for mapping labels.
pub const MARGIN: f32 = 110.0;
/// Size of the whole drawing, in SVG units (shown 1:1 in logical pixels).
pub const WIDTH: f32 = 420.0 + 2.0 * MARGIN;
pub const HEIGHT: f32 = 275.0;
/// Width of each label column; label lines end at its inner edge.
pub const LABEL_COLUMN: f32 = MARGIN - 6.0;
/// Where the right label column starts.
pub const RIGHT_COLUMN_X: f32 = MARGIN + 426.0;
const LABEL_SPACING: f32 = 29.0;
/// Longest label text, so it fits its column.
const LABEL_CHARS: usize = 16;

const FACE_CENTER: (f32, f32) = (302.0, 118.0);
struct FaceButton {
    button: Button,
    /// Offset from `FACE_CENTER`.
    offset: (f32, f32),
    letter: &'static str,
    /// Xbox color.
    rgb: [u8; 3],
}

const FACE_BUTTONS: [FaceButton; 4] = [
    FaceButton { button: Button::South, offset: (0.0, 22.0), letter: "A", rgb: [0x3f, 0xb9, 0x50] },
    FaceButton { button: Button::East, offset: (22.0, 0.0), letter: "B", rgb: [0xf8, 0x51, 0x49] },
    FaceButton { button: Button::West, offset: (-22.0, 0.0), letter: "X", rgb: [0x58, 0xa6, 0xff] },
    FaceButton { button: Button::North, offset: (0.0, -22.0), letter: "Y", rgb: [0xd2, 0x99, 0x22] },
];

/// Text to draw over the SVG, centered at (`x`, `y`) in drawing coordinates.
pub struct Overlay {
    pub x: f32,
    pub y: f32,
    pub text: &'static str,
    pub color: [u8; 3],
}

/// A mapping label's text and row; it sits in the left or right column.
pub struct PlacedLabel {
    pub text: String,
    pub y: f32,
    pub right: bool,
    anchor: (f32, f32),
}

/// Where each label's leader line meets its button (before the margin offset), split into
/// the column its label goes in, top to bottom. Buttons with a letter on them are met at
/// their edge so the line doesn't cover the letter.
const LEFT_LABELS: [(Spot, (f32, f32)); 8] = [
    (Spot::Trigger(Trigger::Left), (95.0, 21.0)),
    (Spot::Button(Button::LeftBumper), (112.0, 47.0)),
    (Spot::Button(Button::Select), (182.0, 118.0)),
    (Spot::Button(Button::LeftStick), (118.0, 118.0)),
    (Spot::Button(Button::DpadUp), (165.0, 150.0)),
    (Spot::Button(Button::DpadLeft), (147.0, 168.0)),
    (Spot::Button(Button::DpadRight), (183.0, 168.0)),
    (Spot::Button(Button::DpadDown), (165.0, 186.0)),
];
const RIGHT_LABELS: [(Spot, (f32, f32)); 9] = [
    (Spot::Trigger(Trigger::Right), (325.0, 21.0)),
    (Spot::Button(Button::RightBumper), (308.0, 47.0)),
    (Spot::Button(Button::Guide), (210.0, 88.0)),
    (Spot::Button(Button::North), (313.0, 96.0)),
    (Spot::Button(Button::West), (280.0, 129.0)),
    (Spot::Button(Button::East), (335.0, 118.0)),
    (Spot::Button(Button::South), (313.0, 140.0)),
    (Spot::Button(Button::Start), (238.0, 118.0)),
    (Spot::Button(Button::RightStick), (255.0, 168.0)),
];

/// Renders the controller, with `labels` (text per button) drawn in the side margins.
/// With no input everything is drawn at rest.
pub fn render(input: Option<&InputSnapshot>, labels: &[(Spot, String)]) -> String {
    let pressed = |b: Button| input.is_some_and(|i| i.buttons.contains(&b));
    let fill = |b: Button| if pressed(b) { ACTIVE } else { IDLE };
    let (ls, rs, lt, rt) = input
        .map(|i| (i.left_stick, i.right_stick, i.left_trigger, i.right_trigger))
        .unwrap_or(((0.0, 0.0), (0.0, 0.0), 0.0, 0.0));

    let mut s = String::with_capacity(8192);
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} 275"><g transform="translate({MARGIN},0)">"#,
        420.0 + 2.0 * MARGIN
    );

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

    // Face buttons, colored like an Xbox pad (letters come from `overlays`).
    for face in &FACE_BUTTONS {
        let [r, g, bl] = face.rgb;
        let color = format!("#{r:02x}{g:02x}{bl:02x}");
        let bg = if pressed(face.button) { color.as_str() } else { IDLE };
        let (x, y) = (FACE_CENTER.0 + face.offset.0, FACE_CENTER.1 + face.offset.1);
        let _ = write!(
            s,
            r#"<circle cx="{x}" cy="{y}" r="11" fill="{bg}" stroke="{color}" stroke-width="2"/>"#
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

    s.push_str("</g>");
    draw_labels(&mut s, labels);
    s.push_str("</svg>");
    s
}

/// Lays out mapping labels top to bottom in each margin column.
pub fn place_labels(labels: &[(Spot, String)]) -> Vec<PlacedLabel> {
    let text_for = |b: Spot| labels.iter().find(|(l, _)| *l == b).map(|(_, t)| t.as_str());
    let mut placed = Vec::new();
    for (right, anchors) in [(false, &LEFT_LABELS[..]), (true, &RIGHT_LABELS[..])] {
        let mut row = 0.0;
        for (spot, (ax, ay)) in anchors {
            let Some(label) = text_for(*spot) else { continue };
            let mut text: String = label.chars().take(LABEL_CHARS).collect();
            if label.chars().count() > LABEL_CHARS {
                text.pop();
                text.push('…');
            }
            placed.push(PlacedLabel { text, y: 10.0 + row * LABEL_SPACING, right, anchor: (ax + MARGIN, *ay) });
            row += 1.0;
        }
    }
    placed
}

/// Button letters and trigger names, drawn by the GUI over the SVG.
pub fn overlays(input: Option<&InputSnapshot>) -> Vec<Overlay> {
    const LABEL_RGB: [u8; 3] = [0xc9, 0xd1, 0xd9];
    const IDLE_RGB: [u8; 3] = [0x1d, 0x20, 0x26];
    let pressed = |b: Button| input.is_some_and(|i| i.buttons.contains(&b));
    let mut list: Vec<Overlay> = FACE_BUTTONS
        .iter()
        .map(|f| Overlay {
            x: FACE_CENTER.0 + f.offset.0 + MARGIN,
            y: FACE_CENTER.1 + f.offset.1,
            text: f.letter,
            color: if pressed(f.button) { IDLE_RGB } else { f.rgb },
        })
        .collect();
    list.push(Overlay { x: 115.0 + MARGIN, y: 21.0, text: "LT", color: LABEL_RGB });
    list.push(Overlay { x: 305.0 + MARGIN, y: 21.0, text: "RT", color: LABEL_RGB });
    list
}

/// Leader lines from each label's column edge to its button.
fn draw_labels(s: &mut String, labels: &[(Spot, String)]) {
    for l in place_labels(labels) {
        let line_x = if l.right { RIGHT_COLUMN_X } else { LABEL_COLUMN };
        let (ax, ay) = l.anchor;
        let _ = write!(
            s,
            r#"<line x1="{line_x}" y1="{}" x2="{ax}" y2="{ay}" stroke="{ACTIVE}" stroke-width="1" stroke-opacity="0.55"/><circle cx="{ax}" cy="{ay}" r="2.5" fill="{ACTIVE}"/>"#,
            l.y + 10.0
        );
    }
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
        let rest = render(None, &[]);
        let input = InputSnapshot {
            device: "Pad".into(),
            buttons: vec![Button::South],
            left_stick: (1.0, 0.0),
            right_stick: (0.0, 0.0),
            left_trigger: 0.0,
            right_trigger: 1.0,
            gyro: None,
        };
        let live = render(Some(&input), &[]);
        assert_ne!(rest, live);
        // Left stick cap moved fully right.
        assert!(live.contains(r#"cx="131" cy="118" r="18""#));
        // Right trigger filled.
        assert!(live.contains(r##"height="30" rx="6" fill="#4ea1ff""##));
    }

    #[test]
    fn output_is_well_formed_svg() {
        let labels = [(Spot::Button(Button::South), "Jump & <run>".into())];
        let svg = render(None, &labels);
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
        assert!(!svg.contains("<text"), "text is drawn by the GUI, not the SVG");
        assert_eq!(svg.matches("<g").count(), svg.matches("</g>").count());
        assert_eq!(svg.matches("<line").count(), 1, "one leader line per label");
    }

    #[test]
    fn labels_fill_columns_top_down_and_truncate() {
        let labels = [
            (Spot::Button(Button::South), "Left click".to_string()),
            (Spot::Trigger(Trigger::Left), "A very long mapping description".to_string()),
            (Spot::Button(Button::DpadUp), "Up".to_string()),
        ];
        let placed = place_labels(&labels);
        let left: Vec<&str> = placed.iter().filter(|l| !l.right).map(|l| l.text.as_str()).collect();
        let right: Vec<&str> = placed.iter().filter(|l| l.right).map(|l| l.text.as_str()).collect();
        assert_eq!(left, ["A very long map…", "Up"]);
        assert_eq!(right, ["Left click"]);
        // Rows are packed: the second label in a column sits one row below the first.
        let ys: Vec<f32> = placed.iter().filter(|l| !l.right).map(|l| l.y).collect();
        assert_eq!(ys[1] - ys[0], LABEL_SPACING);
    }

    #[test]
    fn pressed_face_button_letter_switches_color() {
        let input = InputSnapshot {
            device: "Pad".into(),
            buttons: vec![Button::South],
            left_stick: (0.0, 0.0),
            right_stick: (0.0, 0.0),
            left_trigger: 0.0,
            right_trigger: 0.0,
            gyro: None,
        };
        let a = |o: Vec<Overlay>| o.into_iter().find(|o| o.text == "A").unwrap().color;
        assert_ne!(a(overlays(None)), a(overlays(Some(&input))));
    }
}

//! Builds an SVG drawing of a gamepad reflecting live input. Text (button letters, mapping
//! labels) is not part of the SVG, since iced's SVG renderer may draw no text; the GUI
//! places it on top using the positions exported here.

use std::fmt::Write;

mod body;

use crate::{
    config::{Button, Trigger},
    info::{Glyphs, PadModel, Segment, button_glyph, trigger_glyph},
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
/// Font size of label text, in logical pixels.
pub const LABEL_TEXT_SIZE: f32 = 11.0;
/// Room for label text inside its pill: the column less the pill's padding and border.
const LABEL_TEXT_WIDTH: f32 = LABEL_COLUMN - 14.0;

const FACE_CENTER: (f32, f32) = (302.0, 118.0);
/// The face buttons, each with its offset from `FACE_CENTER`.
const FACE_BUTTONS: [(Button, (f32, f32)); 4] = [
    (Button::South, (0.0, 22.0)),
    (Button::East, (22.0, 0.0)),
    (Button::West, (-22.0, 0.0)),
    (Button::North, (0.0, -22.0)),
];

/// How a face button is drawn: its label and color for the controller's family (as the overlays
/// draw it), and whether that color fills it when pressed. Nintendo's have no color, so they're
/// plain light rings that light up blue.
fn face_look(b: Button, glyphs: Glyphs) -> (String, [u8; 3], bool) {
    match button_glyph(b, glyphs.family, glyphs.nintendo_layout) {
        Segment::Glyph { label, fill: Some(rgb), .. } => (label, rgb, true),
        Segment::Glyph { label, .. } => (label, [0xc9, 0xd1, 0xd9], false),
        _ => (String::new(), [0xc9, 0xd1, 0xd9], false),
    }
}

/// Text to draw over the SVG, centered at (`x`, `y`) in drawing coordinates.
pub struct Overlay {
    pub x: f32,
    pub y: f32,
    pub text: String,
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
/// the column its label goes in, top to bottom in the order that keeps the lines from
/// crossing (X, boxed in by Y, B and A, comes after them). Buttons with a letter on them are
/// met at their edge so the line doesn't cover the letter.
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
    (Spot::Button(Button::East), (335.0, 118.0)),
    (Spot::Button(Button::South), (313.0, 140.0)),
    (Spot::Button(Button::West), (280.0, 129.0)),
    (Spot::Button(Button::Start), (238.0, 118.0)),
    (Spot::Button(Button::RightStick), (255.0, 168.0)),
];

/// Renders the controller of `model` (the generic one when `None`), with `labels` (text per
/// button) drawn in the side margins. With no input everything is drawn at rest.
#[expect(clippy::too_many_lines, reason = "predates the size lints")]
pub fn render(input: Option<&InputSnapshot>, model: Option<PadModel>, glyphs: Glyphs, labels: &[(Spot, String)]) -> String {
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

    // Body, and the marks that tell the models apart.
    s.push_str(&body::outline(model));
    s.push_str(&body::marks(model));

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

    // Face buttons, colored as the controller's family colors them (letters come from `overlays`).
    for (button, offset) in FACE_BUTTONS {
        let (_, [r, g, bl], filled) = face_look(button, glyphs);
        let color = format!("#{r:02x}{g:02x}{bl:02x}");
        let lit = if filled { color.as_str() } else { ACTIVE };
        let bg = if pressed(button) { lit } else { IDLE };
        let (x, y) = (FACE_CENTER.0 + offset.0, FACE_CENTER.1 + offset.1);
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
            placed.push(PlacedLabel { text: fit_label(label), y: 10.0 + row * LABEL_SPACING, right, anchor: (ax + MARGIN, *ay) });
            row += 1.0;
        }
    }
    placed
}

/// Shortens `label` with an ellipsis so it fits on one line of its column.
fn fit_label(label: &str) -> String {
    let budget = LABEL_TEXT_WIDTH / LABEL_TEXT_SIZE;
    if label.chars().map(glyph_width).sum::<f32>() <= budget {
        return label.to_string();
    }
    let mut used = glyph_width('…');
    let mut text: String = label
        .chars()
        .take_while(|&c| {
            used += glyph_width(c);
            used <= budget
        })
        .collect();
    text.truncate(text.trim_end().len());
    text.push('…');
    text
}

/// Rough advance width of `c` in ems (measured on Noto Sans), erring wide so a label never
/// needs a second line.
fn glyph_width(c: char) -> f32 {
    match c {
        'i' | 'j' | 'l' | '.' | ',' | ':' | ';' | '\'' | '!' | ' ' => 0.32,
        'f' | 't' | 'r' | 'I' | '(' | ')' | '[' | ']' | '“' | '”' | '‘' | '’' | '"' | '-' => 0.45,
        'm' | 'w' | 'M' | 'W' | '@' | '%' | '…' => 0.95,
        c if c.is_ascii_lowercase() => 0.62,
        _ => 0.8,
    }
}

/// Button letters and trigger names, drawn by the GUI over the SVG.
pub fn overlays(input: Option<&InputSnapshot>, glyphs: Glyphs) -> Vec<Overlay> {
    const LABEL_RGB: [u8; 3] = [0xc9, 0xd1, 0xd9];
    const IDLE_RGB: [u8; 3] = [0x1d, 0x20, 0x26];
    let pressed = |b: Button| input.is_some_and(|i| i.buttons.contains(&b));
    let mut list: Vec<Overlay> = FACE_BUTTONS
        .into_iter()
        .map(|(button, offset)| {
            let (text, rgb, _) = face_look(button, glyphs);
            Overlay {
                x: FACE_CENTER.0 + offset.0 + MARGIN,
                y: FACE_CENTER.1 + offset.1,
                text,
                color: if pressed(button) { IDLE_RGB } else { rgb },
            }
        })
        .collect();
    for (x, t) in [(115.0, Trigger::Left), (305.0, Trigger::Right)] {
        let text = match trigger_glyph(t, glyphs.family) {
            Segment::Glyph { label, .. } => label,
            _ => String::new(),
        };
        list.push(Overlay { x: x + MARGIN, y: 21.0, text, color: LABEL_RGB });
    }
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
        let rest = render(None, None, Glyphs::default(), &[]);
        let input = InputSnapshot {
            device: "Pad".into(),
            model: None,
            family: None,
            buttons: vec![Button::South],
            left_stick: (1.0, 0.0),
            right_stick: (0.0, 0.0),
            left_trigger: 0.0,
            right_trigger: 1.0,
            gyro: None,
        };
        let live = render(Some(&input), None, Glyphs::default(), &[]);
        assert_ne!(rest, live);
        // Left stick cap moved fully right.
        assert!(live.contains(r#"cx="131" cy="118" r="18""#));
        // Right trigger filled.
        assert!(live.contains(r##"height="30" rx="6" fill="#4ea1ff""##));
    }

    #[test]
    fn letters_follow_the_controllers_family_and_layout() {
        use crate::info::PadFamily::{Nintendo, PlayStation, Xbox};
        let texts = |family, nintendo_layout| -> Vec<String> {
            overlays(None, Glyphs { family, nintendo_layout }).into_iter().map(|o| o.text).collect()
        };
        // South, East, West, North, then the triggers.
        assert_eq!(texts(Xbox, false), ["A", "B", "X", "Y", "LT", "RT"]);
        assert_eq!(texts(Nintendo, false), ["B", "A", "Y", "X", "ZL", "ZR"]);
        assert_eq!(texts(PlayStation, false), ["✕", "○", "□", "△", "L2", "R2"]);
        assert_eq!(texts(Xbox, true), ["B", "A", "Y", "X", "LT", "RT"], "the Nintendo layout swaps an Xbox pad's labels");
    }

    #[test]
    fn face_button_colors_follow_the_family() {
        let drawing = |family| render(None, None, Glyphs { family, nintendo_layout: false }, &[]);
        assert!(drawing(crate::info::PadFamily::Xbox).contains("#3ca03c"), "A is green on Xbox");
        assert!(!drawing(crate::info::PadFamily::Nintendo).contains("#3ca03c"));
        assert_ne!(drawing(crate::info::PadFamily::PlayStation), drawing(crate::info::PadFamily::Xbox));
    }

    #[test]
    fn each_model_has_its_own_picture() {
        let models = [None, Some(PadModel::DualShock4), Some(PadModel::DualSense), Some(PadModel::ProController)];
        let drawings: Vec<String> = models.iter().map(|m| render(None, *m, Glyphs::default(), &[])).collect();
        for (i, a) in drawings.iter().enumerate() {
            for b in &drawings[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn output_is_well_formed_svg() {
        let labels = [(Spot::Button(Button::South), "Jump & <run>".into())];
        let svg = render(None, None, Glyphs::default(), &labels);
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
        assert_eq!(left, ["A very long m…", "Up"]);
        assert_eq!(right, ["Left click"]);
        // Rows are packed: the second label in a column sits one row below the first.
        let ys: Vec<f32> = placed.iter().filter(|l| !l.right).map(|l| l.y).collect();
        assert_eq!(ys[1] - ys[0], LABEL_SPACING);
    }

    #[test]
    fn wide_letters_are_cut_sooner_than_narrow_ones() {
        assert_eq!(fit_label("Menu “Augmentations”"), "Menu “Augme…");
        assert_eq!(fit_label("Hold: fill all"), "Hold: fill all");
        assert_eq!(fit_label("Keypad PLUS"), "Keypad PLUS");
    }

    #[test]
    fn pressed_face_button_letter_switches_color() {
        let input = InputSnapshot {
            device: "Pad".into(),
            model: None,
            family: None,
            buttons: vec![Button::South],
            left_stick: (0.0, 0.0),
            right_stick: (0.0, 0.0),
            left_trigger: 0.0,
            right_trigger: 0.0,
            gyro: None,
        };
        let a = |o: Vec<Overlay>| o.into_iter().find(|o| o.text == "A").unwrap().color;
        assert_ne!(a(overlays(None, Glyphs::default())), a(overlays(Some(&input), Glyphs::default())));
    }
}

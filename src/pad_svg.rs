//! Builds an SVG drawing of a gamepad reflecting live input. Text (button letters, mapping
//! labels) is not part of the SVG, since iced's SVG renderer may draw no text; the GUI
//! places it on top using the positions exported here.

use std::fmt::Write;

mod hit;
mod labels;
mod layout;

pub use hit::{Part, hit, stick_value, trigger_value};
#[cfg(test)]
pub use hit::{face_center, stick_center};
pub use labels::{HEIGHT, LABEL_COLUMN, LABEL_TEXT_SIZE, MARGIN, RIGHT_COLUMN_X, WIDTH, place_labels};
use layout::{DpadKind, Layout, Mark};

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

const BODY_EDGE: &str = "#4a505a";
/// The shell's shading: the body is filled with a gradient from `SHELL_TOP` down to
/// `SHELL_BOTTOM` (around the flat `#30343c` it replaces) so it reads as a molded shape
/// rather than a flat cutout.
const SHELL_TOP: &str = "#373d47";
const SHELL_BOTTOM: &str = "#2c3039";
const IDLE: &str = "#1d2026";
const IDLE_EDGE: &str = "#5d6470";
const ACTIVE: &str = "#4ea1ff";
/// The guide/home button, which stands out from the small buttons around it on the real pads.
const GUIDE: &str = "#3a3f48";

const TRIGGER_HEIGHT: f32 = 30.0;
/// The blue the pads glow in: the light strips beside a DualSense's touchpad, the light bar
/// above a DualShock 4's.
const LIGHT: &str = "#4ea1ff";
/// How much of the bumper shows above the body's top edge, and how far it reaches down past
/// it. The body is drawn over that lower part, so the bar follows an edge that curves away
/// under its ends instead of floating above it.
const BUMPER_ABOVE: f32 = 16.0;
const BUMPER_BELOW: f32 = 10.0;
const BUMPER_HEIGHT: f32 = BUMPER_ABOVE + BUMPER_BELOW;

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

/// Renders the controller of `model` (the generic one when `None`), with `labels` (text per
/// button) drawn in the side margins. With no input everything is drawn at rest.
pub fn render(input: Option<&InputSnapshot>, model: Option<PadModel>, glyphs: Glyphs, labels: &[(Spot, String)]) -> String {
    let layout = Layout::of(model);
    let pressed = |b: Button| input.is_some_and(|i| i.buttons.contains(&b));
    let (ls, rs, lt, rt) = input
        .map(|i| (i.left_stick, i.right_stick, i.left_trigger, i.right_trigger))
        .unwrap_or(((0.0, 0.0), (0.0, 0.0), 0.0, 0.0));

    let mut s = String::with_capacity(8192);
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {WIDTH} {HEIGHT}"><defs><linearGradient id="g-shell" x1="0" y1="46" x2="0" y2="274" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="{SHELL_TOP}"/><stop offset="1" stop-color="{SHELL_BOTTOM}"/></linearGradient></defs><g transform="translate({MARGIN},0)">"#
    );
    shoulders(&mut s, layout, [(lt, Button::LeftBumper), (rt, Button::RightBumper)], pressed);

    // Body, and the marks that tell the models apart.
    let _ = write!(s, r#"<path d="{}" fill="url(#g-shell)" stroke="{BODY_EDGE}" stroke-width="3"/>"#, layout.outline);
    for mark in layout.marks {
        match *mark {
            Mark::Rect(x, y, w, h, rx) => {
                let _ = write!(s, r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#);
            }
            Mark::Circle(x, y, r) => {
                let _ = write!(s, r#"<circle cx="{x}" cy="{y}" r="{r}" fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="1.5"/>"#);
            }
            Mark::Strip(x, y, w, h, rx, fill) => {
                let _ = write!(s, r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}"/>"#);
            }
        }
    }

    stick(&mut s, layout.left_stick, layout.stick_r, ls, pressed(Button::LeftStick));
    stick(&mut s, layout.right_stick, layout.stick_r, rs, pressed(Button::RightStick));
    dpad(&mut s, layout, pressed);

    // Face buttons, colored as the controller's family colors them (letters come from `overlays`).
    for (button, (x, y)) in face_positions(layout) {
        let (_, [r, g, bl], filled) = face_look(button, glyphs);
        let color = format!("#{r:02x}{g:02x}{bl:02x}");
        let lit = if filled { color.as_str() } else { ACTIVE };
        let bg = if pressed(button) { lit } else { IDLE };
        let _ = write!(
            s,
            r#"<circle cx="{x}" cy="{y}" r="{}" fill="{bg}" stroke="{color}" stroke-width="2"/>"#,
            layout.face_r
        );
    }

    // Select, Guide, Start.
    for (b, (x, y, r)) in [(Button::Select, layout.select), (Button::Guide, layout.guide), (Button::Start, layout.start)] {
        let fill = match (pressed(b), b) {
            (true, _) => ACTIVE,
            (false, Button::Guide) => GUIDE,
            _ => IDLE,
        };
        let _ = write!(s, r#"<circle cx="{x}" cy="{y}" r="{r}" fill="{fill}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#);
    }

    s.push_str("</g>");
    labels::draw_labels(&mut s, labels, model);
    s.push_str("</svg>");
    s
}

/// The face buttons with their centers: south, east, west, north.
fn face_positions(layout: &Layout) -> [(Button, (f32, f32)); 4] {
    let [south, east, west, north] = layout.face_offsets();
    let at = |(dx, dy): (f32, f32)| (layout.face.0 + dx, layout.face.1 + dy);
    [(Button::South, at(south)), (Button::East, at(east)), (Button::West, at(west)), (Button::North, at(north))]
}

/// Triggers (an outline with a fill that rises with pressure) above the bumpers.
fn shoulders(s: &mut String, layout: &Layout, sides: [(f32, Button); 2], pressed: impl Fn(Button) -> bool) {
    let w = layout.shoulder_w;
    let (trigger_top, bumper_top) = (layout.trigger_top(), layout.bumper_top());
    for ((value, bumper), cx) in sides.into_iter().zip([layout.shoulders.0, layout.shoulders.1]) {
        let (x, tw) = (cx - w / 4.0, w / 2.0);
        let h = value.clamp(0.0, 1.0) * TRIGGER_HEIGHT;
        let _ = write!(
            s,
            r#"<rect x="{x}" y="{trigger_top}" width="{tw}" height="{TRIGGER_HEIGHT}" rx="8" fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#
        );
        if h > 0.5 {
            let _ = write!(
                s,
                r#"<rect x="{x}" y="{y}" width="{tw}" height="{h}" rx="6" fill="{ACTIVE}"/>"#,
                y = trigger_top + TRIGGER_HEIGHT - h
            );
        }
        let fill = if pressed(bumper) { ACTIVE } else { IDLE };
        let _ = write!(
            s,
            r#"<rect x="{}" y="{bumper_top}" width="{w}" height="{BUMPER_HEIGHT}" rx="7" fill="{fill}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#,
            cx - w / 2.0
        );
    }
}

/// The D-pad, in the shape the model has it; each direction lights up when pressed.
fn dpad(s: &mut String, layout: &Layout, pressed: impl Fn(Button) -> bool) {
    let (cx, cy) = layout.dpad;
    let reach = layout.dpad_reach;
    let half = reach * 8.0 / 26.0;
    let arm = reach * 18.0 / 26.0;
    let fill = |b: Button| if pressed(b) { ACTIVE } else { IDLE };
    let arms = [
        (Button::DpadUp, (0.0, -arm)),
        (Button::DpadDown, (0.0, arm)),
        (Button::DpadLeft, (-arm, 0.0)),
        (Button::DpadRight, (arm, 0.0)),
    ];
    match layout.dpad_kind {
        DpadKind::Cross => {
            let _ = write!(s, r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{IDLE}"/>"#, cx - half, cy - half, 2.0 * half, 2.0 * half);
            for (b, (dx, dy)) in arms {
                // Each arm runs from the center square out to `reach`.
                let (w, h) = if dx == 0.0 { (2.0 * half, reach - half) } else { (reach - half, 2.0 * half) };
                let x = if dx < 0.0 { cx - reach } else if dx > 0.0 { cx + half } else { cx - half };
                let y = if dy < 0.0 { cy - reach } else if dy > 0.0 { cy + half } else { cy - half };
                let _ = write!(
                    s,
                    r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{}" fill="{}" stroke="{IDLE_EDGE}" stroke-width="1.5"/>"#,
                    half * 3.0 / 8.0,
                    fill(b)
                );
            }
        }
        DpadKind::Disc => {
            let _ = write!(s, r#"<circle cx="{cx}" cy="{cy}" r="{reach}" fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#);
            for (b, (dx, dy)) in arms {
                let (w, h) = if dx == 0.0 { (1.6 * half, 0.9 * arm) } else { (0.9 * arm, 1.6 * half) };
                let _ = write!(
                    s,
                    r#"<rect x="{}" y="{}" width="{w}" height="{h}" rx="2" fill="{}" stroke="{IDLE_EDGE}" stroke-width="1.5"/>"#,
                    cx + dx * 0.85 - w / 2.0,
                    cy + dy * 0.85 - h / 2.0,
                    fill(b)
                );
            }
        }
        DpadKind::Buttons => {
            for (b, (dx, dy)) in arms {
                let _ = write!(
                    s,
                    r#"<circle cx="{}" cy="{}" r="{}" fill="{}" stroke="{IDLE_EDGE}" stroke-width="1.5"/>"#,
                    cx + dx,
                    cy + dy,
                    reach * 0.35,
                    fill(b)
                );
            }
        }
    }
}

/// Button letters and trigger names, drawn by the GUI over the SVG.
pub fn overlays(input: Option<&InputSnapshot>, model: Option<PadModel>, glyphs: Glyphs) -> Vec<Overlay> {
    const LABEL_RGB: [u8; 3] = [0xc9, 0xd1, 0xd9];
    const IDLE_RGB: [u8; 3] = [0x1d, 0x20, 0x26];
    let layout = Layout::of(model);
    let pressed = |b: Button| input.is_some_and(|i| i.buttons.contains(&b));
    let mut list: Vec<Overlay> = face_positions(layout)
        .into_iter()
        .map(|(button, (x, y))| {
            let (text, rgb, _) = face_look(button, glyphs);
            Overlay { x: x + MARGIN, y, text, color: if pressed(button) { IDLE_RGB } else { rgb } }
        })
        .collect();
    for (x, t) in [(layout.shoulders.0, Trigger::Left), (layout.shoulders.1, Trigger::Right)] {
        let text = match trigger_glyph(t, glyphs.family) {
            Segment::Glyph { label, .. } => label,
            _ => String::new(),
        };
        list.push(Overlay { x: x + MARGIN, y: layout.trigger_top() + TRIGGER_HEIGHT / 2.0, text, color: LABEL_RGB });
    }
    list
}

/// A thumbstick: its well, and the cap pushed off center by `(x, y)` (-1 to 1 each way).
fn stick(s: &mut String, (cx, cy): (f32, f32), r: f32, (x, y): (f32, f32), clicked: bool) {
    let travel = r * 13.0 / 27.0;
    let (tx, ty) = (cx + x.clamp(-1.0, 1.0) * travel, cy + y.clamp(-1.0, 1.0) * travel);
    let cap_r = r * 2.0 / 3.0;
    let cap = if clicked { ACTIVE } else { "#3a3f48" };
    let _ = write!(
        s,
        r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="2"/><circle cx="{tx}" cy="{ty}" r="{cap_r}" fill="{cap}" stroke="{IDLE_EDGE}" stroke-width="2"/><circle cx="{tx}" cy="{ty}" r="{}" fill="none" stroke="{IDLE_EDGE}" stroke-width="1.5"/>"#,
        cap_r * 11.0 / 18.0
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes every model's SVG (plus the GUI-drawn letters) for eyeballing with
    /// `cargo test --bin padwight pad_svg::tests::dump -- --ignored`. Previews land in
    /// the temp dir under `padwight-pad-svg/`.
    #[test]
    #[ignore = "dev convenience: writes previews to a temp dir"]
    fn dump() {
        let dir = std::env::temp_dir().join("padwight-pad-svg");
        std::fs::create_dir_all(&dir).unwrap();
        let mut models: Vec<Option<crate::info::PadModel>> = vec![None];
        models.extend(crate::info::PadModel::ALL.map(Some));
        for model in models {
            let name = model.map_or_else(|| "generic".into(), |m| m.slug().to_string());
            let glyphs = Glyphs { family: model.map_or(crate::info::PadFamily::Xbox, crate::info::PadModel::family), nintendo_layout: false };
            let svg = render(None, model, glyphs, &[]);
            // Preview only: the GUI draws the letters as widgets, so add them to see the picture whole.
            let mut with_text = String::new();
            for o in overlays(None, model, glyphs) {
                let [r, g, b] = o.color;
                let _ = write!(
                    with_text,
                    r##"<text x="{}" y="{}" font-size="12" font-family="sans-serif" text-anchor="middle" dominant-baseline="central" fill="#{r:02x}{g:02x}{b:02x}">{}</text>"##,
                    o.x, o.y, o.text
                );
            }
            let svg = svg.replace("</svg>", &format!("{with_text}</svg>"));
            std::fs::write(dir.join(format!("{name}.svg")), svg).unwrap();
        }
    }

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
            overlays(None, None, Glyphs { family, nintendo_layout }).into_iter().map(|o| o.text).collect()
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
        let models = [
            None,
            Some(PadModel::DualShock4),
            Some(PadModel::DualSense),
            Some(PadModel::DualSenseEdge),
            Some(PadModel::ProController),
            Some(PadModel::Switch2Pro),
            Some(PadModel::JoyCons),
            Some(PadModel::JoyCons2),
            Some(PadModel::Xbox360),
            Some(PadModel::XboxOne),
            Some(PadModel::XboxSeries),
            Some(PadModel::XboxElite),
            Some(PadModel::SteamController),
            Some(PadModel::WiiUPro),
        ];
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
        assert_eq!(svg.matches("<line ").count(), 1, "one leader line per label");
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
        assert_ne!(a(overlays(None, None, Glyphs::default())), a(overlays(Some(&input), None, Glyphs::default())));
    }
}

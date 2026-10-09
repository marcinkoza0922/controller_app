//! The mapping labels beside the drawing: which column each goes in, where its leader line
//! meets the part it names, and how its text is shortened to fit.

use std::fmt::Write;

use super::{ACTIVE, Spot, layout::Layout};
use crate::{
    config::{Button, Trigger},
    info::PadModel,
};

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

/// A mapping label's text and row; it sits in the left or right column.
pub struct PlacedLabel {
    pub text: String,
    pub y: f32,
    pub right: bool,
    anchor: (f32, f32),
}

/// Where each label's leader line meets its part (before the margin offset), and the column its
/// label goes in. Buttons with a letter on them are met at their edge so the line doesn't cover
/// the letter.
fn anchors(l: &Layout) -> Vec<(Spot, (f32, f32), bool)> {
    use Button::{DpadDown, DpadLeft, DpadRight, DpadUp, Guide, LeftBumper, LeftStick, RightBumper, RightStick, Select, Start};
    let (lx, rx) = l.shoulders;
    let quarter = l.shoulder_w / 4.0;
    let (dx, dy) = l.dpad;
    let arm = l.dpad_reach * 18.0 / 26.0;
    let (fx, fy) = l.face;
    let [south, east, west, north] = l.face_offsets();
    let at = |(x, y, _): (f32, f32, f32)| (x, y);
    vec![
        (Spot::Trigger(Trigger::Left), (lx - quarter, 21.0), false),
        (Spot::Button(LeftBumper), (lx, 47.0), false),
        (Spot::Button(Select), at(l.select), false),
        (Spot::Button(LeftStick), l.left_stick, false),
        (Spot::Button(DpadUp), (dx, dy - arm), false),
        (Spot::Button(DpadLeft), (dx - arm, dy), false),
        (Spot::Button(DpadRight), (dx + arm, dy), false),
        (Spot::Button(DpadDown), (dx, dy + arm), false),
        (Spot::Trigger(Trigger::Right), (rx + quarter, 21.0), true),
        (Spot::Button(RightBumper), (rx, 47.0), true),
        (Spot::Button(Guide), at(l.guide), true),
        (Spot::Button(Button::North), (fx + north.0 + l.face_r, fy + north.1), true),
        (Spot::Button(Button::East), (fx + east.0 + l.face_r, fy + east.1), true),
        (Spot::Button(Button::South), (fx + south.0 + l.face_r, fy + south.1), true),
        (Spot::Button(Button::West), (fx + west.0, fy + west.1 + l.face_r), true),
        (Spot::Button(Start), at(l.start), true),
        (Spot::Button(RightStick), l.right_stick, true),
    ]
}

/// Lays out mapping labels top to bottom in each margin column, ordered by where their lines
/// meet the drawing so the lines don't cross.
pub fn place_labels(labels: &[(Spot, String)], model: Option<PadModel>) -> Vec<PlacedLabel> {
    let text_for = |b: Spot| labels.iter().find(|(l, _)| *l == b).map(|(_, t)| t.as_str());
    let mut anchors = anchors(Layout::of(model));
    anchors.sort_by(|a, b| a.1.1.total_cmp(&b.1.1).then(a.1.0.total_cmp(&b.1.0)));
    let mut placed = Vec::new();
    for right in [false, true] {
        let mut row = 0.0;
        for (spot, (ax, ay), _) in anchors.iter().filter(|a| a.2 == right) {
            let Some(label) = text_for(*spot) else { continue };
            placed.push(PlacedLabel { text: fit_label(label), y: 10.0 + row * LABEL_SPACING, right, anchor: (ax + MARGIN, *ay) });
            row += 1.0;
        }
    }
    placed
}

/// Leader lines from each label's column edge to its part.
pub fn draw_labels(s: &mut String, labels: &[(Spot, String)], model: Option<PadModel>) {
    for l in place_labels(labels, model) {
        let line_x = if l.right { RIGHT_COLUMN_X } else { LABEL_COLUMN };
        let (ax, ay) = l.anchor;
        let _ = write!(
            s,
            r#"<line x1="{line_x}" y1="{}" x2="{ax}" y2="{ay}" stroke="{ACTIVE}" stroke-width="1" stroke-opacity="0.55"/><circle cx="{ax}" cy="{ay}" r="2.5" fill="{ACTIVE}"/>"#,
            l.y + 10.0
        );
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_fill_columns_top_down_and_truncate() {
        let labels = [
            (Spot::Button(Button::South), "Left click".to_string()),
            (Spot::Trigger(Trigger::Left), "A very long mapping description".to_string()),
            (Spot::Button(Button::DpadUp), "Up".to_string()),
        ];
        let placed = place_labels(&labels, None);
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
    fn lines_follow_the_models_layout() {
        let select = [(Spot::Button(Button::Select), "Map".to_string())];
        let anchor = |model| place_labels(&select, model)[0].anchor;
        assert_ne!(anchor(None), anchor(Some(PadModel::ProController)), "Minus sits elsewhere on a Pro Controller");
    }
}

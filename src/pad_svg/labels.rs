//! The mapping labels beside the drawing: which column each goes in, where its leader line
//! meets the part it names, and how its text is shortened to fit.

use std::fmt::Write;

use super::{ACTIVE, Spot, layout::Layout};
use crate::{
    config::{Button, Trigger},
    info::PadModel,
};

/// Margin on each side of the controller for mapping labels: room for a menu's name.
pub const MARGIN: f32 = 140.0;
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
    // Meet each bar in the middle of what shows above the body.
    let trigger_y = l.trigger_top() + super::TRIGGER_HEIGHT / 2.0;
    let bumper_y = l.bumper_top() + super::BUMPER_ABOVE / 2.0;
    let (dx, dy) = l.dpad;
    let arm = l.dpad_reach * 18.0 / 26.0;
    let (fx, fy) = l.face;
    let [south, east, west, north] = l.face_offsets();
    let at = |(x, y, _): (f32, f32, f32)| (x, y);
    vec![
        (Spot::Trigger(Trigger::Left), (lx - quarter, trigger_y), false),
        (Spot::Button(LeftBumper), (lx, bumper_y), false),
        (Spot::Button(Select), at(l.select), false),
        (Spot::Button(LeftStick), l.left_stick, false),
        (Spot::Button(DpadUp), (dx, dy - arm), false),
        (Spot::Button(DpadLeft), (dx - arm, dy), false),
        (Spot::Button(DpadRight), (dx + arm, dy), false),
        (Spot::Button(DpadDown), (dx, dy + arm), false),
        (Spot::Trigger(Trigger::Right), (rx + quarter, trigger_y), true),
        (Spot::Button(RightBumper), (rx, bumper_y), true),
        (Spot::Button(Guide), at(l.guide), true),
        (Spot::Button(Button::North), (fx + north.0 + l.face_r, fy + north.1), true),
        (Spot::Button(Button::East), (fx + east.0 + l.face_r, fy + east.1), true),
        (Spot::Button(Button::South), (fx + south.0 + l.face_r, fy + south.1), true),
        (Spot::Button(Button::West), (fx + west.0, fy + west.1 + l.face_r), true),
        (Spot::Button(Start), at(l.start), true),
        (Spot::Button(RightStick), l.right_stick, true),
    ]
}

/// Lays out mapping labels in each margin column: each as near the height of its part as the
/// rows allow, ordered so that no two leader lines cross.
pub fn place_labels(labels: &[(Spot, String)], model: Option<PadModel>) -> Vec<PlacedLabel> {
    let text_for = |b: Spot| labels.iter().find(|(l, _)| *l == b).map(|(_, t)| t.as_str());
    let mut anchors = anchors(Layout::of(model));
    anchors.sort_by(|a, b| a.1.1.total_cmp(&b.1.1).then(a.1.0.total_cmp(&b.1.0)));
    let mut placed = Vec::new();
    for right in [false, true] {
        let mut column: Vec<PlacedLabel> = anchors
            .iter()
            .filter(|a| a.2 == right)
            .filter_map(|(spot, (ax, ay), _)| {
                let label = text_for(*spot)?;
                Some(PlacedLabel { text: fit_label(label), y: 0.0, right, anchor: (ax + MARGIN, *ay) })
            })
            .collect();
        let rows = column_rows(&column.iter().map(|l| l.anchor.1 - LABEL_HALF).collect::<Vec<_>>());
        for (label, y) in column.iter_mut().zip(rows) {
            label.y = y;
        }
        uncross(&mut column);
        placed.extend(column);
    }
    placed
}

/// Half a label's height: a label is centered on its line's end.
const LABEL_HALF: f32 = 10.0;
/// Where the first and last label rows may sit.
const FIRST_ROW: f32 = 4.0;
const LAST_ROW: f32 = HEIGHT - 2.0 * LABEL_HALF - 4.0;

/// Rows for labels that want to sit at `wanted` (sorted), at least a row apart and inside the
/// picture: pushed down past the one above, then back up from the bottom if they overflow.
fn column_rows(wanted: &[f32]) -> Vec<f32> {
    let mut rows: Vec<f32> = Vec::with_capacity(wanted.len());
    for &w in wanted {
        let below = rows.last().map_or(FIRST_ROW, |r| r + LABEL_SPACING);
        rows.push(w.max(below).max(FIRST_ROW));
    }
    let mut limit = LAST_ROW;
    for r in rows.iter_mut().rev() {
        *r = r.min(limit);
        limit = *r - LABEL_SPACING;
    }
    rows
}

/// Swaps the texts and anchors of labels whose leader lines cross, until none do. Each swap
/// shortens the lines in total, so it ends.
fn uncross(column: &mut [PlacedLabel]) {
    let start = |l: &PlacedLabel| (if l.right { RIGHT_COLUMN_X } else { LABEL_COLUMN }, l.y + LABEL_HALF);
    for _ in 0..column.len() * column.len() {
        let mut swapped = false;
        for i in 0..column.len() {
            for j in i + 1..column.len() {
                if crosses((start(&column[i]), column[i].anchor), (start(&column[j]), column[j].anchor)) {
                    let (a, b) = column.split_at_mut(j);
                    std::mem::swap(&mut a[i].text, &mut b[0].text);
                    std::mem::swap(&mut a[i].anchor, &mut b[0].anchor);
                    swapped = true;
                }
            }
        }
        if !swapped {
            break;
        }
    }
}

type Point = (f32, f32);

/// True if the segments properly cross (touching ends don't count).
fn crosses((p1, p2): (Point, Point), (q1, q2): (Point, Point)) -> bool {
    let side = |a: Point, b: Point, c: Point| (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
    let (d1, d2) = (side(q1, q2, p1), side(q1, q2, p2));
    let (d3, d4) = (side(p1, p2, q1), side(p1, p2, q2));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// Leader lines from each label's column edge to its part.
pub fn draw_labels(s: &mut String, labels: &[(Spot, String)], model: Option<PadModel>) {
    for l in place_labels(labels, model) {
        let line_x = if l.right { RIGHT_COLUMN_X } else { LABEL_COLUMN };
        let (ax, ay) = l.anchor;
        let _ = write!(
            s,
            r#"<line x1="{line_x}" y1="{}" x2="{ax}" y2="{ay}" stroke="{ACTIVE}" stroke-width="1.5" stroke-opacity="0.85"/><circle cx="{ax}" cy="{ay}" r="2.5" fill="{ACTIVE}"/>"#,
            l.y + LABEL_HALF
        );
    }
}

/// The leader lines alone, as a drawing the size of the whole picture: laid over the controller
/// so they keep full strength while the controller is dimmed.
pub fn leaders(labels: &[(Spot, String)], model: Option<PadModel>) -> String {
    let mut s = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {WIDTH} {HEIGHT}">"#);
    draw_labels(&mut s, labels, model);
    s.push_str("</svg>");
    s
}

/// Shortens `label` with an ellipsis so it fits on one line of its column. A quoted name that
/// is cut keeps its closing quote: “Menu “Augmentat…””.
fn fit_label(label: &str) -> String {
    let budget = LABEL_TEXT_WIDTH / LABEL_TEXT_SIZE;
    if label.chars().map(glyph_width).sum::<f32>() <= budget {
        return label.to_string();
    }
    // What follows the cut: the ellipsis, and the closing quote when the cut falls inside quotes.
    let tail = label.find('“').filter(|_| label.contains('”')).map_or("…", |_| "…”");
    let mut used: f32 = tail.chars().map(glyph_width).sum();
    let mut text: String = label
        .chars()
        .take_while(|&c| {
            used += glyph_width(c);
            used <= budget && c != '”'
        })
        .collect();
    text.truncate(text.trim_end().len());
    text.push_str(tail);
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
        assert_eq!(left, ["A very long mappi…", "Up"]);
        assert_eq!(right, ["Left click"]);
        // Rows never overlap: the second label in a column sits at least a row below the first.
        let ys: Vec<f32> = placed.iter().filter(|l| !l.right).map(|l| l.y).collect();
        assert!(ys[1] - ys[0] >= LABEL_SPACING);
    }

    #[test]
    fn wide_letters_are_cut_sooner_than_narrow_ones() {
        assert_eq!(fit_label("Menu “Augmentations”"), "Menu “Augmentat…”", "a cut name keeps its closing quote");
        assert_eq!(fit_label("Menu “Belt” +"), "Menu “Belt” +");
        assert_eq!(fit_label("Hold: fill all"), "Hold: fill all");
        assert_eq!(fit_label("Keypad PLUS"), "Keypad PLUS");
    }

    #[test]
    fn leader_lines_never_cross() {
        let every: Vec<(Spot, String)> = anchors(Layout::of(None)).into_iter().map(|(spot, ..)| (spot, "Mapped".to_string())).collect();
        for model in [None, Some(PadModel::ProController)] {
            let placed = place_labels(&every, model);
            let line = |l: &PlacedLabel| ((if l.right { RIGHT_COLUMN_X } else { LABEL_COLUMN }, l.y + LABEL_HALF), l.anchor);
            for (i, a) in placed.iter().enumerate() {
                for b in &placed[i + 1..] {
                    assert!(!crosses(line(a), line(b)), "{:?} crosses {:?}", a.anchor, b.anchor);
                }
                assert!((FIRST_ROW..=LAST_ROW).contains(&a.y), "row {} is outside the picture", a.y);
            }
        }
    }

    #[test]
    fn lines_follow_the_models_layout() {
        let select = [(Spot::Button(Button::Select), "Map".to_string())];
        let anchor = |model| place_labels(&select, model)[0].anchor;
        assert_ne!(anchor(None), anchor(Some(PadModel::ProController)), "Minus sits elsewhere on a Pro Controller");
    }
}

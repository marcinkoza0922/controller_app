//! Actions drawn from their pieces: keywords in their own colour, and controller buttons as the
//! glyphs the overlays draw, for the controller in use.

use iced::{
    Background, Border,
    widget::{container, row},
};

use super::*;
use crate::{
    config::{Button, GuideInput, Keyword, Piece, RowShape, Stick},
    info::{PadFamily, Segment, button_glyph, glyph, key_glyph, mouse_glyph, trigger_glyph},
    keyword_icon,
};

/// A keyword in its colour: its icon, then its word.
fn keyword<'a>(k: Keyword) -> Element<'a, Message> {
    let [r, g, b] = keyword_icon::colour(k);
    let icon = svg(svg::Handle::from_memory(keyword_icon::svg(k).into_bytes())).width(14).height(14);
    row![icon, text(k.word()).color(Color::from_rgb8(r, g, b))].spacing(4).align_y(Alignment::Center).into()
}

/// The swap icon drawn after a face button whose glyph is swapped: two opposed arrows.
const SWAP_ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#8a8f98" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M4 8h15l-4-4M20 16H5l4 4"/></svg>"##;

/// How the Nintendo layout shows on a profile's face buttons: their labels, and whether the
/// buttons send the letters on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Swap {
    /// The labels are the controller's own.
    Off,
    /// The labels are swapped, and the buttons still send their positions, as on a controller
    /// that isn't a Nintendo pad.
    Labels,
    /// The labels are swapped, and the buttons send the letter on each label.
    Sends,
    /// The labels are swapped on a Nintendo pad whose face buttons are reassigned: the buttons send
    /// their positions, and the labels carry no marker saying otherwise.
    Quiet,
}

impl Swap {
    /// Whether the face buttons' labels are drawn swapped.
    pub(super) fn labels(self) -> bool {
        self != Self::Off
    }

    /// Whether a swap icon marks the swapped labels.
    fn marked(self) -> bool {
        matches!(self, Self::Labels | Self::Sends)
    }
}

/// A summary on one line. `muted` greys the words, for rows that aren't changed. `swap` draws
/// the Nintendo layout's face-button labels, each with a swap icon that explains them.
pub(super) fn piece_line<'a>(pieces: Vec<Piece>, family: PadFamily, swap: Swap, muted: bool) -> Element<'a, Message> {
    let items: Vec<Element<'a, Message>> = pieces
        .into_iter()
        .map(|piece| match piece {
            Piece::Text(s) => text(s).color_maybe(muted.then_some(MUTED_COLOR)).into(),
            Piece::Keyword(k) => keyword(k),
            Piece::Pad(b) if swap.marked() && is_face(b) => row![chip(pad_glyph(b, family, true)), swap_marker(swap)].spacing(2).align_y(Alignment::Center).into(),
            Piece::Pad(b) => chip(pad_glyph(b, family, swap.labels())),
            Piece::Key(code) => chip(key_glyph(&code)),
            Piece::Mouse(m) => chip(mouse_glyph(m)),
        })
        .collect();
    row(items).align_y(Alignment::Center).into()
}

/// An input's glyph, as the overlays draw it: a button as its controller glyph, a trigger as its
/// trigger glyph, and a stick as a round LS or RS.
pub(super) fn input_chip<'a>(input: GuideInput, family: PadFamily, swap: Swap) -> Element<'a, Message> {
    match input {
        GuideInput::Button(b) => piece_line(vec![Piece::Pad(b)], family, swap, false),
        GuideInput::Trigger(t) => chip(trigger_glyph(t, family)),
        GuideInput::Stick(s) => chip(stick_glyph(s)),
    }
}

/// An input's glyph as plain text, for places that can't draw one, such as a dropdown: the same
/// label the glyph is drawn with.
pub(super) fn input_text(input: GuideInput, family: PadFamily, swap: Swap) -> String {
    let segment = match input {
        GuideInput::Button(b) => pad_glyph(b, family, swap.labels()),
        GuideInput::Trigger(t) => trigger_glyph(t, family),
        GuideInput::Stick(s) => stick_glyph(s),
    };
    match segment {
        Segment::Glyph { label, .. } => label,
        _ => format!("{input:?}"),
    }
}

/// The glyphs of a Mapping guide row, joined and marked the way its shape says.
pub(super) fn row_glyphs<'a>(inputs: &[GuideInput], shape: RowShape, family: PadFamily, swap: Swap) -> Element<'a, Message> {
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    for (n, input) in inputs.iter().enumerate() {
        if n > 0 {
            items.push(text(shape.separator()).into());
        }
        items.push(input_chip(*input, family, swap));
    }
    if !shape.mark().is_empty() {
        items.push(text(shape.mark()).into());
    }
    row(items).spacing(2).align_y(Alignment::Center).into()
}

/// A stick as a round LS or RS, the way the overlay draws `{ls}` and `{rs}`.
fn stick_glyph(s: Stick) -> Segment {
    glyph(if s == Stick::Left { "LS" } else { "RS" }, None, true)
}

/// The four buttons whose labels the Nintendo layout swaps.
fn is_face(b: Button) -> bool {
    matches!(b, Button::South | Button::East | Button::West | Button::North)
}

/// A small swap icon whose tooltip says the face buttons' labels are swapped here, and whether the
/// buttons send the swapped letters.
fn swap_marker<'a>(swap: Swap) -> Element<'a, Message> {
    let icon = svg(svg::Handle::from_memory(SWAP_ICON.as_bytes())).width(14).height(14);
    let explain = match swap {
        Swap::Sends => "Nintendo layout: A and B, X and Y are swapped on these labels, and the buttons send the letter on each label.",
        _ => "Nintendo layout: A and B, X and Y are swapped on these labels. The buttons still do what their bindings say.",
    };
    tooltip(
        icon,
        container(text(explain).size(13))
            .padding(8)
            .max_width(320.0)
            .style(style::tooltip),
        tooltip::Position::Top,
    )
    .into()
}

/// A button's glyph, as the overlays draw it. A stick press is named here, since its cap is too
/// big for a line of text.
fn pad_glyph(b: Button, family: PadFamily, swapped: bool) -> Segment {
    match button_glyph(b, family, swapped) {
        Segment::StickClick { right } => glyph(if right { "R3" } else { "L3" }, None, false),
        other => other,
    }
}

/// The D-pad as the overlays draw it: a cross of cells, the pressed arms (`[up, down, left, right]`) lit.
fn dpad_chip<'a>([up, down, left, right]: [bool; 4]) -> Element<'a, Message> {
    let cell = |x: f32, y: f32, on: bool| {
        let fill = if on { "#4ea1ff" } else { "#3a3f48" };
        format!("<rect x='{x}' y='{y}' width='8' height='8' rx='1.5' fill='{fill}' stroke='#5d6470' stroke-width='1'/>")
    };
    let body = [cell(8.0, 0.0, up), cell(0.0, 8.0, left), cell(8.0, 8.0, false), cell(16.0, 8.0, right), cell(8.0, 16.0, down)].concat();
    let svg_text = format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'>{body}</svg>");
    svg(svg::Handle::from_memory(svg_text.into_bytes())).width(22).height(22).into()
}

/// A glyph drawn the way the overlays draw it: round for face buttons, in its colour when it has one.
fn chip<'a>(segment: Segment) -> Element<'a, Message> {
    let (label, fill, round) = match segment {
        Segment::Dpad(lit) => return dpad_chip(lit),
        Segment::Glyph { label, fill, round } => (label, fill, round),
        _ => return space().into(),
    };
    let fill = fill.map(|[r, g, b]| Color::from_rgb8(r, g, b));
    let label = text(label).size(12);
    let label = if fill.is_some() { label.color(Color::WHITE) } else { label };
    let width = if round { 22.0 } else { 0.0 };
    container(label)
        .padding([1, if round { 0 } else { 6 }])
        .width(if round { Length::Fixed(width) } else { Length::Shrink })
        .height(22)
        .center_x(if round { Length::Fixed(width) } else { Length::Shrink })
        .center_y(22)
        .style(move |_| container::Style {
            background: fill.map(Background::Color),
            border: Border {
                color: MUTED_COLOR,
                width: if fill.is_some() { 0.0 } else { 1.0 },
                radius: if round { (width / 2.0).into() } else { 5.0.into() },
            },
            ..container::Style::default()
        })
        .into()
}

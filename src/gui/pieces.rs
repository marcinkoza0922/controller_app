//! Actions drawn from their pieces: keywords in their own colour, and controller buttons as the
//! glyphs the overlays draw, for the controller in use.

use iced::{
    Background, Border,
    widget::{container, row},
};

use super::*;
use crate::{
    config::{Button, Keyword, Piece},
    info::{PadFamily, Segment, button_glyph, glyph},
    keyword_icon,
};

/// A keyword in its colour: its icon, then its word.
fn keyword<'a>(k: Keyword) -> Element<'a, Message> {
    let [r, g, b] = keyword_icon::colour(k);
    let icon = svg(svg::Handle::from_memory(keyword_icon::svg(k).into_bytes())).width(14).height(14);
    row![icon, text(k.word()).color(Color::from_rgb8(r, g, b))].spacing(4).align_y(Alignment::Center).into()
}

/// A summary on one line. `muted` greys the words, for rows that aren't changed.
pub(super) fn piece_line<'a>(pieces: Vec<Piece>, family: PadFamily, muted: bool) -> Element<'a, Message> {
    let items: Vec<Element<'a, Message>> = pieces
        .into_iter()
        .map(|piece| match piece {
            Piece::Text(s) => text(s).color_maybe(muted.then_some(MUTED_COLOR)).into(),
            Piece::Keyword(k) => keyword(k),
            Piece::Pad(b) => chip(pad_glyph(b, family)),
        })
        .collect();
    row(items).align_y(Alignment::Center).into()
}

/// A button's glyph. The overlay draws a cross for the D-pad and a pressed stick; here an arrow
/// and the stick's name stand in for them.
fn pad_glyph(b: Button, family: PadFamily) -> Segment {
    match button_glyph(b, family) {
        Segment::Dpad(_) => glyph(
            match b {
                Button::DpadUp => "↑",
                Button::DpadDown => "↓",
                Button::DpadLeft => "←",
                _ => "→",
            },
            None,
            false,
        ),
        Segment::StickClick { right } => glyph(if right { "R3" } else { "L3" }, None, false),
        other => other,
    }
}

/// A glyph drawn the way the overlays draw it: round for face buttons, in its colour when it has one.
fn chip<'a>(segment: Segment) -> Element<'a, Message> {
    let Segment::Glyph { label, fill, round } = segment else {
        return space().into();
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

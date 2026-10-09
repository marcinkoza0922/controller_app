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
};

/// The colour a keyword is drawn in: one per kind of item, readable on light and dark.
fn keyword_color(k: Keyword) -> Color {
    match k {
        Keyword::Toggle => Color::from_rgb8(0x2a, 0x9d, 0x8f),
        Keyword::Turbo => Color::from_rgb8(0xe7, 0x6f, 0x20),
        Keyword::Macro => Color::from_rgb8(0x8e, 0x5b, 0xd6),
        Keyword::Menu => Color::from_rgb8(0xd6, 0x45, 0x7a),
        Keyword::Layer => Color::from_rgb8(0xb0, 0x85, 0x10),
        Keyword::Info => Color::from_rgb8(0x3f, 0x7f, 0xd8),
        Keyword::Log => Color::from_rgb8(0x4f, 0x95, 0xa0),
    }
}

/// A summary on one line. `muted` greys the words, for rows that aren't changed.
pub(super) fn piece_line<'a>(pieces: Vec<Piece>, family: PadFamily, muted: bool) -> Element<'a, Message> {
    let items: Vec<Element<'a, Message>> = pieces
        .into_iter()
        .map(|piece| match piece {
            Piece::Text(s) => text(s).color_maybe(muted.then_some(MUTED_COLOR)).into(),
            Piece::Keyword(k) => text(k.word()).color(keyword_color(k)).into(),
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

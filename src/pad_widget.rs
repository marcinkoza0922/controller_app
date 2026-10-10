//! The controller drawing as an iced widget, shared by the settings window and the debug window.

use iced::{
    Color, Element,
    widget::{container, pin, stack, svg, text},
};

use crate::{
    info::{Glyphs, PadModel},
    ipc::InputSnapshot,
    pad_svg::{self, Spot},
};

/// The controller SVG with button letters and mapping-label pills placed over it. (iced's
/// SVG renderer may not draw SVG text, so text is real widgets pinned at drawing
/// coordinates; the drawing is shown at 1:1.) `opacity` dims the controller alone: the leader
/// lines and labels stay at full strength.
pub fn controller_drawing<'a, M: 'a>(
    input: Option<&InputSnapshot>,
    model: Option<PadModel>,
    glyphs: Glyphs,
    labels: &[(Spot, String)],
    opacity: f32,
) -> Element<'a, M> {
    let body = svg::Handle::from_memory(pad_svg::render(input, model, glyphs, &[]).into_bytes());
    let lines = svg::Handle::from_memory(pad_svg::leaders(labels, model).into_bytes());
    let mut layers: Vec<Element<'a, M>> = vec![
        svg(body).width(pad_svg::WIDTH).height(pad_svg::HEIGHT).opacity(opacity).into(),
        svg(lines).width(pad_svg::WIDTH).height(pad_svg::HEIGHT).into(),
    ];

    for o in pad_svg::overlays(input, model, glyphs) {
        let [r, g, b] = o.color;
        let letter = container(text(o.text).size(12).color(Color::from_rgb8(r, g, b)))
            .center_x(24)
            .center_y(20);
        layers.push(pin(letter).x(o.x - 12.0).y(o.y - 10.0).into());
    }
    for l in pad_svg::place_labels(labels, model) {
        let label = text(l.text).size(pad_svg::LABEL_TEXT_SIZE).wrapping(text::Wrapping::None);
        let pill = container(label.color(Color::from_rgb8(0xe6, 0xed, 0xf3)))
            .padding([2, 6])
            .style(|_: &iced::Theme| container::Style {
                background: Some(Color::from_rgb8(0x1d, 0x20, 0x26).into()),
                border: iced::Border { width: 1.0, radius: 5.0.into(), color: Color::from_rgb8(0x4e, 0xa1, 0xff) },
                ..container::Style::default()
            });
        // Each column is a box the width of the margin; pills hug its inner edge.
        let column = container(pill).width(pad_svg::LABEL_COLUMN);
        let (column, x) = if l.right {
            (column.align_x(iced::alignment::Horizontal::Left), pad_svg::RIGHT_COLUMN_X)
        } else {
            (column.align_x(iced::alignment::Horizontal::Right), 0.0)
        };
        layers.push(pin(column).x(x).y(l.y).into());
    }
    stack(layers).width(pad_svg::WIDTH).height(pad_svg::HEIGHT).into()
}

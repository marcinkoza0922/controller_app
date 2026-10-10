//! The radial menu: items as arcs of a circle sized by their weights, or as boxes at equal
//! angles around a hub, with the stick that aims it named in the middle.

use iced::{
    Border, Color, Element,
    widget::{container, pin, space, stack, text},
};

use super::*;
use crate::{config::MenuKind, menu::MenuView};

pub(super) fn radial<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
    match m.kind {
        MenuKind::Radial { boxes: true, .. } => radial_boxes(m, c, s),
        _ => radial_arcs(m, c, s),
    }
}

/// The menu as a circle divided into arcs, one per item, sized by their weights.
fn radial_arcs<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
    let weights: Vec<f32> = m.items.iter().map(|item| item.weight).collect();
    let arcs = crate::radial::arcs(&weights);
    let hub = 48.0 * s;
    let inner = hub / 2.0 + 6.0 * s;
    let outer = arc_outer_radius(&arcs, inner, s);
    let size = 2.0 * outer + 16.0 * s;
    let svg = arcs_svg(m, c, s, &arcs, (inner, outer));
    let center = size / 2.0;
    let point = |degrees: f32, r: f32| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        (center + sin * r, center - cos * r)
    };
    let (cell_w, cell_h) = (120.0 * s, 44.0 * s);
    let mut layers: Vec<Element<'a, M>> = vec![
        iced::widget::svg(iced::widget::svg::Handle::from_memory(svg.into_bytes()))
            .width(size)
            .height(size)
            .into(),
    ];
    for (i, &(start, end)) in arcs.iter().enumerate() {
        // Each label sits in the middle of its arc.
        let (x, y) = point((start + end) / 2.0, (inner + outer) / 2.0);
        let (label, _, _) = item_cell(m, i, c, 15.0 * s);
        let content = container(label).center_x(cell_w).center_y(cell_h);
        layers.push(pin(content).x(x - cell_w / 2.0).y(y - cell_h / 2.0).into());
    }
    layers.push(radial_hub(m, c, s, hub, size));
    stack(layers).width(size).height(size).into()
}

/// The SVG behind the arcs: a filled sector per item (the selected one in its own color),
/// or one ring when a single item takes the whole circle, which an arc can't draw.
fn arcs_svg(
    m: &MenuView,
    c: Colors,
    s: f32,
    arcs: &[(f32, f32)],
    (inner, outer): (f32, f32),
) -> String {
    let size = 2.0 * outer + 16.0 * s;
    let center = size / 2.0;
    let point = |degrees: f32, r: f32| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        (center + sin * r, center - cos * r)
    };
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 {size} {size}">"#
    );
    let fill_of = |i: usize| c.lit_item(m, i).item;
    if let [_] = arcs {
        let fill = fill_of(0);
        let r = (inner + outer) / 2.0;
        svg.push_str(&format!(
            r#"<circle cx="{center}" cy="{center}" r="{r}" fill="none" stroke="{}" stroke-opacity="{:.3}" stroke-width="{:.1}"/>"#,
            hex(fill),
            fill.a,
            outer - inner
        ));
    }
    for (i, &(start, end)) in arcs.iter().enumerate().filter(|_| arcs.len() > 1) {
        let fill = fill_of(i);
        let large = u8::from(end - start > 180.0);
        let at = |(x, y): (f32, f32)| format!("{x:.1} {y:.1}");
        let path = format!(
            "M{} A{outer:.1} {outer:.1} 0 {large} 1 {} L{} A{inner:.1} {inner:.1} 0 {large} 0 {} Z",
            at(point(start, outer)),
            at(point(end, outer)),
            at(point(end, inner)),
            at(point(start, inner)),
        );
        svg.push_str(&format!(
            r#"<path d="{path}" fill="{}" fill-opacity="{:.3}" stroke="{}" stroke-opacity="{:.3}" stroke-width="{:.1}"/>"#,
            hex(fill),
            fill.a,
            hex(c.background),
            c.background.a,
            2.0 * s
        ));
    }
    svg.push_str("</svg>");
    svg
}

/// The circle in the middle of a radial menu, naming the stick that aims it.
fn radial_hub<'a, M: 'a>(m: &MenuView, c: Colors, s: f32, hub: f32, size: f32) -> Element<'a, M> {
    // The middle names the stick that aims the menu.
    let stick = match m.kind {
        MenuKind::Radial {
            stick: crate::config::Stick::Left,
            ..
        } => "LS",
        _ => "RS",
    };
    let center = container(text(stick).font(c.font).size(16.0 * s).color(c.item_text))
        .center_x(hub)
        .center_y(hub)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(c.item.into()),
            border: Border {
                width: 1.0,
                radius: (hub / 2.0).into(),
                color: Color {
                    a: 0.25,
                    ..c.item_text
                },
            },
            ..container::Style::default()
        });
    pin(center)
        .x(size / 2.0 - hub / 2.0)
        .y(size / 2.0 - hub / 2.0)
        .into()
}

/// How far the arcs reach: far enough that the middle of each arc has room for its label,
/// up to a limit for very many items.
fn arc_outer_radius(arcs: &[(f32, f32)], inner: f32, s: f32) -> f32 {
    let label = 120.0 * s;
    let fits = |outer: f32| {
        let mid = (inner + outer) / 2.0;
        arcs.len() < 2
            || arcs
                .iter()
                .all(|&(a, b)| 2.0 * mid * ((b - a).to_radians() / 2.0).sin() >= label)
    };
    let mut outer = 170.0 * s;
    while !fits(outer) && outer < 420.0 * s {
        outer += 4.0 * s;
    }
    outer
}

/// `#rrggbb` for an SVG attribute.
fn hex(c: Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8
    )
}

/// The menu as boxes at equal angles around a hub.
fn radial_boxes<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
    let (cell_w, cell_h) = (120.0 * s, 44.0 * s);
    let n = m.items.len().max(1) as f32;
    let radius = radial_radius(m.items.len(), 120.0, 44.0) * s;
    let size = 2.0 * radius + 120.0 * s;
    let mut layers: Vec<Element<'a, M>> = vec![space().width(size).height(size).into()];
    for i in 0..m.items.len() {
        // First item at the top, then clockwise.
        let angle = i as f32 / n * TAU;
        let (x, y) = (
            size / 2.0 + radius * angle.sin(),
            size / 2.0 - radius * angle.cos(),
        );
        let (label, lit, selected) = item_cell(m, i, c, 15.0 * s);
        let content = container(label)
            .center_x(cell_w)
            .center_y(cell_h)
            .style(lit_cell_style(lit, selected, cell_h));
        layers.push(pin(content).x(x - cell_w / 2.0).y(y - cell_h / 2.0).into());
    }
    layers.push(radial_hub(m, c, s, 48.0 * s, size));
    stack(layers).width(size).height(size).into()
}

/// The smallest ring (at least 150) on which neighbouring `w`×`h` cells don't touch.
fn radial_radius(n: usize, w: f32, h: f32) -> f32 {
    let gap = 8.0;
    let step = TAU / n.max(1) as f32;
    let overlaps = |r: f32| {
        (0..n).any(|i| {
            let (a, b) = (i as f32 * step, (i + 1) as f32 * step);
            let dx = (r * a.sin() - r * b.sin()).abs();
            let dy = (r * a.cos() - r * b.cos()).abs();
            n > 1 && dx < w + gap && dy < h + gap
        })
    };
    let mut r = 150.0;
    while overlaps(r) && r < 600.0 {
        r += 5.0;
    }
    r
}

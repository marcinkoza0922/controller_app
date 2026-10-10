//! Drawing the overlays: each panel as an iced element, sized by its style's `Fit` (see
//! `fit.rs`) and laid out where its position puts it.

mod log_panel;
mod radial;

use std::f32::consts::TAU;

pub use log_panel::log_panel;

use radial::radial;

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Shadow, Vector,
    alignment::{Horizontal, Vertical},
    widget::{column, container, progress_bar, row, scrollable, space, text},
};
use iced::Font;

use super::{KeyboardView, fit::Fit};
use crate::{
    config::{MenuKind, OverlayStyle, Paint, ScreenPosition},
    info::{Charge, FormFactor, Icon, InfoView, PadFamily, Segment},
    keyboard, media,
    media::{MediaView, PlayState},
    menu::MenuView,
    motion::Anim,
    offer::OfferView,
};

const UNIT: f32 = 46.0;
const GAP: f32 = 4.0;
/// The keyboard's and numpad's padding inside their box, at scale 1.
const KEYBOARD_PADDING: f32 = 16.0;

/// The width of a keyboard's body at scale 1: the legend's or the keys', whichever is wider.
fn keyboard_body_width(numpad: bool) -> f32 {
    if numpad {
        // Numpad keys are fewer, so bigger; its body has a floor of 230 at any scale.
        (3.0 * (UNIT * 1.5 + GAP)).max(230.0)
    } else {
        15.0 * (UNIT + GAP)
    }
}
/// Distance from the screen edge for edge and corner positions.
const EDGE_MARGIN: f32 = 40.0;

/// What a menu is drawn with besides its own style: the controller in use (for its button
/// glyphs), the color-blind tints, and how the menu is moving.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuLook {
    pub colorblind: bool,
    pub family: crate::info::PadFamily,
    pub nintendo_layout: bool,
    pub anim: Anim,
}

fn blend(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: f32, y: f32| x + (y - x) * t;
    Color { r: mix(a.r, b.r), g: mix(a.g, b.g), b: mix(a.b, b.b), a: mix(a.a, b.a) }
}

#[derive(Clone, Copy)]
pub struct Colors {
    anim: Anim,
    font: Font,
    /// The controller in use, for button glyphs.
    family: crate::info::PadFamily,
    /// Face buttons drawn with the Nintendo layout's labels.
    nintendo_layout: bool,
    /// Tints for added and removed rows in colors color-blind people can tell apart.
    colorblind: bool,
    background: Color,
    background_text: Color,
    muted: Color,
    item: Color,
    item_text: Color,
    selected: Color,
    selected_text: Color,
    /// 0 square .. 1 round, from the style's corners setting.
    corners: f32,
}

/// Panels round to this radius at full corners; their size depends on their content.
const PANEL_CORNER: f32 = 40.0;

fn paint(p: &Paint, fallback: [u8; 3]) -> Color {
    let [r, g, b] = p.rgb().unwrap_or(fallback);
    Color::from_rgba8(r, g, b, p.opacity.clamp(0.0, 1.0))
}

/// White on dark colors, black on light ones.
fn text_on(c: Color) -> Color {
    let luminance = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    if luminance > 0.6 { Color::from_rgb(0.08, 0.08, 0.1) } else { Color::WHITE }
}

impl Colors {
    /// How lit item `i` is, 0 to 1: the cursor's item, and the one it came from as it slides away.
    pub fn lit(&self, m: &MenuView, i: usize) -> f32 {
        self.anim.lit(m.selected == Some(i), self.anim.timing.from == Some(i as u32))
    }

    /// These colors for item `i`: its highlight blended in, its pick flashed, and its stagger fade.
    pub fn lit_item(&self, m: &MenuView, i: usize) -> Colors {
        let lit = self.lit(m, i);
        let flash = if m.selected == Some(i) { self.anim.flash() } else { 0.0 };
        let fade = self.anim.item_fade(i);
        let faded = |c: Color| Color { a: c.a * fade, ..c };
        Colors {
            item: faded(blend(blend(self.item, self.selected, lit), Color::WHITE, flash * 0.5)),
            item_text: faded(blend(self.item_text, self.selected_text, lit)),
            ..*self
        }
    }

    /// Every color at `opacity` times its own, for fading out.
    fn faded(self, opacity: f32) -> Colors {
        let f = |c: Color| Color { a: c.a * opacity.clamp(0.0, 1.0), ..c };
        Colors {
            anim: self.anim,
            font: self.font,
            family: self.family,
            nintendo_layout: self.nintendo_layout,
            colorblind: self.colorblind,
            background: f(self.background),
            background_text: f(self.background_text),
            muted: f(self.muted),
            item: f(self.item),
            item_text: f(self.item_text),
            selected: f(self.selected),
            selected_text: f(self.selected_text),
            corners: self.corners,
        }
    }
}

fn colors(style: &OverlayStyle, font: Font) -> Colors {
    let background = paint(&style.background, [0x16, 0x18, 0x1c]);
    let item = paint(&style.items, [0x30, 0x34, 0x3c]);
    let selected = paint(&style.selected, [0x2f, 0x5d, 0xb0]);
    let background_text = text_on(background);
    Colors {
        anim: Anim::still(),
        font,
        family: crate::info::PadFamily::default(),
    nintendo_layout: false,
        colorblind: false,
        background,
        background_text,
        muted: Color { a: 0.75, ..background_text },
        item,
        item_text: text_on(item),
        selected,
        selected_text: text_on(selected),
        corners: style.corners,
    }
}

/// Positions an overlay panel on the full-screen surface, moved by its animation and by the
/// style's offset.
pub fn place<'a, M: 'a>(panel: Element<'a, M>, style: &OverlayStyle, fit: &Fit, anim: &Anim) -> Element<'a, M> {
    let (col, row) = style.position.cell();
    let horizontal = [Horizontal::Left, Horizontal::Center, Horizontal::Right][col];
    let vertical = [Vertical::Top, Vertical::Center, Vertical::Bottom][row];
    let margin = if style.position == ScreenPosition::Center { 0.0 } else { EDGE_MARGIN };
    let (x, y) = anim.offset();
    let (left, right) = sides(margin, x + fit.x, col);
    let (top, bottom) = sides(margin, y + fit.y, row);
    container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(horizontal)
        .align_y(vertical)
        .padding(Padding { top, right, bottom, left })
        .into()
}

/// The padding on the two sides of a panel, for a panel `shift` pixels off where it sits in
/// `slot` (0 start, 1 center, 2 end) of a `margin` from the edge.
fn sides(margin: f32, shift: f32, slot: usize) -> (f32, f32) {
    match slot {
        0 => ((margin + shift).max(0.0), margin),
        2 => (margin, (margin - shift).max(0.0)),
        // A centered panel sits halfway between its two paddings.
        _ => (margin + (2.0 * shift).max(0.0), margin + (-2.0 * shift).max(0.0)),
    }
}

/// A panel's box: its colors and padding, at the size `fit` gives. The contents scroll, so a
/// box held to its max length scrolls rather than spilling, and the box clips anything wider.
fn framed<'a, M: 'a>(body: Element<'a, M>, c: Colors, padding: impl Into<Padding>, fit: &Fit) -> Element<'a, M> {
    let height = if fit.height.is_some() { Length::Fill } else { Length::Shrink };
    let contents = scrollable(body).width(Length::Shrink).height(height);
    let mut boxed = container(contents)
        .padding(padding)
        .align_x(Horizontal::Center)
        .clip(true)
        .max_width(fit.max_width)
        .max_height(fit.max_height)
        .style(panel_style(c));
    if let Some(w) = fit.width {
        boxed = boxed.width(w);
    }
    if let Some(h) = fit.height {
        boxed = boxed.height(h);
    }
    boxed.into()
}

fn panel_style(c: Colors) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(c.background.into()),
        border: Border { width: 1.0, radius: (c.corners * PANEL_CORNER).into(), color: Color { a: 0.15, ..c.background_text } },
        shadow: Shadow { color: Color { a: 0.35 * c.background.a, ..Color::BLACK }, offset: Vector::new(0.0, 6.0), blur_radius: 18.0 },
        ..container::Style::default()
    }
}

/// The colors of a row of this kind: adding rows are tinted green and removing ones red, so
/// they stand out from the items they act on.
fn toned(c: Colors, tone: crate::menu::Tone) -> Colors {
    // Lime with white text stays readable at this mix (about 4.7:1 on the default items).
    let (tint, share) = match (tone, c.colorblind) {
        (crate::menu::Tone::Normal, _) => return c,
        (crate::menu::Tone::Add, false) => (Color::from_rgb8(0x5a, 0x9a, 0x1a), 0.75),
        (crate::menu::Tone::Remove, false) => (Color::from_rgb8(0xd0, 0x64, 0x64), 0.4),
        (crate::menu::Tone::Add, true) => (Color::from_rgb8(0x4a, 0x8f, 0xe0), 0.4),
        (crate::menu::Tone::Remove, true) => (Color::from_rgb8(0xe8, 0x96, 0x2e), 0.4),
    };
    let mix = |a: f32, b: f32| a + (b - a) * share;
    let item = Color { r: mix(c.item.r, tint.r), g: mix(c.item.g, tint.g), b: mix(c.item.b, tint.b), a: c.item.a };
    Colors { item, item_text: text_on(item), ..c }
}

/// An item's box, `height` tall: its corners round by half of that at full corners, so a
/// square cell becomes a circle.
/// A cell whose colors already include its highlight (see `Colors::lit_item`).
fn lit_cell_style(c: Colors, selected: bool, height: f32) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(c.item.into()),
        border: Border {
            width: if selected { 2.0 } else { 1.0 },
            radius: (c.corners * height / 2.0).into(),
            color: if selected { c.selected_text } else { Color { a: 0.12, ..c.item_text } },
        },
        ..container::Style::default()
    }
}

fn cell_style(c: Colors, selected: bool, height: f32) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(if selected { c.selected } else { c.item }.into()),
        border: Border {
            width: if selected { 2.0 } else { 1.0 },
            radius: (c.corners * height / 2.0).into(),
            color: if selected { c.selected_text } else { Color { a: 0.12, ..c.item_text } },
        },
        ..container::Style::default()
    }
}

/// An info overlay: its cells in a table, so the rows line up across the columns. A row is as
/// tall as its tallest cell, and a cell's segments wrap onto the next line when the panel is
/// held to its max width, so the whole row grows rather than the cell alone.
pub fn info_panel<'a, M: 'a>(v: &InfoView, font: Font, fit: &Fit, anim: &Anim) -> Element<'a, M> {
    let opacity = v.opacity * anim.opacity();
    let c = colors(&v.style, font).faded(opacity);
    let s = v.style.scale.clamp(0.5, 2.0);
    let columns = v.rows.iter().map(Vec::len).max().unwrap_or(0);
    let cols = (0..columns).map(|j| {
        iced::widget::table::column(space(), move |i: usize| -> Element<'a, M> {
            match v.rows[i].get(j) {
                Some(segments) => container(info_cell(segments, c, s, opacity)).align_y(Vertical::Center).into(),
                None => space().into(),
            }
        })
    });
    // Columns are as wide as their widest cell; the table's padding is half the gap between them.
    let grid = iced::widget::table(cols, 0..v.rows.len()).padding_x(10.0 * s).padding_y(2.0 * s).separator(0.0);
    let body: Element<'a, M> = match &v.title {
        Some(title) => column![info_cell(title, c, s, opacity), grid].spacing(8.0 * s).into(),
        None => grid.into(),
    };
    framed(body, c, [12.0 * s, 16.0 * s], fit)
}

/// A cell's segments in a row that wraps them onto the next line when they're too wide.
fn info_cell<'a, M: 'a>(segments: &[Segment], c: Colors, s: f32, opacity: f32) -> Element<'a, M> {
    let mut line = row![].spacing(2.0 * s).align_y(Alignment::Center);
    for seg in segments {
        line = line.push(segment_element(seg, c, s, opacity));
    }
    line.wrap().vertical_spacing(2.0 * s).into()
}

/// One piece of an info overlay's line, or of a menu row's glyphs.
fn segment_element<'a, M: 'a>(seg: &Segment, c: Colors, s: f32, opacity: f32) -> Element<'a, M> {
    match seg {
        Segment::Text(t) => Element::from(text(t.clone()).font(c.font).size(16.0 * s).color(c.background_text)),
        Segment::Glyph { label, fill, round } => glyph(label, *fill, *round, c, s, opacity),
        Segment::Dpad(lit) => dpad_glyph(*lit, c, s),
        Segment::StickClick { right } => stick_click_glyph(*right, c, s),
        Segment::Icon(icon) => icon_glyph(icon, c, s),
    }
}

/// A cross-shaped D-pad with the pressed arms (`[up, down, left, right]`) lit.
fn dpad_glyph<'a, M: 'a>([up, down, left, right]: [bool; 4], c: Colors, s: f32) -> Element<'a, M> {
    let cell = 8.0 * s;
    let arm = |on: bool| -> Element<'a, M> {
        let color = if on { c.selected } else { c.item };
        container(space())
            .width(cell)
            .height(cell)
            .style(move |_: &iced::Theme| container::Style {
                background: Some(color.into()),
                border: Border { width: 1.0, radius: (1.5 * s).into(), color: Color { a: 0.35, ..c.item_text } },
                ..container::Style::default()
            })
            .into()
    };
    let gap = || -> Element<'a, M> { space().width(cell).height(cell).into() };
    column![row![gap(), arm(up), gap()], row![arm(left), arm(false), arm(right)], row![gap(), arm(down), gap()]]
    .into()
}

/// A stick press: a round stick cap marked L or R, with a down arrow for the push.
fn stick_click_glyph<'a, M: 'a>(right: bool, c: Colors, s: f32) -> Element<'a, M> {
    let size = 26.0 * s;
    let label = if right { "R" } else { "L" };
    let cap = container(row![
        text(label).font(c.font).size(12.0 * s).color(c.item_text),
        text("↓").font(c.font).size(12.0 * s).color(c.item_text),
    ])
    .center_x(size)
    .center_y(size);
    cap.style(move |_: &iced::Theme| container::Style {
        background: Some(c.item.into()),
        border: Border { width: 2.0, radius: (size / 2.0).into(), color: Color { a: 0.6, ..c.item_text } },
        ..container::Style::default()
    })
    .into()
}

/// A button glyph: a colored disc for face buttons, a rounded tag for the rest.
#[expect(clippy::too_many_arguments, reason = "predates the size lints")]
fn glyph<'a, M: 'a>(label: &str, fill: Option<[u8; 3]>, round: bool, c: Colors, s: f32, opacity: f32) -> Element<'a, M> {
    let (bg, fg) = match fill {
        Some([r, g, b]) => (Color::from_rgba8(r, g, b, opacity), Color { a: opacity, ..Color::WHITE }),
        None => (c.item, c.item_text),
    };
    let size = 24.0 * s;
    let disc = round && label.chars().count() <= 2;
    let body = container(text(label.to_string()).font(c.font).size(13.0 * s).color(fg));
    let body = if disc {
        body.center_x(size).center_y(size)
    } else {
        body.padding([0.0, 7.0 * s]).height(size).center_y(size)
    };
    body.style(move |_: &iced::Theme| container::Style {
        background: Some(bg.into()),
        border: Border {
            width: 1.0,
            radius: if disc { size / 2.0 } else { 6.0 * s }.into(),
            color: Color { a: 0.2, ..fg },
        },
        ..container::Style::default()
    })
    .into()
}

#[expect(clippy::too_many_lines, reason = "predates the size lints")]
pub fn keyboard_panel<'a, M: 'a>(v: &KeyboardView, font: Font, fit: &Fit, anim: &Anim) -> Element<'a, M> {
    let c = colors(&v.style, font).faded(anim.opacity());
    let numpad = v.layout == crate::keyboard::Layout::Numpad;
    // A set width fits the keys (and their text) to it; otherwise the style's scale applies.
    let s = match fit.width {
        Some(w) => w / (keyboard_body_width(numpad) + 2.0 * KEYBOARD_PADDING),
        None => v.style.scale.clamp(0.5, 2.0),
    };
    // Numpad keys are fewer, so bigger.
    let unit = if numpad { UNIT * 1.5 } else { UNIT } * s;
    let gap = GAP * s;
    let cursor_code = keyboard::key_at(v.layout, v.cursor).code;
    let label_size = if numpad { 22.0 } else { 14.0 };
    let mut rows = column![].spacing(gap);
    // Keys are numbered in reading order, as the motion tracking numbers them.
    let mut slot = 0u32;
    for keys in v.layout.rows().iter() {
        let mut line = row![].spacing(gap);
        for key in keys.iter() {
            let here = slot;
            slot += 1;
            let width = key.width * (unit + gap) - gap;
            if key.code.is_empty() {
                line = line.push(space().width(width).height(unit));
                continue;
            }
            let cursor = key.code == cursor_code;
            let pressed = v.pressed.as_deref() == Some(key.code);
            let selected = cursor || pressed;
            let latched = v.latched.iter().any(|l| l == key.code);
            // The cursor's key lights up and the one it left goes dark, sliding between them.
            let lit = if pressed { 1.0 } else { anim.lit(cursor, anim.timing.from == Some(here)) };
            let (bg, fg, border) = if latched && !selected {
                let green = Color::from_rgb8(0x2e, 0x7d, 0x46);
                (green, Color::WHITE, Color::from_rgb8(0x3f, 0xb9, 0x50))
            } else {
                let flash = if pressed { anim.flash() * 0.5 } else { 0.0 };
                (
                    blend(blend(c.item, c.selected, lit), Color::WHITE, flash),
                    blend(c.item_text, c.selected_text, lit),
                    blend(Color { a: 0.12, ..c.item_text }, c.selected_text, lit),
                )
            };
            let round = c.corners * unit / 2.0;
            let cap = container(text(key.label).font(c.font).size(if selected { label_size + 2.0 } else { label_size } * s).color(fg))
                .center_x(width)
                .center_y(unit)
                .style(move |_| container::Style {
                    background: Some(bg.into()),
                    border: Border { width: if selected { 2.0 } else { 1.0 }, radius: round.into(), color: border },
                    ..container::Style::default()
                });
            line = line.push(cap);
        }
        rows = rows.push(line);
    }
    let hint = |t: &'static str| text(t).font(c.font).size(14.0 * s).color(c.background_text);
    let (legend, width): (Element<'a, M>, f32) = if numpad {
        let legend = column![
            row![hint("A  press"), hint("X  backspace")].spacing(14.0 * s).wrap().align_x(Horizontal::Center),
            row![hint("Start  enter"), hint("hold B to close")].spacing(14.0 * s).wrap().align_x(Horizontal::Center),
        ]
        .spacing(4.0 * s)
        .align_x(Alignment::Center);
        (legend.into(), (3.0 * (unit + gap)).max(230.0 * s))
    } else {
        let legend = row![
            hint("A  press"),
            hint("X  backspace"),
            hint("Y  space"),
            hint("Start  enter"),
            hint("hold LT  shift"),
            hint("Shift, Ctrl and Alt stay on for one key"),
            hint("hold B to close"),
        ]
        .spacing(18.0 * s)
        .align_y(Alignment::Center)
        .wrap()
        .vertical_spacing(4.0 * s)
        .align_x(Horizontal::Center);
        (legend.into(), 15.0 * (unit + gap))
    };
    let mut body = column![rows, legend].spacing(12.0 * s).width(width).align_x(Alignment::Center);
    if v.closing > 0.0 {
        body = body.push(progress_bar(0.0..=1.0, v.closing).girth(4.0 * s));
    }
    framed(body.into(), c, KEYBOARD_PADDING * s, fit)
}

/// An item's glyphs (the same as the info overlays'), its label, and a ▸ for submenus. A
/// text badge (a stick's LS, say) is drawn as a small tag.
fn item_text<'a, M: 'a>(c: Colors, item: &crate::menu::ItemView, label: &str, size: f32, fg: Color) -> Element<'a, M> {
    let font = c.font;
    let mut line = row![].spacing(size * 0.5).align_y(Alignment::Center);
    // The glyphs are sized from the label, which is 17 units when the scale is 1.
    for b in &item.buttons {
        line = line.push(segment_element(&crate::info::button_glyph(*b, c.family, c.nintendo_layout), c, size / 17.0, 1.0));
    }
    if let Some(b) = item.button.as_deref().filter(|b| !b.is_empty()) {
        line = line.push(
            container(text(b.to_string()).font(font).size(size - 2.0).color(Color::BLACK))
                .padding([1.0, size * 0.4])
                .style(|_| container::Style {
                    background: Some(Color::from_rgb(0.85, 0.87, 0.9).into()),
                    border: Border { radius: 5.0.into(), ..Border::default() },
                    ..container::Style::default()
                }),
        );
    }
    if let Some(k) = item.keyword {
        let icon = iced::widget::svg::Handle::from_memory(crate::keyword_icon::svg(k).into_bytes());
        line = line.push(iced::widget::svg(icon).width(size).height(size));
    }
    line = line.push(text(label.to_string()).font(font).size(size).color(fg));
    if item.submenu {
        line = line.push(text("▸").font(font).size(size).color(Color { a: 0.7, ..fg }));
    }
    line.into()
}

/// An icon in an info cell, drawn as SVG so it doesn't depend on the font having the glyph.
fn icon_glyph<'a, M: 'a>(icon: &Icon, c: Colors, s: f32) -> Element<'a, M> {
    let size = 22.0 * s;
    let svg_text = icon_svg(icon, c.background_text);
    iced::widget::svg(iced::widget::svg::Handle::from_memory(svg_text.into_bytes())).width(size).height(size).into()
}

/// The SVG for an info icon, in `ink`. A controller is drawn in its kind's color, where it
/// has one; a battery's fill is green, amber or red by its level.
pub fn icon_svg(icon: &Icon, ink: Color) -> String {
    let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    let [r, g, b, _] = ink.into_rgba8();
    let c = hex([r, g, b]);
    let body = match icon {
        Icon::Form(FormFactor::Desktop) => format!(
            "<rect x='2' y='3' width='20' height='13' rx='2' fill='none' stroke='{c}' stroke-width='2'/>\
             <path d='M8 21 H16 M12 16 V21' fill='none' stroke='{c}' stroke-width='2' stroke-linecap='round'/>"
        ),
        Icon::Form(FormFactor::Laptop) => format!(
            "<rect x='5' y='4' width='14' height='11' rx='1.5' fill='none' stroke='{c}' stroke-width='2'/>\
             <path d='M2 18 H22 L21 20.5 H3 Z' fill='{c}'/>"
        ),
        Icon::Form(FormFactor::Handheld) => format!(
            "<rect x='2' y='6' width='20' height='12' rx='4' fill='none' stroke='{c}' stroke-width='2'/>\
             <rect x='8.5' y='9' width='7' height='6' rx='1' fill='{c}'/>"
        ),
        Icon::Controller(family) => {
            let tint = match family {
                PadFamily::Xbox => hex([0x3c, 0xa0, 0x3c]),
                PadFamily::PlayStation => hex([0x5a, 0x8c, 0xdc]),
                PadFamily::Nintendo => c.clone(),
            };
            format!(
                "<path d='M7 7 H17 C20.5 7 22.5 9.5 22.5 13.5 C22.5 17.5 20.8 18.5 19.2 18.5 \
                 C17.8 18.5 16.9 17.2 15.9 15.8 H8.1 C7.1 17.2 6.2 18.5 4.8 18.5 \
                 C3.2 18.5 1.5 17.5 1.5 13.5 C1.5 9.5 3.5 7 7 7 Z' fill='{tint}'/>"
            )
        }
        Icon::Battery(charge) => battery_svg(*charge, &c),
        Icon::Wifi(percent) => wifi_svg(*percent, &c),
    };
    format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' opacity='{}'>{body}</svg>", ink.a)
}

/// A battery outline filled to its level; an estimate is faded.
fn battery_svg(charge: Charge, c: &str) -> String {
    let percent = charge.percent.min(100);
    let width = 14.0 * f32::from(percent) / 100.0;
    let level = match percent {
        50.. => [0x3c, 0xa0, 0x3c],
        20..50 => [0xc8, 0xa0, 0x1e],
        _ => [0xc8, 0x3c, 0x3c],
    };
    let level = format!("#{:02x}{:02x}{:02x}", level[0], level[1], level[2]);
    let opacity = if charge.estimated { 0.6 } else { 1.0 };
    format!(
        "<rect x='1.5' y='6.5' width='18' height='11' rx='2.5' fill='none' stroke='{c}' stroke-width='2'/>\
         <rect x='21' y='10' width='2' height='4' rx='1' fill='{c}'/>\
         <rect x='4' y='9' width='{width:.2}' height='6' rx='1' fill='{level}' fill-opacity='{opacity}'/>"
    )
}

/// Wi-Fi bars, lit as the signal passes each quarter.
fn wifi_svg(percent: u8, c: &str) -> String {
    let lit: u8 = match percent {
        0 => 0,
        1..=25 => 1,
        26..=50 => 2,
        51..=75 => 3,
        _ => 4,
    };
    (0u8..4)
        .map(|i| {
            let height = 5.0 + 4.0 * f32::from(i);
            let opacity = if i < lit { 1.0 } else { 0.25 };
            format!(
                "<rect x='{:.1}' y='{:.1}' width='3.5' height='{height:.1}' rx='1' fill='{c}' fill-opacity='{opacity}'/>",
                2.0 + 5.5 * f32::from(i),
                21.0 - height
            )
        })
        .collect()
}

/// A play, pause or stop symbol, drawn so it doesn't depend on the font having the glyph.
fn state_icon<'a, M: 'a>(state: PlayState, color: Color, size: f32) -> Element<'a, M> {
    let [r, g, b, _] = color.into_rgba8();
    let shape = match state {
        PlayState::Playing => "<path d='M6 3 L21 12 L6 21 Z'/>",
        PlayState::Paused => "<rect x='5' y='3' width='5' height='18' rx='1'/><rect x='14' y='3' width='5' height='18' rx='1'/>",
        PlayState::Stopped => "<rect x='5' y='5' width='14' height='14' rx='2'/>",
    };
    let svg_text = format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='#{r:02x}{g:02x}{b:02x}' fill-opacity='{}'>{shape}</svg>", color.a);
    iced::widget::svg(iced::widget::svg::Handle::from_memory(svg_text.into_bytes())).width(size).height(size).into()
}

/// The media controls: what is playing, how far along, and the volume.
pub fn media_panel<'a, M: 'a>(m: &MediaView, font: Font, fit: &Fit, anim: &Anim) -> Element<'a, M> {
    let c = colors(&m.style, font).faded(anim.opacity());
    let s = m.style.scale.clamp(0.5, 2.0);
    let width = 560.0 * s;
    let body: Element<'a, M> = match &m.player {
        None => text("No media player is running").font(c.font).size(18.0 * s).color(c.background_text).into(),
        Some(player) => {
            let title = if m.title.is_empty() { "Nothing playing" } else { &m.title };
            let by = [m.artist.as_str(), player.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" · ");
            let progress = if m.length_ms > 0 { (m.position_ms as f32 / m.length_ms as f32).clamp(0.0, 1.0) } else { 0.0 };
            let times = format!("{} / {}", media::clock(m.position_ms), if m.length_ms > 0 { media::clock(m.length_ms) } else { "--:--".into() });
            let mut meta = row![text(times).font(c.font).size(14.0 * s).color(c.muted)].spacing(16.0 * s);
            if let Some(v) = m.volume {
                meta = meta.push(text(format!("Volume {:.0}%", v * 100.0)).font(c.font).size(14.0 * s).color(c.muted));
            }
            if m.players > 1 {
                meta = meta.push(text(format!("{} players", m.players)).font(c.font).size(14.0 * s).color(c.muted));
            }
            column![
                row![
                    state_icon(m.state, c.background_text, 28.0 * s),
                    column![
                        text(title.to_string()).font(c.font).size(20.0 * s).color(c.background_text),
                        text(by).font(c.font).size(14.0 * s).color(c.muted),
                    ]
                    .spacing(2.0 * s),
                ]
                .spacing(14.0 * s)
                .align_y(Alignment::Center),
                progress_bar(0.0..=1.0, progress).girth(6.0 * s),
                meta.wrap().vertical_spacing(4.0 * s),
            ]
            .spacing(10.0 * s)
            .into()
        }
    };
    let contents = column![body, text(m.hint.clone()).font(c.font).size(12.0 * s).color(c.muted)]
        .spacing(12.0 * s)
        .width(width)
        .align_x(Alignment::Center);
    framed(contents.into(), c, 18.0 * s, fit)
}

/// The offer to add a library game: the question and its answers, or the packs to pick from.
pub fn offer_panel<'a, M: 'a>(o: &OfferView, font: Font, fit: &Fit, anim: &Anim) -> Element<'a, M> {
    let c = colors(&o.style, font).faded(anim.opacity());
    let s = o.style.scale.clamp(0.5, 2.0);
    let cell = |label: String, lit: Colors, selected: bool| {
        container(text(label).font(c.font).size(16.0 * s).color(lit.item_text))
            .padding([8.0 * s, 16.0 * s])
            .style(lit_cell_style(lit, selected, 36.0 * s))
    };
    let body: Element<'a, M> = if o.choices.is_empty() {
        let answers = o.answers.iter().map(|(button, what)| cell(format!("{button}  {what}"), c, false).into());
        row(answers).spacing(10.0 * s).into()
    } else {
        let items = o.choices.iter().enumerate().map(|(i, name)| {
            let selected = i == o.selected;
            let lit = anim.lit(selected, anim.timing.from == Some(i as u32));
            let tinted = Colors { item: blend(c.item, c.selected, lit), item_text: blend(c.item_text, c.selected_text, lit), ..c };
            cell(name.clone(), tinted, selected).width(Length::Fill).into()
        });
        column(items).spacing(6.0 * s).width(360.0 * s).into()
    };
    let contents = column![
        text(o.title.clone()).font(c.font).size(20.0 * s).color(c.background_text),
        text(o.detail.clone()).font(c.font).size(14.0 * s).color(c.muted).width(520.0 * s).align_x(Alignment::Center),
        body,
        text(o.hint.clone()).font(c.font).size(12.0 * s).color(c.muted),
    ]
    .spacing(14.0 * s)
    .align_x(Alignment::Center);
    framed(contents.into(), c, 22.0 * s, fit)
}

pub fn menu_panel<'a, M: 'a>(m: &MenuView, font: Font, look: MenuLook, fit: &Fit) -> Element<'a, M> {
    let c = Colors {
        anim: look.anim,
        family: look.family,
        nintendo_layout: look.nintendo_layout,
        colorblind: look.colorblind,
        ..colors(&m.style, font)
    };
    let c = c.faded(look.anim.opacity());
    let s = m.style.scale.clamp(0.5, 2.0);
    let body: Element<'a, M> = match m.kind {
        MenuKind::Radial { .. } => radial(m, c, s),
        MenuKind::Directional { .. } => directional(m, c, s),
        MenuKind::List | MenuKind::Buttons => list(m, c, s),
        MenuKind::Carousel { .. } => carousel(m, c, s),
        MenuKind::Grid { .. } => grid(m, c, s),
    };
    // Where this page is in the menus, above its title: "Menu › Edit Controls › A button".
    let title = text(m.title.clone()).font(c.font).size(20.0 * s).color(c.background_text);
    let heading: Element<'a, M> = if m.crumbs.is_empty() {
        title.into()
    } else {
        let crumbs = text(m.crumbs.join(" › ")).font(c.font).size(13.0 * s).color(c.muted);
        column![crumbs, title].spacing(4.0 * s).align_x(Alignment::Center).into()
    };
    let contents = column![
        heading,
        body,
        text(m.hint.clone()).font(c.font).size(13.0 * s).color(c.muted),
    ]
    .spacing(14.0 * s)
    .align_x(Alignment::Center);
    framed(contents.into(), c, 22.0 * s, fit)
}

/// An item's label, its colors (highlighted, and faded in under Stagger) and whether the
/// cursor is on it.
fn item_cell<'a, M: 'a>(m: &MenuView, i: usize, c: Colors, size: f32) -> (Element<'a, M>, Colors, bool) {
    let item = &m.items[i];
    let selected = m.selected == Some(i);
    let lit = c.lit_item(m, i);
    (item_text(lit, item, &item.label, size, lit.item_text), lit, selected)
}

fn directional<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
    let (w, h) = (190.0 * s, 52.0 * s);
    let slot = |i: usize| -> Element<'a, M> {
        match m.items.get(i).filter(|item| !item.label.is_empty()) {
            Some(item) => container(item_text(c, item, &item.label, 16.0 * s, c.item_text))
                .center_x(w)
                .center_y(h)
                .style(cell_style(c, false, h))
                .into(),
            None => space().width(w).height(h).into(),
        }
    };
    column![
        slot(0),
        row![slot(3), space().width(40.0 * s), slot(1)].align_y(Alignment::Center),
        slot(2),
    ]
    .spacing(10.0 * s)
    .align_x(Alignment::Center)
    .into()
}

/// Items in rows of equal cells, read left to right; a short last row keeps to the left.
fn grid<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
    let columns = m.kind.grid_columns().unwrap_or(1);
    let (w, h) = (150.0 * s, 64.0 * s);
    let mut rows = column![].spacing(8.0 * s);
    for start in (0..m.items.len()).step_by(columns) {
        let mut line = row![].spacing(8.0 * s);
        for i in start..(start + columns).min(m.items.len()) {
            let (label, lit, selected) = item_cell(m, i, c, 16.0 * s);
            line = line.push(container(label).center_x(w).center_y(h).padding([0.0, 6.0 * s]).style(lit_cell_style(lit, selected, h)));
        }
        rows = rows.push(line);
    }
    rows.into()
}

/// A list: one column while it is short, two once it is longer. Two columns fill top to
/// bottom, the left one first, and scroll together with the cursor.
fn list<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
    let len = m.items.len();
    let cursor = m.selected.unwrap_or(0);
    let cell = |i: usize| -> Element<'a, M> {
        let item = &m.items[i];
        let selected = m.selected == Some(i);
        let lit = toned(c, item.tone).lit_item(m, i);
        let label = match (c.colorblind, item.tone) {
            (true, crate::menu::Tone::Add) => format!("+ {}", item.label),
            (true, crate::menu::Tone::Remove) => format!("− {}", item.label),
            _ => item.label.clone(),
        };
        let content = item_text(lit, item, &label, 17.0 * s, lit.item_text);
        container(content).padding([10.0 * s, 14.0 * s]).width(Length::Fill).style(lit_cell_style(lit, selected, 40.0 * s)).into()
    };
    let marker = |t: &'static str| text(t).font(c.font).size(13.0 * s).color(c.muted);
    let start = crate::menu::list_start(cursor, len);
    let half = if crate::menu::list_columns(len) == 2 { len.div_ceil(2) } else { len };
    let shown = |from: usize, to: usize| (from..to.min(from + crate::menu::VISIBLE_ROWS)).map(&cell).collect::<Vec<Element<'a, M>>>();
    let column_of = |items: Vec<Element<'a, M>>, width: f32| items.into_iter().fold(column![].spacing(6.0 * s).width(width * s), iced::widget::Column::push);
    let left = column_of(shown(start, half), 300.0);
    let grid: Element<'a, M> = if half < len {
        row![left, column_of(shown(half + start, len), 300.0)].spacing(14.0 * s).into()
    } else {
        left.into()
    };
    let mut body = column![].spacing(6.0 * s);
    if start > 0 {
        body = body.push(marker("▲"));
    }
    body = body.push(grid);
    if start + crate::menu::VISIBLE_ROWS < half {
        body = body.push(marker("▼"));
    }
    body.into()
}

fn carousel<'a, M: 'a>(m: &MenuView, c: Colors, s: f32) -> Element<'a, M> {
    let n = m.items.len();
    let selected = m.selected.unwrap_or(0);
    let arrow = |t: &'static str| text(t).font(c.font).size(22.0 * s).color(c.muted);
    let mut line = row![arrow("◀")].spacing(12.0 * s).align_y(Alignment::Center);
    // The selected item in the middle, with up to two neighbors on each side.
    let shown = n.min(5) as i32;
    for offset in -(shown / 2)..=(shown - 1 - shown / 2) {
        let i = (selected as i32 + offset).rem_euclid(n as i32) as usize;
        let big = offset == 0;
        let item = &m.items[i];
        let lit = c.lit_item(m, i);
        let height = if big { 80.0 } else { 60.0 } * s;
        line = line.push(
            container(item_text(lit, item, &item.label, if big { 19.0 } else { 14.0 } * s, lit.item_text))
                .center_x(if big { 170.0 } else { 120.0 } * s)
                .center_y(height)
                .style(lit_cell_style(lit, big, height)),
        );
    }
    line.push(arrow("▶")).into()
}

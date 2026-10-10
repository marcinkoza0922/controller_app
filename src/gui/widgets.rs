//! Small widgets in the app's style, shared by every page: dropdowns, fields, labeled rows, sections, ⓘ tips, the overlay style editor and previews.

use std::ops::RangeInclusive;

use iced::widget::{column, row};

use super::*;

/// A "▸ Label" / "▾ Label" toggle for an optional section.
pub(super) fn disclosure<'a>(label: &str, open: bool, message: Message) -> Element<'a, Message> {
    let chevron = if open { "▾" } else { "▸" };
    button(text(format!("{chevron} {label}")).size(14)).style(button::text).padding([4, 0]).on_press(message).into()
}

pub(super) type OnStyle<'a> = Rc<dyn Fn(OverlayStyle) -> Message + 'a>;

/// Colors offered as swatches; any other color can be typed as #rrggbb.
pub(super) const SWATCHES: [&str; 12] = [
    "#000000", "#16181c", "#30343c", "#5a606b", "#e6e8eb", "#ffffff", "#2f5db0", "#1f7a7a", "#2e7d46", "#6a3fb0",
    "#a83232", "#c26a1d",
];

/// Position, size and colors of an overlay.
#[expect(clippy::needless_pass_by_value, reason = "editors take their callbacks by value")]
#[expect(clippy::too_many_lines, reason = "predates the size lints")]
pub(super) fn style_editor<'a>(style: &OverlayStyle, on_change: OnStyle<'a>) -> Element<'a, Message> {
    let with = |f: &dyn Fn(&mut OverlayStyle)| {
        let mut s = style.clone();
        f(&mut s);
        on_change(s)
    };
    // A small "screen" of 3×3 spots to click.
    let mut grid = column![].spacing(3);
    for r in 0..3 {
        let mut line = row![].spacing(3);
        for c in 0..3 {
            let pos = ScreenPosition::GRID[r * 3 + c];
            let chosen = pos == style.position;
            line = line.push(
                tooltip(
                    button(space().width(22).height(12))
                        .padding(2)
                        .style(if chosen { button::primary } else { style::secondary })
                        .on_press(with(&|s| s.position = pos)),
                    container(text(pos.to_string()).size(13)).padding(6).style(style::tooltip),
                    tooltip::Position::Top,
                ),
            );
        }
        grid = grid.push(line);
    }
    let position = row![grid, text(style.position.to_string()).size(13).color(MUTED_COLOR)]
        .spacing(12)
        .align_y(Alignment::Center);

    let scale = style.scale;
    let size = row![
        slider(50.0..=200.0, scale * 100.0, {
            let on_change = on_change.clone();
            let style = style.clone();
            move |v| on_change(OverlayStyle { scale: (v / 100.0 * 20.0).round() / 20.0, ..style.clone() })
        })
        .step(5.0_f32)
        .width(220),
        text(format!("{:.0}%", scale * 100.0)).size(13),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let paint_editor = |label: &'static str, paint: &Paint, set: fn(&mut OverlayStyle, Paint)| -> Element<'a, Message> {
        let mut swatches = row![].spacing(4).align_y(Alignment::Center);
        for hex in SWATCHES {
            let [r, g, b] = crate::config::parse_hex(hex).unwrap_or_default();
            let color = Color::from_rgb8(r, g, b);
            let chosen = paint.color.eq_ignore_ascii_case(hex);
            let pick = Paint { color: hex.into(), ..paint.clone() };
            swatches = swatches.push(
                button(space().width(16).height(16))
                    .padding(0)
                    .style(move |_, _| button::Style {
                        background: Some(color.into()),
                        border: iced::Border {
                            width: if chosen { 3.0 } else { 1.0 },
                            radius: 4.0.into(),
                            color: if chosen { Color::from_rgb8(0x4e, 0xa1, 0xff) } else { Color::from_rgb(0.5, 0.5, 0.5) },
                        },
                        ..button::Style::default()
                    })
                    .on_press(with(&|s| set(s, pick.clone()))),
            );
        }
        let typed = {
            let on_change = on_change.clone();
            let style = style.clone();
            let opacity = paint.opacity;
            move |hex: String| {
                let mut s = style.clone();
                set(&mut s, Paint { color: hex, opacity });
                on_change(s)
            }
        };
        let valid = paint.rgb().is_some();
        let opacity = {
            let on_change = on_change.clone();
            let style = style.clone();
            let color = paint.color.clone();
            move |v: f32| {
                let mut s = style.clone();
                set(&mut s, Paint { color: color.clone(), opacity: (v / 100.0).clamp(0.0, 1.0) });
                on_change(s)
            }
        };
        let mut line = row![
            swatches,
            field("#rrggbb", &paint.color).on_input(typed).width(90),
            slider(0.0..=100.0, paint.opacity * 100.0, opacity).step(5.0_f32).width(110),
            text(format!("{:.0}%", paint.opacity * 100.0)).size(12),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if !valid {
            line = line.push(text("not a #rrggbb color").size(12).color(ERROR_COLOR));
        }
        labeled(format!("    {label}"), line.into())
    };

    let corners = style.corners;
    let shape = row![
        slider(0.0..=100.0, corners * 100.0, {
            let on_change = on_change.clone();
            let style = style.clone();
            move |v| on_change(OverlayStyle { corners: (v / 100.0 * 20.0).round() / 20.0, ..style.clone() })
        })
        .step(5.0_f32)
        .width(220),
        text(corner_name(corners)).size(13),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let width = optional_slider(
        "Set width",
        style.width,
        Span { default: 50.0, range: 10.0..=100.0, unit: "%" },
        set_field(on_change.clone(), style, |s: &mut OverlayStyle, v: Option<f32>| s.width = v),
    );
    let height = optional_slider(
        "Set height",
        style.height,
        Span { default: 50.0, range: 10.0..=100.0, unit: "%" },
        set_field(on_change.clone(), style, |s: &mut OverlayStyle, v: Option<f32>| s.height = v),
    );
    let offset_x = percent_slider(style.x_offset, -50.0..=50.0, set_field(on_change.clone(), style, |s: &mut OverlayStyle, v: f32| s.x_offset = v));
    let offset_y = percent_slider(style.y_offset, -50.0..=50.0, set_field(on_change.clone(), style, |s: &mut OverlayStyle, v: f32| s.y_offset = v));
    let max_width = percent_slider(style.max_width, 10.0..=100.0, set_field(on_change.clone(), style, |s: &mut OverlayStyle, v: f32| s.max_width = v));
    let max_height = percent_slider(style.max_height, 10.0..=100.0, set_field(on_change.clone(), style, |s: &mut OverlayStyle, v: f32| s.max_height = v));

    column![
        labeled("    Position", position.into()),
        labeled("    Size", size.into()),
        labeled("    Width", width),
        labeled("    Height", height),
        labeled("    Offset X", offset_x),
        labeled("    Offset Y", offset_y),
        labeled("    Max width", max_width),
        labeled("    Max height", max_height),
        labeled("    Corners", shape.into()),
        paint_editor("Background", &style.background, |s, p| s.background = p),
        paint_editor("Items", &style.items, |s, p| s.items = p),
        paint_editor("Selected item", &style.selected, |s, p| s.selected = p),
    ]
    .spacing(8)
    .into()
}

/// A slider for a number of percent of the display.
fn percent_slider<'a>(value: f32, range: RangeInclusive<f32>, on_change: impl Fn(f32) -> Message + 'a) -> Element<'a, Message> {
    row![slider(range, value, on_change).step(1.0_f32).width(220), text(format!("{value:.0}%")).size(13)]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}

/// What a corners setting looks like, from square to circle.
fn corner_name(corners: f32) -> &'static str {
    match corners {
        c if c < 0.1 => "Square",
        c if c < 0.5 => "Rounded",
        c if c < 0.95 => "Squircle",
        _ => "Circle",
    }
}

/// The preview keeps the real colors but caps the size so it fits the window.
pub(super) fn preview_style(style: &OverlayStyle) -> OverlayStyle {
    OverlayStyle { scale: style.scale.min(1.0), ..style.clone() }
}

/// A live preview on a dark "screen", so transparency shows.
pub(super) fn preview<'a>(panel: Element<'a, Message>) -> Element<'a, Message> {
    column![
        text("Preview").size(12).color(MUTED_COLOR),
        container(container(panel).center_x(Length::Fill))
            .padding(16)
            .width(Length::Fill)
            .style(|_: &iced::Theme| container::Style {
                background: Some(Color::from_rgb8(0x3a, 0x4a, 0x5c).into()),
                border: iced::Border { radius: 8.0.into(), ..iced::Border::default() },
                ..container::Style::default()
            }),
    ]
    .spacing(4)
    .into()
}

/// A checkbox for an optional number of seconds, with a slider while it's ticked.
pub(super) fn seconds_option<'a>(label: &'static str, value: Option<f32>, default: f32, on_change: impl Fn(Option<f32>) -> Message + Clone + 'a) -> Element<'a, Message> {
    optional_slider(label, value, Span { default, range: 1.0..=60.0, unit: " s" }, on_change)
}

/// How an optional slider runs: where it starts when ticked, its range, and the unit it shows.
pub(super) struct Span {
    pub default: f32,
    pub range: RangeInclusive<f32>,
    pub unit: &'static str,
}

/// A checkbox for an optional number, with a slider while it's ticked. Unticked, the number is
/// unset: `value` is `None`.
pub(super) fn optional_slider<'a>(label: &'static str, value: Option<f32>, span: Span, on_change: impl Fn(Option<f32>) -> Message + Clone + 'a) -> Element<'a, Message> {
    let toggle = on_change.clone();
    let default = span.default;
    let mut line = row![checkbox(value.is_some()).label(label).on_toggle(move |on| toggle(on.then_some(default)))]
        .spacing(10)
        .align_y(Alignment::Center);
    if let Some(number) = value {
        line = line
            .push(slider(span.range, number, move |v| on_change(Some(v))).step(1.0_f32).width(180))
            .push(text(format!("{number:.0}{}", span.unit)).size(13));
    }
    line.into()
}

/// A message for `on_change` with one field of `style` set to the value it's given.
pub(super) fn set_field<'a, T: 'a>(on_change: OnStyle<'a>, style: &OverlayStyle, set: fn(&mut OverlayStyle, T)) -> impl Fn(T) -> Message + Clone + 'a {
    let style = style.clone();
    move |v| {
        let mut s = style.clone();
        set(&mut s, v);
        on_change(s)
    }
}

/// A dropdown in the app's style: stands out from cards, with a raised menu.
pub(super) fn dropdown<'a, T, L, V>(options: L, selected: Option<V>, on_select: impl Fn(T) -> Message + 'a) -> iced::widget::PickList<'a, T, L, V, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: Borrow<[T]> + 'a,
    V: Borrow<T> + 'a,
{
    pick_list(options, selected, on_select)
        .style(style::dropdown)
        .menu_style(style::dropdown_menu)
        .padding([6, 10])
}

/// An exact milliseconds entry beside a slider (sliders can't hit 17 ms on a 5 s range).
pub(super) fn ms_field<'a>(ms: u64, on_change: impl Fn(u64) -> Message + 'a) -> Element<'a, Message> {
    row![
        field("ms", &ms.to_string())
            .on_input(move |s| {
                let digits: String = s.chars().filter(char::is_ascii_digit).take(6).collect();
                on_change(digits.parse().unwrap_or(0))
            })
            .width(80),
        text("ms").size(13),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// A text input in the app's style.
pub(super) fn field<'a>(placeholder: &str, value: &str) -> iced::widget::TextInput<'a, Message> {
    text_input(placeholder, value).style(style::text_field).padding([6, 10])
}

pub(super) fn section<'a>(title: &'a str, help_text: Option<String>, rows: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut heading = row![text(title).size(18)].spacing(8).align_y(Alignment::Center);
    if let Some(h) = help_text {
        heading = heading.push(help(h));
    }
    container(column![heading].extend(rows).spacing(10))
        .padding(14)
        .width(Length::Fill)
        .style(style::card)
        .into()
}

/// The card at the top of an items tab (layers, macros, menus, info and log overlays): the add
/// buttons, a line on what the items are, the copy link and the tab's help. Every tab has this
/// layout, so the controls sit in the same places.
pub(super) fn items_header<'a>(kind: ItemKind, add: Element<'a, Message>, blurb: &'static str, explain: String) -> Element<'a, Message> {
    container(
        row![
            add,
            text(blurb).size(13).color(MUTED_COLOR),
            space::horizontal(),
            button(text("Copy from another setup…").size(13)).style(button::text).on_press(Message::OpenBrowse(kind)),
            help(explain),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(14)
    .width(Length::Fill)
    .style(style::card)
    .into()
}

/// The title of an item's card, with the chevron that opens it at the same size as a disclosure's.
pub(super) fn card_title<'a>(open: bool, title: iced::widget::Text<'a>) -> iced::widget::Button<'a, Message> {
    let chevron = text(if open { "▾" } else { "▸" }).size(14);
    button(row![chevron, title].spacing(8).align_y(Alignment::Center)).style(button::text).padding(0)
}

/// The top of a Details card that a setup can override: a toggle for "Use its own `what` in this
/// setup", and while it's off, a note that the card follows App settings. Every such card starts
/// with this, and shows its editor only while the toggle is on.
pub(super) fn own_rows<'a>(what: &str, own: bool, on_toggle: impl Fn(bool) -> Message + 'a) -> Vec<Element<'a, Message>> {
    let mut rows = vec![toggler(own).label(format!("Use its own {what} in this setup")).on_toggle(on_toggle).into()];
    if !own {
        rows.push(text(format!("Following the {what} set on the App settings page.")).size(13).color(MUTED_COLOR).into());
    }
    rows
}

/// Deletes a whole item, from its card's header.
pub(super) fn delete_item<'a>(label: &'static str, message: Message) -> Element<'a, Message> {
    button(text(label).size(13)).style(style::quiet_danger).on_press(message).into()
}

/// An ⓘ that explains a section on hover, instead of a paragraph of gray text.
pub(super) fn help<'a>(explanation: String) -> Element<'a, Message> {
    tooltip(
        text("ⓘ").size(16).color(style_accent()),
        container(text(explanation).size(13)).padding(10).max_width(420.0).style(style::tooltip),
        tooltip::Position::Bottom,
    )
    .gap(6)
    .into()
}

/// The accent color, for hints that should catch the eye.
pub(super) fn style_accent() -> Color {
    Color::from_rgb(0.35, 0.45, 0.95)
}

pub(super) fn labeled<'a>(label: impl text::IntoFragment<'a>, editor: Element<'a, Message>) -> Element<'a, Message> {
    row![text(label).width(LABEL_WIDTH), editor]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}

/// Gives an action editor the row's free width, so it wraps onto a second line there instead of
/// running off screen, and buttons placed after it (such as a remove button) line up at the right.
pub(super) fn fill_x<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content).width(Length::Fill).into()
}

/// A slider's value and unit: "250 ms", but "45°" and "120°/s turning", since a degree sign
/// goes against its number.
fn slider_value(value: f32, step: f32, unit: &str) -> String {
    let number = if step >= 1.0 { format!("{value:.0}") } else { format!("{value:.2}") };
    if unit.is_empty() || unit.starts_with('°') { format!("{number}{unit}") } else { format!("{number} {unit}") }
}

#[expect(clippy::too_many_arguments, reason = "predates the size lints")]
pub(super) fn value_slider<'a>(
    label: &'a str,
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    step: f32,
    unit: &'a str,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    labeled(
        label,
        row![slider(range, value, on_change).step(step).width(300), text(slider_value(value, step, unit)).size(13)]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::slider_value;

    #[test]
    fn units_are_spaced_but_degrees_are_not() {
        assert_eq!(slider_value(250.0, 10.0, "ms"), "250 ms");
        assert_eq!(slider_value(0.0, 15.0, "° from up"), "0° from up");
        assert_eq!(slider_value(120.0, 10.0, "°/s turning"), "120°/s turning");
        assert_eq!(slider_value(0.5, 0.1, "°/s"), "0.50°/s");
        assert_eq!(slider_value(8.0, 1.0, ""), "8");
    }
}

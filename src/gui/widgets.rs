//! Small widgets in the app's style, shared by every page: dropdowns, fields, labeled rows, sections, ⓘ tips, the overlay style editor and previews.

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
                        .style(if chosen { button::primary } else { button::secondary })
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

    column![
        labeled("    Position", position.into()),
        labeled("    Size", size.into()),
        paint_editor("Background", &style.background, |s, p| s.background = p),
        paint_editor("Items", &style.items, |s, p| s.items = p),
        paint_editor("Selected item", &style.selected, |s, p| s.selected = p),
    ]
    .spacing(8)
    .into()
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
    let toggle = on_change.clone();
    let mut line = row![checkbox(value.is_some()).label(label).on_toggle(move |on| toggle(on.then_some(default)))]
        .spacing(10)
        .align_y(Alignment::Center);
    if let Some(seconds) = value {
        line = line
            .push(slider(1.0..=60.0, seconds, move |v| on_change(Some(v))).step(1.0_f32).width(180))
            .push(text(format!("{seconds:.0} s")).size(13));
    }
    line.into()
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

/// An ⓘ that explains a section on hover, instead of a paragraph of grey text.
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

pub(super) fn value_slider<'a>(
    label: &'a str,
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    step: f32,
    unit: &'a str,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    let shown = if step >= 1.0 { format!("{value:.0} {unit}") } else { format!("{value:.2} {unit}") };
    labeled(
        label,
        row![slider(range, value, on_change).step(step).width(300), text(shown).size(13)]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
    )
}

//! Shared widget styles for the GUI. iced's defaults draw dropdowns, their menus and the
//! section cards in the same color, so fields disappear inside cards; these keep them apart
//! in both light and dark themes.

use iced::{
    Background, Border, Color, Shadow, Theme, Vector,
    widget::{button, container, overlay::menu, pick_list, text_input},
};

/// Section cards: a step above the page, with a subtle edge.
pub fn card(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.weakest.color.into()),
        text_color: Some(p.background.weakest.text),
        border: Border { width: 1.0, radius: 8.0.into(), color: p.background.weak.color },
        ..container::Style::default()
    }
}

/// Grouped rows inside a card (a combo, a zone, a macro step).
pub fn inset(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.base.color.into()),
        text_color: Some(p.background.base.text),
        border: Border { width: 1.0, radius: 6.0.into(), color: p.background.weak.color },
        ..container::Style::default()
    }
}

/// The outputs of a "Several at once" action: a faint accent tint, so the list reads as one group
/// apart from the surrounding card. Translucent, so it works over either theme's surfaces.
pub fn output_list(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(Color { a: 0.07, ..p.primary.base.color }.into()),
        border: Border { width: 1.0, radius: 6.0.into(), color: Color { a: 0.3, ..p.primary.base.color } },
        ..container::Style::default()
    }
}

/// Tooltip bubbles.
pub fn tooltip(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.strong.color.into()),
        text_color: Some(p.background.strong.text),
        border: Border { width: 1.0, radius: 6.0.into(), color: p.background.stronger.color },
        shadow: popup_shadow(),
        ..container::Style::default()
    }
}

/// Sidebar entries: plain text, with the open page (and the hovered one) on a tinted
/// background, so they read as navigation rather than as buttons.
pub fn nav(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let p = theme.extended_palette();
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let background = if selected {
            Some(p.primary.weak.color.into())
        } else if hovered {
            Some(p.background.weak.color.into())
        } else {
            None
        };
        let text_color = if selected { p.primary.weak.text } else { p.background.base.text };
        button::Style {
            background,
            text_color,
            border: Border { radius: 6.0.into(), ..Border::default() },
            ..button::Style::default()
        }
    }
}

/// The track behind a segmented control (the section switches inside a page).
pub fn segments(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.weak.color.into()),
        border: Border { width: 1.0, radius: 8.0.into(), color: p.background.strong.color },
        ..container::Style::default()
    }
}

/// One segment: the chosen one is a raised chip on the track.
pub fn segment(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let p = theme.extended_palette();
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let background = if selected {
            Some(p.background.base.color.into())
        } else if hovered {
            Some(p.background.strong.color.into())
        } else {
            None
        };
        button::Style {
            background,
            text_color: if selected { p.primary.base.color } else { p.background.weak.text },
            border: Border { radius: 6.0.into(), ..Border::default() },
            shadow: if selected {
                Shadow { color: Color { a: 0.25, ..Color::BLACK }, offset: Vector::new(0.0, 1.0), blur_radius: 3.0 }
            } else {
                Shadow::default()
            },
            ..button::Style::default()
        }
    }
}

/// Ordinary buttons. iced's own secondary is a flat mid-grey that looks the same as a
/// disabled button; this one is a light field-like surface with an edge, so it reads as
/// clickable, and fades out when disabled.
pub fn secondary(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    let (background, border_color) = match status {
        button::Status::Active => (p.background.weak.color, p.background.strong.color),
        button::Status::Hovered => (p.background.strong.color, p.primary.base.color),
        button::Status::Pressed => (p.background.stronger.color, p.primary.base.color),
        button::Status::Disabled => (p.background.weakest.color, p.background.weak.color),
    };
    let text_color = if status == button::Status::Disabled { muted(p.background.weak.text) } else { p.background.weak.text };
    button::Style {
        background: Some(background.into()),
        text_color,
        border: Border { width: 1.0, radius: 5.0.into(), color: border_color },
        ..button::Style::default()
    }
}

/// A quiet "✕" for removing a card: muted until hovered, then red.
pub fn quiet_danger(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    let text_color = match status {
        button::Status::Hovered | button::Status::Pressed => p.danger.base.color,
        _ => muted(p.background.base.text),
    };
    button::Style { text_color, ..button::Style::default() }
}

/// Field surface shared by dropdowns and text inputs: lighter than both page and cards.
fn field(theme: &Theme, emphasized: bool) -> (Background, Border, Color) {
    let p = theme.extended_palette();
    let border_color = if emphasized { p.primary.base.color } else { p.background.strong.color };
    (
        p.background.weak.color.into(),
        Border { width: if emphasized { 2.0 } else { 1.0 }, radius: 5.0.into(), color: border_color },
        p.background.weak.text,
    )
}

pub fn dropdown(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let p = theme.extended_palette();
    let emphasized = matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. });
    let (background, border, text_color) = field(theme, emphasized);
    pick_list::Style {
        text_color,
        placeholder_color: muted(p.background.weak.text),
        // The arrow in the accent color marks every dropdown as one at a glance.
        handle_color: p.primary.base.color,
        background,
        border,
    }
}

/// Open dropdown menus: a raised surface with an accent edge and a shadow.
pub fn dropdown_menu(theme: &Theme) -> menu::Style {
    let p = theme.extended_palette();
    menu::Style {
        background: p.background.neutral.color.into(),
        border: Border { width: 1.0, radius: 6.0.into(), color: p.primary.base.color },
        text_color: p.background.neutral.text,
        selected_text_color: p.primary.strong.text,
        selected_background: p.primary.strong.color.into(),
        shadow: popup_shadow(),
    }
}

pub fn text_field(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let p = theme.extended_palette();
    let emphasized = matches!(status, text_input::Status::Hovered | text_input::Status::Focused { .. });
    let (background, border, value) = field(theme, emphasized);
    text_input::Style {
        background,
        border,
        icon: muted(p.background.weak.text),
        placeholder: muted(p.background.weak.text),
        value,
        selection: p.primary.weak.color,
    }
}

/// Readable but clearly secondary text on a field.
fn muted(text: Color) -> Color {
    Color { a: 0.6, ..text }
}

fn popup_shadow() -> Shadow {
    Shadow { color: Color { a: 0.45, ..Color::BLACK }, offset: Vector::new(0.0, 4.0), blur_radius: 14.0 }
}

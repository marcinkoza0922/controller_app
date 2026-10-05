//! Shared widget styles for the GUI. iced's defaults draw dropdowns, their menus and the
//! section cards in the same color, so fields disappear inside cards; these keep them apart
//! in both light and dark themes.

use iced::{
    Background, Border, Color, Shadow, Theme, Vector,
    widget::{container, overlay::menu, pick_list, text_input},
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

/// A row picked out by "Find by pressing" or flagged by validation.
pub fn highlight(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let p = theme.extended_palette();
        container::Style {
            background: Some(p.background.weakest.color.into()),
            border: Border { width: 2.0, radius: 6.0.into(), color },
            ..container::Style::default()
        }
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

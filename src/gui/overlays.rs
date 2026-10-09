//! A game's own look for the on-screen keyboard and numpad, shown on its Details tab.

use std::fmt;

use iced::widget::{checkbox, column};

use super::*;
use crate::{keyboard::Layout, overlay::KeyboardView};

/// An entry in a font dropdown: nothing chosen (whatever applies instead), or a family.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum FontChoice {
    Unset(&'static str),
    Named(String),
}

impl fmt::Display for FontChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontChoice::Unset(label) => f.write_str(label),
            FontChoice::Named(name) => f.write_str(name),
        }
    }
}

/// The dropdown's entries: the unset one, then the bundled fonts, then the current one if
/// it is something else (set by a pack, say).
fn font_choices(unset: &'static str, current: Option<&str>) -> Vec<FontChoice> {
    let mut names = crate::font::names();
    if let Some(c) = current.filter(|c| !names.iter().any(|n| n == c)) {
        names.push(c.to_string());
    }
    std::iter::once(FontChoice::Unset(unset)).chain(names.into_iter().map(FontChoice::Named)).collect()
}

fn font_picker<'a>(
    unset: &'static str,
    current: Option<&'a str>,
    on_pick: fn(Option<String>) -> Message,
) -> Element<'a, Message> {
    let selected = current.map_or(FontChoice::Unset(unset), |c| FontChoice::Named(c.to_string()));
    dropdown(font_choices(unset, current), Some(selected), move |choice| {
        on_pick(match choice {
            FontChoice::Unset(_) => None,
            FontChoice::Named(name) => Some(name),
        })
    })
    .width(200)
    .into()
}

const FONT_HELP: &str = "One font for every overlay, menu and the on-screen keyboard and numpad. Pick one of the \
                         free fonts that come with the app. (A pack can also name any font installed on your system.)";

impl App {
    /// The font overlay previews are drawn in: the shown game's, else the global one.
    pub(super) fn preview_font(&self) -> iced::Font {
        let own = match &self.page {
            Page::Game(_) => self.game().overlay_font.as_deref(),
            _ => None,
        };
        crate::font::resolve(own.or(self.config.overlay_font.as_deref()))
    }

    /// The App settings page's font card: used by every game that doesn't pick its own.
    pub(super) fn view_font_card(&self) -> Element<'_, Message> {
        section(
            "Overlay font",
            Some(FONT_HELP.into()),
            vec![labeled("Font", font_picker("System default", self.config.overlay_font.as_deref(), Message::SetOverlayFont))],
        )
    }

    /// How menu previews are drawn: the colour-blind setting, and the controller the info
    /// glyphs are drawn for when none is detected.
    pub(super) fn menu_look(&self) -> crate::overlay::draw::MenuLook {
        crate::overlay::draw::MenuLook {
            colourblind: self.config.colourblind_tones,
            family: self.config.info_glyphs,
            nintendo_layout: self.nintendo_layout(),
        }
    }

    /// The App settings page's colour card: the tints of added and removed rows.
    pub(super) fn view_colour_card(&self) -> Element<'_, Message> {
        section(
            "Menu colours",
            Some("Rows that add to a list are tinted, and rows that remove from one are tinted red. Colour-blind mode uses blue and orange instead, and puts a + or − in front of each row.".into()),
            vec![iced::widget::toggler(self.config.colourblind_tones).label("Colour-blind mode").on_toggle(Message::SetColourblindTones).into()],
        )
    }

    fn view_game_font(&self) -> Element<'_, Message> {
        section(
            "Overlay font",
            Some(FONT_HELP.into()),
            vec![labeled(
                "Font",
                font_picker("Same as Settings", self.game().overlay_font.as_deref(), Message::SetGameOverlayFont),
            )],
        )
    }

    /// Sets (or with `None` clears) the shown game's own style for a layout.
    pub(super) fn set_game_overlay_style(&mut self, layout: Layout, style: Option<OverlayStyle>) {
        let game = self.game_mut();
        match layout {
            Layout::Keyboard => game.keyboard_style = style,
            Layout::Numpad => game.numpad_style = style,
        }
    }

    /// The keyboard, numpad, media controls and in-game menu cards: each follows the Settings
    /// page until given its own look.
    pub(super) fn view_game_overlays(&self) -> Element<'_, Message> {
        let game = self.game();
        let font = self.preview_font();
        let look = self.menu_look();
        column![
            self.view_game_font(),
            game_overlay_card(
                "On-screen keyboard",
                game.keyboard_style.as_ref(),
                &self.config.keyboard_style,
                |s| Message::SetGameOverlayStyle(Layout::Keyboard, s),
                move |style| keyboard_preview(Layout::Keyboard, style, font),
            ),
            game_overlay_card(
                "On-screen numpad",
                game.numpad_style.as_ref(),
                &self.config.numpad_style,
                |s| Message::SetGameOverlayStyle(Layout::Numpad, s),
                move |style| keyboard_preview(Layout::Numpad, style, font),
            ),
            game_overlay_card(
                "Media controls",
                game.media_style.as_ref(),
                &self.config.media_style,
                Message::SetGameMediaStyle,
                move |style| preview(crate::overlay::draw::media_panel(&crate::media::MediaView::sample(preview_style(style)), font)),
            ),
            game_overlay_card(
                "In-game menu",
                game.menu_style.as_ref(),
                &self.config.menu_style,
                Message::SetGameMenuStyle,
                move |style| {
                    let sample = crate::system_menu::main_page(preview_style(style), 0);
                    preview(crate::overlay::draw::menu_panel(&sample, font, look))
                },
            ),
        ]
        .spacing(16)
        .into()
    }
}

/// One overlay's card: whether the game has its own look (`own`), that look's editor and a
/// preview, or a note that it follows `global`. `set` makes the message for a new look.
fn game_overlay_card<'a>(
    title: &'a str,
    own: Option<&'a OverlayStyle>,
    global: &'a OverlayStyle,
    set: impl Fn(Option<OverlayStyle>) -> Message + 'a,
    sample: impl Fn(&OverlayStyle) -> Element<'static, Message>,
) -> Element<'a, Message> {
    let set = Rc::new(set);
    let on_style = {
        let set = set.clone();
        Rc::new(move |s| set(Some(s))) as OnStyle<'a>
    };
    let toggle = checkbox(own.is_some())
        .label("Use its own appearance in this setup")
        .on_toggle(move |on| set(on.then(|| global.clone())));
    let mut rows: Vec<Element<'a, Message>> = vec![toggle.into()];
    if let Some(style) = own {
        rows.push(style_editor(style, on_style));
        rows.push(sample(style));
    } else {
        rows.push(text("Following the appearance set on the App settings page.").size(13).color(MUTED_COLOR).into());
    }
    section(title, None, rows)
}

/// A keyboard or numpad drawn with `style`, with the cursor on a key, for the card's preview.
fn keyboard_preview(layout: Layout, style: &OverlayStyle, font: iced::Font) -> Element<'static, Message> {
    let sample = KeyboardView {
        layout,
        style: preview_style(style),
        cursor: match layout {
            Layout::Keyboard => crate::keyboard::find(layout, "KEY_H").unwrap_or_default(),
            Layout::Numpad => layout.home(),
        },
        latched: Vec::new(),
        pressed: None,
        closing: 0.0,
    };
    preview(crate::overlay::draw::keyboard_panel(&sample, font))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::ScreenPosition, gui::tests::*};

    #[test]
    fn a_game_can_override_the_keyboard_and_numpad_styles() {
        let mut app = with_game();
        let style = OverlayStyle { position: ScreenPosition::TopLeft, ..OverlayStyle::default() };
        let _ = app.update(Message::SetGameOverlayStyle(Layout::Keyboard, Some(style.clone())));
        let doom = app.config.games.iter().find(|g| g.name == "Doom").unwrap();
        assert_eq!(doom.keyboard_style, Some(style));
        assert_eq!(doom.numpad_style, None);
        let _ = app.update(Message::SetGameOverlayStyle(Layout::Keyboard, None));
        assert!(app.config.games.iter().find(|g| g.name == "Doom").unwrap().keyboard_style.is_none());
    }

    #[test]
    fn a_game_can_override_the_media_and_in_game_menu_looks() {
        let mut app = with_game();
        let style = OverlayStyle { position: ScreenPosition::BottomLeft, ..OverlayStyle::default() };
        let _ = app.update(Message::SetGameMediaStyle(Some(style.clone())));
        let _ = app.update(Message::SetGameMenuStyle(Some(style.clone())));
        let doom = app.config.games.iter().find(|g| g.name == "Doom").unwrap();
        assert_eq!((doom.media_style.clone(), doom.menu_style.clone()), (Some(style.clone()), Some(style.clone())));
        assert_eq!(doom.keyboard_style, None);
        let _ = app.update(Message::SetGameMediaStyle(None));
        assert!(app.config.games.iter().find(|g| g.name == "Doom").unwrap().media_style.is_none());
    }

    #[test]
    fn the_font_is_one_setting_per_game_over_a_global_one() {
        let mut app = with_game();
        let _ = app.update(Message::SetOverlayFont(Some("Inter".into())));
        assert_eq!(app.config.active_font(), Some("Inter"));
        let _ = app.update(Message::SetGameOverlayFont(Some("Comfortaa".into())));
        let doom = app.config.games.iter().find(|g| g.name == "Doom").unwrap();
        assert_eq!(doom.overlay_font.as_deref(), Some("Comfortaa"));
        assert_eq!(app.config.overlay_font.as_deref(), Some("Inter"));
        assert_eq!(font_choices("x", Some("Fira")).len(), crate::font::names().len() + 2);
    }
}

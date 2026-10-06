//! A game's own look for the on-screen keyboard and numpad, shown on its Details tab.

use iced::widget::{checkbox, column};

use super::*;
use crate::{keyboard::Layout, overlay::KeyboardView};

impl App {
    /// Sets (or with `None` clears) the shown game's own style for a layout.
    pub(super) fn set_game_overlay_style(&mut self, layout: Layout, style: Option<OverlayStyle>) {
        let game = self.game_mut();
        match layout {
            Layout::Keyboard => game.keyboard_style = style,
            Layout::Numpad => game.numpad_style = style,
        }
    }

    /// The keyboard and numpad cards: each follows the Settings page until given its own look.
    pub(super) fn view_game_overlays(&self) -> Element<'_, Message> {
        let game = self.game();
        column![
            self.view_game_overlay(Layout::Keyboard, "On-screen keyboard", game.keyboard_style.as_ref(), &self.config.keyboard_style),
            self.view_game_overlay(Layout::Numpad, "On-screen numpad", game.numpad_style.as_ref(), &self.config.numpad_style),
        ]
        .spacing(16)
        .into()
    }

    fn view_game_overlay<'a>(
        &self,
        layout: Layout,
        title: &'a str,
        own: Option<&'a OverlayStyle>,
        global: &'a OverlayStyle,
    ) -> Element<'a, Message> {
        let toggle = checkbox(own.is_some())
            .label("Use its own appearance in this game")
            .on_toggle(move |on| Message::SetGameOverlayStyle(layout, on.then(|| global.clone())));
        let mut rows: Vec<Element<'a, Message>> = vec![toggle.into()];
        if let Some(style) = own {
            rows.push(style_editor(style, Rc::new(move |s| Message::SetGameOverlayStyle(layout, Some(s)))));
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
            rows.push(preview(crate::overlay::draw::keyboard_panel(&sample)));
        } else {
            rows.push(text("Following the appearance set on the Settings page.").size(13).color(MUTED_COLOR).into());
        }
        section(title, None, rows)
    }
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
}

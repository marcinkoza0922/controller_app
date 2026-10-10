//! Short descriptions of buttons and actions, for the settings window and the input log.

use serde::{Deserialize, Serialize};

use super::{Button, ButtonAction, MouseButton};

impl Button {
    /// A short name on Xbox terms, for one-line summaries.
    pub fn short_name(self) -> &'static str {
        match self {
            Button::South => "A",
            Button::East => "B",
            Button::North => "Y",
            Button::West => "X",
            Button::LeftBumper => "LB",
            Button::RightBumper => "RB",
            Button::Select => "Select",
            Button::Start => "Start",
            Button::Guide => "Guide",
            Button::LeftStick => "LS",
            Button::RightStick => "RS",
            Button::DpadUp => "Up",
            Button::DpadDown => "Down",
            Button::DpadLeft => "Left",
            Button::DpadRight => "Right",
            Button::LeftStickUp => "LS↑",
            Button::LeftStickDown => "LS↓",
            Button::LeftStickLeft => "LS←",
            Button::LeftStickRight => "LS→",
            Button::RightStickUp => "RS↑",
            Button::RightStickDown => "RS↓",
            Button::RightStickLeft => "RS←",
            Button::RightStickRight => "RS→",
            Button::LeftPaddle => "LP",
            Button::RightPaddle => "RP",
            Button::LeftPaddle2 => "LP2",
            Button::RightPaddle2 => "RP2",
        }
    }
}

/// A word that says what an action does with its item, shown in its own color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Keyword {
    Toggle,
    Turbo,
    Macro,
    Menu,
    Info,
    Log,
    Layer,
}

impl Keyword {
    pub fn word(self) -> &'static str {
        match self {
            Keyword::Toggle => "Toggle",
            Keyword::Turbo => "Turbo",
            Keyword::Macro => "Macro",
            Keyword::Menu => "Menu",
            Keyword::Info => "Info",
            Keyword::Log => "Log",
            Keyword::Layer => "Layer",
        }
    }
}

/// A summary in pieces, so the settings window can color keywords and draw controller buttons
/// as their glyphs. [`plain_text`] gives the plain text.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(String),
    Keyword(Keyword),
    /// A controller button, drawn as its glyph for the controller in use.
    Pad(Button),
    /// A keyboard key, by its evdev code, drawn as a keycap.
    Key(String),
    /// A mouse button, drawn as its glyph.
    Mouse(MouseButton),
}

/// The pieces as plain text: a controller button or trigger by its name.
pub fn plain_text(pieces: &[Piece]) -> String {
    pieces
        .iter()
        .map(|piece| match piece {
            Piece::Text(text) => text.clone(),
            Piece::Keyword(k) => k.word().to_string(),
            Piece::Pad(b) => b.short_name().to_string(),
            Piece::Key(code) => key_name(code),
            Piece::Mouse(m) => format!("{m} click"),
        })
        .collect()
}

/// The text a Mappings row shows for an output by default. A pad button, a key and a mouse
/// button are glyph tokens (`{pad:south}`, `{keyboard:a}`, `{mouse:leftclick}`), drawn as the
/// controller the daemon presents itself as or as the key or button they are. The rest is plain.
pub fn guide_text(pieces: &[Piece]) -> String {
    pieces
        .iter()
        .map(|piece| match piece {
            Piece::Pad(b) => match crate::info::button_token(*b) {
                Some(token) => format!("{{pad:{token}}}"),
                None => b.short_name().to_string(),
            },
            Piece::Key(code) => match crate::keyboard::token_name(code) {
                Some(name) => format!("{{keyboard:{name}}}"),
                None => key_name(code),
            },
            Piece::Mouse(m) => format!("{{mouse:{}}}", crate::info::mouse_token(*m)),
            other => plain_text(std::slice::from_ref(other)),
        })
        .collect()
}

impl ButtonAction {
    /// One-line description, for collapsed rows, the controller drawing and the input log.
    pub fn summary(&self) -> String {
        plain_text(&self.pieces())
    }

    /// The first keyword in the description, which is the one to draw an icon for.
    pub fn keyword(&self) -> Option<Keyword> {
        self.pieces().into_iter().find_map(|piece| match piece {
            Piece::Keyword(k) => Some(k),
            _ => None,
        })
    }

    /// The description in pieces (see [`Piece`]).
    pub fn pieces(&self) -> Vec<Piece> {
        let text = |s: &str| Piece::Text(s.to_string());
        let keyword = Piece::Keyword;
        match self {
            ButtonAction::Disabled => vec![text("Disabled")],
            ButtonAction::Gamepad(b) => vec![Piece::Pad(*b)],
            ButtonAction::Keys(keys) if keys.is_empty() => vec![text("(no key)")],
            ButtonAction::Keys(keys) => keys.iter().enumerate().flat_map(|(i, k)| [text(if i > 0 { " + " } else { "" }), Piece::Key(k.clone())]).collect(),
            ButtonAction::Mouse(m) => vec![Piece::Mouse(*m)],
            ButtonAction::Wheel(d) => vec![text(&d.to_string())],
            ButtonAction::ToggleOverlay => vec![text("On-screen keyboard")],
            ButtonAction::ToggleNumpad => vec![text("On-screen numpad")],
            ButtonAction::Screenshot => vec![text("Screenshot")],
            ButtonAction::ToggleRecording => vec![text("Start / stop recording")],
            ButtonAction::ToggleMedia => vec![text("Media controls")],
            ButtonAction::ForceQuit => vec![text("Force quit focused window (hold)")],
            ButtonAction::OpenMenu(name) => vec![keyword(Keyword::Menu), text(&format!(" “{name}”"))],
            ButtonAction::ShowInfo(name) => vec![keyword(Keyword::Info), text(&format!(" “{name}”"))],
            ButtonAction::ShowLog(name) => vec![keyword(Keyword::Log), text(&format!(" “{name}”"))],
            ButtonAction::Multi(list) if list.is_empty() => vec![text("(nothing)")],
            ButtonAction::Multi(list) => {
                let mut pieces = Vec::new();
                for (i, action) in list.iter().enumerate() {
                    if i > 0 {
                        pieces.push(text(" & "));
                    }
                    pieces.extend(action.pieces());
                }
                pieces
            }
            ButtonAction::Toggle(t) => {
                let mut pieces = vec![keyword(Keyword::Toggle), text(" ")];
                pieces.extend(t.action.pieces());
                if t.start_on {
                    pieces.push(text(" (starts on)"));
                }
                pieces
            }
            ButtonAction::Turbo { action, every_ms, .. } if matches!(**action, ButtonAction::Macro { .. }) => {
                let mut pieces = vec![keyword(Keyword::Turbo), text(" ")];
                pieces.extend(action.pieces());
                pieces.push(text(&format!(" (every {every_ms} ms)")));
                pieces
            }
            ButtonAction::Turbo { action, rate, .. } => {
                let mut pieces = vec![keyword(Keyword::Turbo), text(" ")];
                pieces.extend(action.pieces());
                pieces.push(text(&format!(" ({rate:.0}/s)")));
                pieces
            }
            ButtonAction::Macro { name, repeat: true } => vec![keyword(Keyword::Macro), text(&format!(" “{name}” (repeat)"))],
            ButtonAction::Macro { name, .. } => vec![keyword(Keyword::Macro), text(&format!(" “{name}”"))],
            ButtonAction::Layer(name) => vec![keyword(Keyword::Layer), text(&format!(" “{name}”"))],
        }
    }
}

/// A key's label, with single punctuation characters quoted so they stay visible (“;”, “]”).
fn key_name(code: &str) -> String {
    let label = crate::keyboard::label(code);
    let mut chars = label.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_alphanumeric() => format!("“{c}”"),
        _ => label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_and_pad_buttons_are_pieces_and_plain_text_matches() {
        let menu = ButtonAction::OpenMenu("Belt".into());
        assert_eq!(menu.pieces(), [Piece::Keyword(Keyword::Menu), Piece::Text(" “Belt”".into())]);
        assert_eq!(menu.summary(), "Menu “Belt”");
        let pad = ButtonAction::Gamepad(Button::South);
        assert_eq!(pad.pieces(), [Piece::Pad(Button::South)]);
        assert_eq!(pad.summary(), "A");
        let turbo = ButtonAction::Turbo { action: Box::new(ButtonAction::Gamepad(Button::East)), rate: 12.0, every_ms: 0 };
        assert_eq!(turbo.summary(), "Turbo B (12/s)");
    }
}

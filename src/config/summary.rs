//! Short descriptions of buttons and actions, for the settings window and the input log.

use super::{Button, ButtonAction};

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
        }
    }
}

impl ButtonAction {
    /// One-line description, for collapsed rows, the controller drawing and the input log.
    pub fn summary(&self) -> String {
        match self {
            ButtonAction::Disabled => "Disabled".into(),
            ButtonAction::Gamepad(b) => format!("Pad {}", b.short_name()),
            ButtonAction::Keys(keys) if keys.is_empty() => "(no key)".into(),
            ButtonAction::Keys(keys) => keys.iter().map(|k| key_name(k)).collect::<Vec<_>>().join(" + "),
            ButtonAction::Mouse(m) => format!("{m} click"),
            ButtonAction::Wheel(d) => d.to_string(),
            ButtonAction::NextProfile => "Next profile".into(),
            ButtonAction::ToggleOverlay => "On-screen keyboard".into(),
            ButtonAction::ToggleNumpad => "On-screen numpad".into(),
            ButtonAction::Screenshot => "Screenshot".into(),
            ButtonAction::OpenMenu(name) => format!("Menu “{name}”"),
            ButtonAction::ShowInfo(name) => format!("Info “{name}”"),
            ButtonAction::ShowLog(name) => format!("Log “{name}”"),
            ButtonAction::Multi(list) if list.is_empty() => "(nothing)".into(),
            ButtonAction::Multi(list) => list.iter().map(ButtonAction::summary).collect::<Vec<_>>().join(" & "),
            ButtonAction::Toggle(t) if t.start_on => format!("Toggle {} (starts on)", t.action.summary()),
            ButtonAction::Toggle(t) => format!("Toggle {}", t.action.summary()),
            ButtonAction::Turbo { action, rate } => format!("Turbo {} ({rate:.0}/s)", action.summary()),
            ButtonAction::Macro { name, repeat: true } => format!("Macro “{name}” (repeat)"),
            ButtonAction::Macro { name, .. } => format!("Macro “{name}”"),
            ButtonAction::Layer(name) => format!("Layer “{name}”"),
            ButtonAction::Shift { layer, tap } => format!("Layer “{layer}” (tap: {})", tap.summary()),
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

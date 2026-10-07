//! A keyboard profile's gestures (double tap, triple tap, long press) and combos, and their timing.

use iced::widget::{column, row};

use super::*;
use super::check::input_label;
use crate::config::KeyboardCombo;
use crate::engine::RawInput;


fn mouse_name(b: MouseButton) -> String {
    crate::engine::input_name(RawInput::Button(b)).unwrap_or_default()
}

impl App {
    pub(super) fn view_gestures<'a>(&'a self, map: &'a KeyboardMap) -> Element<'a, Message> {
        let names = self.names();
        let mut rows = column![].spacing(10);
        for (name, gestures) in &map.gestures {
            let mut lines = column![
                row![text(input_label(name)).size(16), space::horizontal(), button(text("✕").size(14)).style(style::secondary).on_press(Message::KbGestureDrop(name.clone()))]
                    .align_y(Alignment::Center)
            ]
            .spacing(6);
            for kind in GestureKind::ALL {
                let (n, id) = (name.clone(), name.clone());
                let editor: Element<'a, Message> = match gestures.get(kind) {
                    Some(action) => {
                        let on_change: OnAction<'a> = Rc::new(move |a| Message::KbGestureSet(n.clone(), kind, Some(a)));
                        let clear = button(text("✕").size(13)).style(style::secondary).on_press(Message::KbGestureSet(id.clone(), kind, None));
                        let field = KeyField::KbGesture(id, kind, Vec::new());
                        row![action_editor(action, Button::South, KB_ACTION_KINDS, on_change, field, &names), clear].spacing(6).align_y(Alignment::Start).into()
                    }
                    None => button(text("+ Set").size(13))
                        .style(style::secondary)
                        .padding([3, 10])
                        .on_press(Message::KbGestureSet(id, kind, Some(ButtonAction::Keys(Vec::new()))))
                        .into(),
                };
                lines = lines.push(labeled(format!("    {kind}"), editor));
            }
            rows = rows.push(container(lines).padding(10).width(Length::Fill).style(style::inset));
        }
        let add_key = button(text("+ Key")).style(style::secondary).on_press(Message::OpenKeyPicker(KeyField::KbGestureNew, Vec::new(), true));
        let free: Vec<MouseButton> = MouseButton::ALL.into_iter().filter(|b| !map.gestures.contains_key(&mouse_name(*b))).collect();
        let add_mouse = dropdown(free, None::<MouseButton>, Message::KbGestureMouse).placeholder("+ Mouse button");
        let empty = map.gestures.is_empty().then(|| text("No key or mouse button has gestures.").size(13).color(MUTED_COLOR));
        section(
            "Gestures",
            Some("Give a key or mouse button more than one action: a double tap, a triple tap, or a long press. A quick single tap acts once the gesture is decided; holding past the tap window presses the input's own action right away.".into()),
            vec![
                column![].extend(empty.map(Element::from)).push(rows).spacing(8).into(),
                row![add_key, add_mouse].spacing(8).into(),
                value_slider("Tap window", 100.0..=600.0, self.profile().map_or(0, |p| p.tap_window_ms) as f32, 10.0, "ms", Message::SetTapWindow),
                value_slider("Long press after", 200.0..=1500.0, self.profile().map_or(0, |p| p.long_press_ms) as f32, 50.0, "ms", Message::SetLongPress),
            ],
        )
    }

    pub(super) fn view_combos<'a>(&'a self, map: &'a KeyboardMap) -> Element<'a, Message> {
        let names = self.names();
        let mut rows = column![].spacing(10);
        for (i, combo) in map.combos.iter().enumerate() {
            let title = if combo.inputs.is_empty() { "(nothing yet)".to_string() } else { combo.inputs.iter().map(|n| input_label(n)).collect::<Vec<_>>().join(" + ") };
            let keys: Vec<String> = combo.inputs.iter().filter(|n| n.starts_with("KEY_")).cloned().collect();
            let mut inputs = row![
                text(title).size(16),
                button(text("⌨ Keys").size(13)).style(style::secondary).on_press(Message::OpenKeyPicker(KeyField::KbComboKeys(i), keys, false)),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            for b in MouseButton::ALL {
                let on = combo.inputs.contains(&mouse_name(b));
                inputs = inputs.push(checkbox(on).label(b.to_string()).size(14).text_size(13).on_toggle(move |on| Message::KbComboMouse(i, b, on)));
            }
            let on_change: OnAction<'a> = Rc::new(move |a| Message::KbComboAction(i, a));
            let editor = action_editor(&combo.action, Button::South, KB_ACTION_KINDS, on_change, KeyField::KbCombo(i, Vec::new()), &names);
            let problem = (combo.inputs.len() < 2).then(|| text("⚠ needs at least two keys or buttons").size(12).color(ERROR_COLOR));
            let body = column![inputs.push(space::horizontal()).push(button(text("Remove").size(13)).style(button::danger).on_press(Message::KbRemoveCombo(i)))]
                .extend(problem.map(Element::from))
                .push(labeled("    Does", editor))
                .spacing(8);
            rows = rows.push(container(body).padding(10).width(Length::Fill).style(style::inset));
        }
        let empty = map.combos.is_empty().then(|| text("No combos.").size(13).color(MUTED_COLOR));
        section(
            "Combos",
            Some("Keys and mouse buttons pressed together act as an input of their own. Each member waits a short window for the rest of the combo before acting alone.".into()),
            vec![
                column![].extend(empty.map(Element::from)).push(rows).spacing(8).into(),
                button(text("+ New combo")).style(style::secondary).on_press(Message::KbAddCombo).into(),
                value_slider("Combo window", 20.0..=300.0, self.profile().map_or(0, |p| p.combo_window_ms) as f32, 5.0, "ms", Message::SetComboWindow),
            ],
        )
    }

    /// Handles the messages for gestures and combos; gives back any other.
    pub(super) fn update_extras(&mut self, message: Message) -> Option<Message> {
        match message {
            Message::KbGestureSet(name, kind, action) => self.edit_keymap(|map| {
                let gestures = map.gestures.entry(name).or_default();
                *gestures.slot(kind) = action;
            }),
            Message::KbGestureDrop(name) => self.edit_keymap(|map| drop(map.gestures.remove(&name))),
            Message::KbGestureMouse(b) => self.edit_keymap(|map| {
                map.gestures.entry(mouse_name(b)).or_default();
            }),
            Message::KbAddCombo => self.edit_keymap(|map| map.combos.push(KeyboardCombo { inputs: Vec::new(), action: ButtonAction::Keys(Vec::new()) })),
            Message::KbRemoveCombo(i) => self.edit_keymap(|map| {
                if i < map.combos.len() {
                    map.combos.remove(i);
                }
            }),
            Message::KbComboMouse(i, b, on) => self.edit_keymap(|map| {
                let Some(combo) = map.combos.get_mut(i) else { return };
                combo.inputs.retain(|n| *n != mouse_name(b));
                if on {
                    combo.inputs.push(mouse_name(b));
                }
            }),
            Message::KbComboAction(i, action) => self.edit_keymap(|map| {
                if let Some(combo) = map.combos.get_mut(i) {
                    combo.action = action;
                }
            }),
            other => return Some(other),
        }
        None
    }
}

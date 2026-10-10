//! Key fields: the text field for a key or chord, and the on-screen key picker it opens.

use iced::widget::{column, row};

use super::*;

/// Address of a key field, so the on-screen keyboard can write its result back.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum KeyField {
    /// A `Keys` action at `target`, reached through these `Multi` list indices.
    Action { target: Target, path: Vec<usize> },
}

impl KeyField {
    pub(super) fn root(target: Target) -> Self {
        KeyField::Action { target, path: Vec::new() }
    }

    pub(super) fn child(&self, index: usize) -> Self {
        let KeyField::Action { target, path } = self;
        let mut path = path.clone();
        path.push(index);
        KeyField::Action { target: *target, path }
    }
}

/// Open on-screen keyboard. `single` fields hold one key and close on the first click.
pub(super) struct KeyPicker {
    pub(super) field: KeyField,
    pub(super) keys: Vec<String>,
    pub(super) single: bool,
}

/// Follows `Multi` list indices down from `action`.
pub(super) fn action_at<'a>(action: &'a mut ButtonAction, path: &[usize]) -> Option<&'a mut ButtonAction> {
    match path.split_first() {
        None => Some(action),
        Some((i, rest)) => match action {
            ButtonAction::Multi(list) => action_at(list.get_mut(*i)?, rest),
            // Toggle and Turbo wrap a single action, addressed as index 0.
            ButtonAction::Toggle(Toggled { action: inner, .. }) | ButtonAction::Turbo { action: inner, .. } if *i == 0 => {
                action_at(inner, rest)
            }
            _ => None,
        },
    }
}

/// Modal on-screen keyboard over a dimmed backdrop; clicking the backdrop cancels.
pub(super) fn view_picker(picker: &KeyPicker) -> Element<'_, Message> {
    let selection = if picker.keys.is_empty() {
        "Nothing selected".to_string()
    } else {
        picker.keys.iter().map(|k| keyboard::label(k)).collect::<Vec<_>>().join(" + ")
    };
    let (title, hint) = if picker.single {
        ("Pick a key", "Click a key to choose it.")
    } else {
        ("Pick keys", "Click keys to toggle them. Selected keys are pressed together, in the order chosen.")
    };
    let mut actions = row![
        button(text("Clear")).style(style::secondary).on_press(Message::PickerClear),
        space::horizontal(),
        button(text("Cancel")).style(style::secondary).on_press(Message::PickerClose { apply: false }),
    ]
    .spacing(8);
    if !picker.single {
        actions = actions.push(button(text("Done")).on_press(Message::PickerClose { apply: true }));
    }
    let dialog = container(
        column![
            text(title).size(20),
            text(hint).size(13).color(MUTED_COLOR),
            text(selection).size(16),
            keyboard::view(&picker.keys, Message::PickerKey),
            actions,
        ]
        .spacing(12),
    )
    .padding(20)
    .style(style::inset);

    let backdrop = container(center(opaque(dialog)))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Color { a: 0.6, ..Color::BLACK }.into()),
            ..container::Style::default()
        });
    opaque(mouse_area(backdrop).on_press(Message::PickerClose { apply: false }))
}

/// Text input for evdev key names (without the `KEY_` prefix) that turns red when invalid.
/// Also offers a button that opens the on-screen keyboard (`open_picker`).
pub(super) fn key_input<'a>(
    keys: &[String],
    placeholder: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    open_picker: Message,
) -> Element<'a, Message> {
    let valid = keys.iter().all(|k| KeyCode::from_str(k).is_ok());
    let input = field(placeholder, &keys_to_text(keys)).on_input(on_input).width(180);
    let pick = button(text("⌨").size(14)).style(style::secondary).on_press(open_picker);
    let mut r = row![input, pick].spacing(6).align_y(Alignment::Center);
    if !valid {
        r = r.push(text("unknown key").size(12).color(ERROR_COLOR));
    }
    r.into()
}

pub(super) fn short_key(k: &str) -> &str {
    k.strip_prefix("KEY_").unwrap_or(k)
}

/// A key as the key field shows it: its name on the keyboard, as the summaries write it. The
/// field is typed in and reread on every keystroke, so a name that can't be typed back (an
/// arrow, or "Vol +", whose "+" separates keys) is spelled out instead.
fn field_key(code: &str) -> String {
    let arrow = match code {
        "KEY_UP" => "Up",
        "KEY_DOWN" => "Down",
        "KEY_LEFT" => "Left",
        "KEY_RIGHT" => "Right",
        _ => "",
    };
    let label = crate::keyboard::label(code);
    if !arrow.is_empty() {
        arrow.into()
    } else if label.is_ascii() && !label.contains('+') {
        label
    } else {
        short_key(code).into()
    }
}

pub(super) fn keys_to_text(keys: &[String]) -> String {
    keys.iter().map(|k| field_key(k)).collect::<Vec<_>>().join("+")
}

/// The keys typed in a key field: keyboard names ("Left Ctrl+C") or evdev names without `KEY_`
/// ("leftctrl+c"), in any case.
pub(super) fn text_to_keys(s: &str) -> Vec<String> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    s.split('+')
        .map(|name| match crate::keyboard::code_for_label(name.trim()) {
            Some(code) => code.to_string(),
            None => format!("KEY_{}", name.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase()),
        })
        .collect()
}

impl App {
    pub(super) fn update_actions(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenKeyPicker(field, keys, single) => {
                let keys = keys.into_iter().filter(|k| KeyCode::from_str(k).is_ok()).collect();
                self.picker = Some(KeyPicker { field, keys, single });
            }
            Message::PickerKey(code) => {
                if let Some(picker) = &mut self.picker {
                    if picker.single {
                        picker.keys = vec![code.to_string()];
                        return Task::done(Message::PickerClose { apply: true });
                    }
                    match picker.keys.iter().position(|k| k == code) {
                        Some(i) => {
                            picker.keys.remove(i);
                        }
                        None => picker.keys.push(code.to_string()),
                    }
                }
            }
            Message::PickerClear => {
                if let Some(picker) = &mut self.picker {
                    picker.keys.clear();
                }
            }
            Message::PickerClose { apply } => {
                if let Some(picker) = self.picker.take()
                    && apply
                {
                    self.apply_keys(picker.field, picker.keys);
                }
            }
            other => return self.update_layers(other),
        }
        Task::none()
    }

    /// Writes keys chosen in the on-screen keyboard into the field it was opened for.
    pub(super) fn apply_keys(&mut self, field: KeyField, keys: Vec<String>) {
        if let KeyField::Action { target: Target::MenuItem(m, i), path } = &field {
            let item = self.menus_mut().get_mut(*m).and_then(|m| m.items.get_mut(*i));
            if let Some(action) = item.and_then(|item| action_at(&mut item.action, path)) {
                *action = ButtonAction::Keys(keys);
            }
            return;
        }
        if let KeyField::Action { target: Target::MacroStep(m, s), path } = &field {
            let step = self.macros_mut().get_mut(*m).and_then(|m| m.steps.get_mut(*s));
            if let Some(action) = step.and_then(|s| s.action_mut()).and_then(|a| action_at(a, path)) {
                *action = ButtonAction::Keys(keys);
            }
            return;
        }
        let Some(p) = self.profile_mut() else { return };
        match field {
            KeyField::Action { target, path } => {
                let root = match target {
                    Target::Button(b) => Some(p.buttons.entry(b).or_insert(ButtonAction::Disabled)),
                    Target::Trigger(t) => match p.trigger_mut(t) {
                        TriggerAction::Button { action, .. } => Some(action),
                        _ => None,
                    },
                    Target::Combo(i) => p.combos.get_mut(i).map(|c| &mut c.action),
                    Target::Gesture(b, kind) => p.gestures.get_mut(&b).and_then(|g| g.slot(kind).as_mut()),
                    Target::Zone(a, i) => p.zones_mut(a).get_mut(i).map(|z| &mut z.action),
                    Target::RingSector(st, i) => p.stick_mut(st).action.ring_actions_mut().get_mut(i),
                    // Handled above, outside any profile.
                    Target::MacroStep(..) | Target::MenuItem(..) | Target::StickResponse(_) => None,
                };
                if let Some(action) = root.and_then(|a| action_at(a, &path)) {
                    *action = ButtonAction::Keys(keys);
                }
            }
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::tests::*;

    #[test]
    fn picker_writes_chord_into_nested_multi_entry() {
        let mut app = app();
        let multi = ButtonAction::Multi(vec![ButtonAction::Gamepad(Button::South), ButtonAction::Keys(vec![])]);
        app.config.general.profiles[0].set_button(Button::South, multi);
        let field = KeyField::root(Target::Button(Button::South)).child(1);
        // Toggling a key twice removes it again.
        pick(&mut app, field, false, &["KEY_LEFTSHIFT", "KEY_A", "KEY_B", "KEY_B"]);
        assert_eq!(
            app.config.general.profiles[0].button(Button::South),
            &ButtonAction::Multi(vec![
                ButtonAction::Gamepad(Button::South),
                ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into(), "KEY_A".into()]),
            ])
        );
        assert!(app.picker.is_none());
    }

    #[test]
    fn wrapping_in_toggle_keeps_the_action_and_picker_writes_inside_it() {
        let mut app = app();
        app.config.general.profiles[0].set_button(Button::RightStick, ButtonAction::Keys(vec!["KEY_C".into()]));
        // What choosing "Toggle" in the kind list produces for an existing key action.
        let wrapped = ButtonAction::toggle(ButtonAction::Keys(vec!["KEY_C".into()]));
        let _ = app.update(Message::SetAction(Target::Button(Button::RightStick), wrapped));
        let field = KeyField::root(Target::Button(Button::RightStick)).child(0);
        pick(&mut app, field, false, &["KEY_LEFTCTRL"]);
        assert_eq!(
            app.config.general.profiles[0].button(Button::RightStick),
            &ButtonAction::toggle(ButtonAction::Keys(vec!["KEY_LEFTCTRL".into()]))
        );
    }

    #[test]
    fn cancel_leaves_config_untouched() {
        let mut app = app();
        let before = app.config.clone();
        let _ = app.update(Message::OpenKeyPicker(KeyField::root(Target::Button(Button::North)), vec![], false));
        let _ = app.update(Message::PickerKey("KEY_Q"));
        let _ = app.update(Message::PickerClose { apply: false });
        assert_eq!(app.config, before);
    }

    #[test]
    fn key_text_roundtrip() {
        for s in ["", "Left Ctrl+C", "Left Ctrl+", "A", "Space", ";", "Up", "VOLUMEUP", "Keypad Plus"] {
            assert_eq!(keys_to_text(&text_to_keys(s)), s);
        }
        assert_eq!(text_to_keys("leftctrl + c"), vec!["KEY_LEFTCTRL", "KEY_C"]);
        assert_eq!(text_to_keys("left ctrl+SPACE"), vec!["KEY_LEFTCTRL", "KEY_SPACE"]);
        // Typing a code name letter by letter: LEFT is the arrow until it grows into LEFTCTRL.
        assert_eq!(keys_to_text(&text_to_keys("LEFT")), "Left");
        assert_eq!(keys_to_text(&text_to_keys("LeftC")), "LEFTC");
        assert_eq!(keys_to_text(&text_to_keys("LEFTCTRL")), "Left Ctrl");
    }
}

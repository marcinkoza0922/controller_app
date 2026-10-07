//! The keyboard profile editor: which keys and mouse buttons become which.

use iced::widget::{column, row};

use super::*;
use crate::config::{KeyboardMap, OtherKeys, is_reserved_key};

/// An input of a keyboard profile: a key (`KEY_W`) or a mouse button.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum KbInput {
    Key(String),
    Mouse(MouseButton),
}

/// What a remapped input becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KbKind {
    Keys,
    Mouse,
    Disabled,
}

impl KbKind {
    const ALL: [KbKind; 3] = [KbKind::Keys, KbKind::Mouse, KbKind::Disabled];

    fn of(action: &ButtonAction) -> Option<KbKind> {
        match action {
            ButtonAction::Keys(_) => Some(KbKind::Keys),
            ButtonAction::Mouse(_) => Some(KbKind::Mouse),
            ButtonAction::Disabled => Some(KbKind::Disabled),
            _ => None,
        }
    }
}

impl fmt::Display for KbKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            KbKind::Keys => "Key or combo",
            KbKind::Mouse => "Mouse button",
            KbKind::Disabled => "Nothing",
        })
    }
}

fn map_action<'a>(map: &'a mut KeyboardMap, input: &KbInput) -> Option<&'a mut ButtonAction> {
    match input {
        KbInput::Key(k) => map.keys.get_mut(k),
        KbInput::Mouse(b) => map.mouse.get_mut(b),
    }
}

impl App {
    pub(super) fn view_keyboard_profile<'a>(&'a self, p: &'a Profile) -> Element<'a, Message> {
        let map = &p.keyboard;
        let mut keys = column![].spacing(6);
        for (key, action) in &map.keys {
            let source = button(text(keyboard::label(key))).style(style::secondary).width(120).on_press(Message::OpenKeyPicker(
                KeyField::KbSource(KbInput::Key(key.clone())),
                vec![key.clone()],
                true,
            ));
            keys = keys.push(self.kb_row(source.into(), KbInput::Key(key.clone()), action));
        }
        let mut mouse = column![].spacing(6);
        for (button_, action) in &map.mouse {
            let old = *button_;
            let source = dropdown(MouseButton::ALL, Some(old), move |new| Message::KbMouseSource(old, new)).width(120);
            mouse = mouse.push(self.kb_row(source.into(), KbInput::Mouse(old), action));
        }
        let empty = |what: &'static str| text(format!("No {what} are remapped: they work as normal.")).size(13).color(MUTED_COLOR);
        let add_key = button(text("+ Remap a key")).style(style::secondary).on_press(Message::OpenKeyPicker(KeyField::KbNew, Vec::new(), true));
        let add_mouse = button(text("+ Remap a mouse button")).style(style::secondary).on_press_maybe(
            MouseButton::ALL.into_iter().find(|b| !map.mouse.contains_key(b)).map(|_| Message::KbAddMouse),
        );
        column![
            section(
                "Other keys",
                Some("What happens to keys and mouse buttons this profile doesn't remap. Blocking is for games where stray keys are unwanted. Mouse movement and the wheel always work, and so do the Super keys and the Ctrl+Alt+F1–F12 console switches.".into()),
                vec![row![
                    text("Keys without a mapping"),
                    dropdown(OtherKeys::ALL, Some(map.other_keys), Message::KbOtherKeys).width(180),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into()],
            ),
            section(
                "Keys",
                Some("Pick a key, then what pressing it does instead. Super (the Windows key) can't be remapped.".into()),
                vec![if map.keys.is_empty() { empty("keys").into() } else { keys.into() }, add_key.into()],
            ),
            section(
                "Mouse buttons",
                None,
                vec![if map.mouse.is_empty() { empty("mouse buttons").into() } else { mouse.into() }, add_mouse.into()],
            ),
        ]
        .spacing(16)
        .into()
    }

    /// Settings: the chord that turns remapping off.
    pub(super) fn view_panic_card(&self) -> Element<'_, Message> {
        let chord = &self.config.panic_chord;
        let blank = chord.is_empty();
        let mut line = row![
            text("Panic chord"),
            key_input(
                &keys_to_text(chord),
                "e.g. LEFTCTRL+LEFTALT+LEFTSHIFT+ESC",
                |s| Message::SetPanicChord(text_to_keys(&s)),
                Message::OpenKeyPicker(KeyField::PanicChord, chord.clone(), false),
            ),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        if blank {
            line = line.push(text("can't be empty").size(12).color(ERROR_COLOR));
        }
        section(
            "Keyboard and mouse",
            Some("Keyboard profiles take over your keyboard and mouse while they are active. Hold these keys together to turn all remapping off if something goes wrong; turn it back on in this window. Either Ctrl, Alt or Shift counts.".into()),
            vec![line.into()],
        )
    }

    /// One remapped input: what it is, what it becomes, and a way to take the mapping away.
    fn kb_row<'a>(&'a self, source: Element<'a, Message>, input: KbInput, action: &'a ButtonAction) -> Element<'a, Message> {
        let kind = KbKind::of(action);
        let target: Element<'a, Message> = match action {
            ButtonAction::Keys(keys) => {
                let id = input.clone();
                key_input(
                    &keys_to_text(keys),
                    "e.g. E or LEFTCTRL+C",
                    move |s| Message::KbKeysText(id.clone(), s),
                    Message::OpenKeyPicker(KeyField::KbTarget(input.clone()), keys.clone(), false),
                )
            }
            ButtonAction::Mouse(b) => {
                let id = input.clone();
                dropdown(MouseButton::ALL, Some(*b), move |b| Message::KbMouseTarget(id.clone(), b)).width(120).into()
            }
            ButtonAction::Disabled => text("The input does nothing").size(13).color(MUTED_COLOR).into(),
            _ => text("Not available for keyboard profiles").size(13).color(ERROR_COLOR).into(),
        };
        let id = input.clone();
        row![
            source,
            text("▸"),
            dropdown(KbKind::ALL, kind, move |k| Message::KbKind(id.clone(), k)).width(150),
            target,
            space::horizontal(),
            button(text("✕").size(14)).style(style::secondary).on_press(Message::KbRemove(input)),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    }

    pub(super) fn update_keymap(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SetPanicChord(chord) => self.config.panic_chord = chord,
            Message::KbOtherKeys(other) => self.edit_keymap(|map| map.other_keys = other),
            Message::KbAddMouse => self.edit_keymap(|map| {
                if let Some(free) = MouseButton::ALL.into_iter().find(|b| !map.mouse.contains_key(b)) {
                    map.mouse.insert(free, ButtonAction::Disabled);
                }
            }),
            Message::KbRemove(input) => self.edit_keymap(|map| match input {
                KbInput::Key(k) => drop(map.keys.remove(&k)),
                KbInput::Mouse(b) => drop(map.mouse.remove(&b)),
            }),
            Message::KbKind(input, kind) => self.edit_keymap(|map| {
                if let Some(action) = map_action(map, &input) {
                    *action = match kind {
                        KbKind::Keys => ButtonAction::Keys(Vec::new()),
                        KbKind::Mouse => ButtonAction::Mouse(MouseButton::Left),
                        KbKind::Disabled => ButtonAction::Disabled,
                    };
                }
            }),
            Message::KbKeysText(input, text) => self.edit_keymap(|map| {
                if let Some(action) = map_action(map, &input) {
                    *action = ButtonAction::Keys(text_to_keys(&text));
                }
            }),
            Message::KbMouseTarget(input, button) => self.edit_keymap(|map| {
                if let Some(action) = map_action(map, &input) {
                    *action = ButtonAction::Mouse(button);
                }
            }),
            Message::KbMouseSource(old, new) => {
                if old != new && self.profile().is_some_and(|p| p.keyboard.mouse.contains_key(&new)) {
                    self.message = Some((format!("{new} is already remapped."), true));
                    return Task::none();
                }
                self.edit_keymap(|map| {
                    if let Some(action) = map.mouse.remove(&old) {
                        map.mouse.insert(new, action);
                    }
                });
            }
            other => return self.update_actions(other),
        }
        Task::none()
    }

    fn edit_keymap(&mut self, edit: impl FnOnce(&mut KeyboardMap)) {
        if let Some(p) = self.profile_mut() {
            edit(&mut p.keyboard);
        }
    }

    /// Writes what the on-screen keyboard picked into a keyboard profile.
    pub(super) fn apply_keymap_keys(&mut self, field: KeyField, keys: Vec<String>) {
        match field {
            KeyField::KbTarget(input) => self.edit_keymap(|map| {
                if let Some(action) = map_action(map, &input) {
                    *action = ButtonAction::Keys(keys);
                }
            }),
            KeyField::KbSource(KbInput::Key(old)) => {
                let Some(new) = keys.into_iter().next().filter(|k| *k != old) else { return };
                if let Some(problem) = self.new_source_problem(&new) {
                    self.message = Some((problem, true));
                    return;
                }
                self.edit_keymap(|map| {
                    if let Some(action) = map.keys.remove(&old) {
                        map.keys.insert(new, action);
                    }
                });
            }
            KeyField::PanicChord => self.config.panic_chord = keys,
            KeyField::KbNew => {
                let Some(new) = keys.into_iter().next() else { return };
                if let Some(problem) = self.new_source_problem(&new) {
                    self.message = Some((problem, true));
                    return;
                }
                self.edit_keymap(|map| drop(map.keys.insert(new, ButtonAction::Keys(Vec::new()))));
            }
            _ => {}
        }
    }

    /// Why `key` can't be remapped now, if it can't.
    fn new_source_problem(&self, key: &str) -> Option<String> {
        if is_reserved_key(key) {
            return Some(format!("{} can't be remapped: it always works as normal.", keyboard::label(key)));
        }
        let taken = self.profile().is_some_and(|p| p.keyboard.keys.contains_key(key));
        taken.then(|| format!("{} is already remapped.", keyboard::label(key)))
    }
}

/// The first thing saving would reject in a keyboard profile's mappings.
pub(super) fn keyboard_problem(map: &KeyboardMap) -> Option<String> {
    let known = |k: &str| KeyCode::from_str(k).is_ok();
    if let Some(bad) = map.keys.keys().find(|k| !known(k)) {
        return Some(format!("unknown key {:?}", short_key(bad)));
    }
    if let Some(key) = map.keys.keys().find(|k| is_reserved_key(k)) {
        return Some(format!("{} can't be remapped", keyboard::label(key)));
    }
    let named = map.keys.iter().map(|(k, a)| (keyboard::label(k), a));
    let buttons = map.mouse.iter().map(|(b, a)| (format!("{b} mouse button"), a));
    for (name, action) in named.chain(buttons) {
        match action {
            ButtonAction::Disabled | ButtonAction::Mouse(_) => {}
            ButtonAction::Keys(keys) if keys.is_empty() => return Some(format!("{name} has no key to press")),
            ButtonAction::Keys(keys) => {
                if let Some(bad) = keys.iter().find(|k| !known(k)) {
                    return Some(format!("unknown key {:?}", short_key(bad)));
                }
                if let Some(key) = keys.iter().find(|k| is_reserved_key(k)) {
                    return Some(format!("{} can't be pressed by a remap", keyboard::label(key)));
                }
            }
            _ => return Some(format!("{name} has an action that keyboard profiles don't support yet")),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::tests::app;

    /// The app editing a new keyboard profile in General.
    fn keyboard_app() -> App {
        let mut app = app();
        app.config.general.profiles.push(Profile::keyboard("Keys"));
        app.editing = app.config.general.profiles.len() - 1;
        app
    }

    /// Picks keys in the on-screen keyboard, closing a single-key pick the way the runtime would.
    fn pick(app: &mut App, field: KeyField, single: bool, keys: &[&'static str]) {
        crate::gui::tests::pick(app, field, single, keys);
        if single {
            let _ = app.update(Message::PickerClose { apply: true });
        }
    }

    fn map(app: &App) -> &KeyboardMap {
        &app.profile().unwrap().keyboard
    }

    #[test]
    fn a_picked_key_is_added_and_can_be_pointed_at_another_key() {
        let mut app = keyboard_app();
        pick(&mut app, KeyField::KbNew, true, &["KEY_W"]);
        assert_eq!(map(&app).keys["KEY_W"], ButtonAction::Keys(vec![]));
        assert!(keyboard_problem(map(&app)).unwrap().contains("no key to press"));

        pick(&mut app, KeyField::KbTarget(KbInput::Key("KEY_W".into())), false, &["KEY_E"]);
        assert_eq!(map(&app).keys["KEY_W"], ButtonAction::Keys(vec!["KEY_E".into()]));
        assert_eq!(keyboard_problem(map(&app)), None);
    }

    #[test]
    fn super_and_repeated_keys_are_refused_with_a_message() {
        let mut app = keyboard_app();
        pick(&mut app, KeyField::KbNew, true, &["KEY_LEFTMETA"]);
        assert!(map(&app).keys.is_empty());
        assert!(app.message.as_ref().unwrap().0.contains("can't be remapped"));
        pick(&mut app, KeyField::KbNew, true, &["KEY_W"]);
        pick(&mut app, KeyField::KbNew, true, &["KEY_W"]);
        assert!(app.message.as_ref().unwrap().0.contains("already remapped"));
        assert_eq!(map(&app).keys.len(), 1);
    }

    #[test]
    fn repicking_a_source_moves_its_mapping() {
        let mut app = keyboard_app();
        app.profile_mut().unwrap().keyboard = Profile::wasd_to_esdf("x").keyboard;
        pick(&mut app, KeyField::KbSource(KbInput::Key("KEY_W".into())), true, &["KEY_Q"]);
        assert_eq!(map(&app).keys["KEY_Q"], ButtonAction::Keys(vec!["KEY_E".into()]));
        assert!(!map(&app).keys.contains_key("KEY_W"));
    }

    #[test]
    fn mouse_buttons_can_be_remapped_to_keys_and_removed() {
        let mut app = keyboard_app();
        let _ = app.update(Message::KbAddMouse);
        assert_eq!(map(&app).mouse[&MouseButton::Left], ButtonAction::Disabled);
        let left = KbInput::Mouse(MouseButton::Left);
        let _ = app.update(Message::KbKind(left.clone(), KbKind::Keys));
        let _ = app.update(Message::KbKeysText(left.clone(), "leftctrl+z".into()));
        assert_eq!(map(&app).mouse[&MouseButton::Left], ButtonAction::Keys(vec!["KEY_LEFTCTRL".into(), "KEY_Z".into()]));
        let _ = app.update(Message::KbMouseSource(MouseButton::Left, MouseButton::Back));
        assert!(map(&app).mouse.contains_key(&MouseButton::Back));
        let _ = app.update(Message::KbRemove(KbInput::Mouse(MouseButton::Back)));
        assert!(map(&app).mouse.is_empty());
    }

    #[test]
    fn the_panic_chord_cannot_be_blank_or_unknown() {
        let mut app = app();
        assert_eq!(app.validate(), None);
        app.config.panic_chord.clear();
        assert!(app.validate().unwrap().contains("can't be empty"));
        app.config.panic_chord = vec!["KEY_NOPE".into()];
        assert!(app.validate().unwrap().contains("unknown key"));
        pick(&mut app, KeyField::PanicChord, false, &["KEY_LEFTCTRL", "KEY_PAUSE"]);
        assert_eq!(app.config.panic_chord, ["KEY_LEFTCTRL", "KEY_PAUSE"]);
        assert_eq!(app.validate(), None);
    }

    #[test]
    fn a_default_keyboard_profile_must_be_a_keyboard_profile() {
        let mut app = keyboard_app();
        app.config.auto_switch.default_keyboard_profile = Some(ProfileRef::new(None, "Keys"));
        assert_eq!(app.validate(), None);
        app.config.auto_switch.default_keyboard_profile = Some(ProfileRef::new(None, "Desktop"));
        assert!(app.validate().unwrap().contains("default keyboard profile"));
    }
}

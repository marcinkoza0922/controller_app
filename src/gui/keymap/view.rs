//! What the keyboard profile editor shows.

use iced::widget::{column, row};

use super::*;
use crate::config::OtherKeys;

impl App {
    pub(in crate::gui) fn view_keyboard_profile<'a>(&'a self, p: &'a Profile) -> Element<'a, Message> {
        let map = &p.keyboard;
        let layer = self.editing_layer();
        let mut sections: Vec<Element<'a, Message>> = Vec::new();
        // What a layer can't change: how unmapped keys and mouse movement are handled.
        if !layer {
            sections.push(section(
                "Other keys",
                Some("What happens to keys and mouse buttons this profile doesn't remap. Blocking is for games where stray keys are unwanted. Mouse movement and the wheel always work, and so do the Super keys and the Ctrl+Alt+F1–F12 console switches.".into()),
                vec![row![
                    text("Keys without a mapping"),
                    dropdown(OtherKeys::ALL, Some(map.other_keys), Message::KbOtherKeys).width(180),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into()],
            ));
        }
        sections.extend([self.view_keys(map), self.view_mouse_buttons(map), self.view_wheel(map)]);
        if !layer {
            sections.push(self.view_motion(p));
        }
        sections.extend([self.view_motion_buttons(map), self.view_gestures(map), self.view_combos(map)]);
        column(sections)
        .spacing(16)
        .into()
    }

    fn view_keys<'a>(&'a self, map: &'a KeyboardMap) -> Element<'a, Message> {
        let names = self.names();
        let mut rows = column![].spacing(6);
        for (key, action) in &map.keys {
            let source = button(text(keyboard::label(key)))
                .style(style::secondary)
                .width(120)
                .on_press(Message::OpenKeyPicker(KeyField::KbSource(KbInput::Key(key.clone())), vec![key.clone()], true));
            rows = rows.push(kb_row(source.into(), KbInput::Key(key.clone()), action, &names));
        }
        let add = button(text("+ Remap a key")).style(style::secondary).on_press(Message::OpenKeyPicker(KeyField::KbNew, Vec::new(), true));
        section(
            "Keys",
            Some("Pick a key, then what pressing it does instead. Super (the Windows key) can't be remapped. Outputs to a gamepad create a virtual controller while this profile is active.".into()),
            vec![or_empty(map.keys.is_empty(), "keys", rows), add.into()],
        )
    }

    fn view_mouse_buttons<'a>(&'a self, map: &'a KeyboardMap) -> Element<'a, Message> {
        let names = self.names();
        let mut rows = column![].spacing(6);
        for (&old, action) in &map.mouse {
            let source = dropdown(MouseButton::ALL, Some(old), move |new| Message::KbMouseSource(old, new)).width(120);
            rows = rows.push(kb_row(source.into(), KbInput::Mouse(old), action, &names));
        }
        let free = MouseButton::ALL.into_iter().any(|b| !map.mouse.contains_key(&b));
        let add = button(text("+ Remap a mouse button")).style(style::secondary).on_press_maybe(free.then_some(Message::KbAddMouse));
        section("Mouse buttons", None, vec![or_empty(map.mouse.is_empty(), "mouse buttons", rows), add.into()])
    }

    fn view_wheel<'a>(&'a self, map: &'a KeyboardMap) -> Element<'a, Message> {
        let names = self.names();
        let mut rows = column![].spacing(6);
        for (&old, action) in &map.wheel {
            let source = dropdown(WheelDirection::ALL, Some(old), move |new| Message::KbWheelSource(old, new)).width(120);
            rows = rows.push(kb_row(source.into(), KbInput::Wheel(old), action, &names));
        }
        let free = WheelDirection::ALL.into_iter().any(|d| !map.wheel.contains_key(&d));
        let add = button(text("+ Remap the wheel")).style(style::secondary).on_press_maybe(free.then_some(Message::KbAddWheel));
        section(
            "Scroll wheel",
            Some("Each notch of the wheel presses the action once. Directions you don't list keep scrolling.".into()),
            vec![or_empty(map.wheel.is_empty(), "wheel directions", rows), add.into()],
        )
    }

    fn view_motion<'a>(&'a self, p: &'a Profile) -> Element<'a, Message> {
        let m = &p.keyboard.motion;
        let choice = match m.target {
            MotionTarget::Pointer => MotionChoice::Pointer,
            MotionTarget::Stick(s) => MotionChoice::Stick(s),
        };
        let edit = |f: fn(f32) -> MotionEdit| move |v| Message::KbMotion(f(v));
        let mut rows: Vec<Element<'a, Message>> =
            vec![labeled("Mouse movement", dropdown(MotionChoice::ALL, Some(choice), |c| Message::KbMotion(MotionEdit::Target(c))).width(220).into())];
        match m.target {
            MotionTarget::Pointer => rows.push(value_slider("    Speed", 0.2..=3.0, m.pointer_scale, 0.05, "× the normal pointer speed", edit(MotionEdit::PointerScale))),
            MotionTarget::Stick(s) => {
                rows.push(value_slider("    Full push at", 20.0..=500.0, m.counts, 5.0, "counts of movement (less is more sensitive)", edit(MotionEdit::Counts)));
                rows.push(value_slider("    Fades back in", 0.0..=500.0, m.decay_ms as f32, 10.0, "ms after you stop", edit(MotionEdit::DecayMs)));
                rows.push(value_slider("    Deadzone", 0.0..=0.4, p.stick(s).deadzone, 0.01, "", edit(MotionEdit::Deadzone)));
            }
        }
        rows.push(
            row![
                text("    "),
                checkbox(m.invert_x).label("Invert left/right").on_toggle(|on| Message::KbMotion(MotionEdit::InvertX(on))),
                checkbox(m.invert_y).label("Invert up/down").on_toggle(|on| Message::KbMotion(MotionEdit::InvertY(on))),
            ]
            .spacing(16)
            .into(),
        );
        section(
            "Mouse movement",
            Some("As a stick, moving the mouse pushes the stick and it drifts back to the middle when you stop, so a game made for a controller can be aimed with a mouse. The pointer stays put.".into()),
            rows,
        )
    }

    fn view_motion_buttons<'a>(&'a self, map: &'a KeyboardMap) -> Element<'a, Message> {
        let names = self.names();
        let mut rows = column![].spacing(6);
        for (&old, action) in &map.motion_buttons {
            let source = dropdown(MotionDirection::ALL, Some(old), move |new| Message::KbMotionSource(old, new)).width(140);
            rows = rows.push(kb_row(source.into(), KbInput::Motion(old), action, &names));
        }
        let free = MotionDirection::ALL.into_iter().any(|d| !map.motion_buttons.contains_key(&d));
        let add = button(text("+ Add a direction")).style(style::secondary).on_press_maybe(free.then_some(Message::KbAddMotion));
        section(
            "Mouse directions",
            Some("An action held while the mouse is moving that way, e.g. a flick left presses a key.".into()),
            vec![or_empty(map.motion_buttons.is_empty(), "mouse directions", rows), add.into()],
        )
    }
}

fn or_empty<'a>(empty: bool, what: &'static str, rows: iced::widget::Column<'a, Message>) -> Element<'a, Message> {
    if empty {
        text(format!("No {what} are remapped: they work as normal.")).size(13).color(MUTED_COLOR).into()
    } else {
        rows.into()
    }
}

impl App {
    /// Settings: the chord that turns remapping off.
    pub(in crate::gui) fn view_panic_card(&self) -> Element<'_, Message> {
        let chord = &self.config.panic_chord;
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
        if chord.is_empty() {
            line = line.push(text("can't be empty").size(12).color(ERROR_COLOR));
        }
        section(
            "Keyboard and mouse",
            Some("Keyboard profiles take over your keyboard and mouse while they are active. Hold these keys together to turn all remapping off if something goes wrong; turn it back on in this window. Either Ctrl, Alt or Shift counts.".into()),
            vec![line.into()],
        )
    }
}

/// One remapped input: what it is, what it becomes, and a way to take the mapping away.
fn kb_row<'a>(source: Element<'a, Message>, input: KbInput, action: &'a ButtonAction, names: &Names) -> Element<'a, Message> {
    let id = input.clone();
    let on_change: OnAction<'a> = Rc::new(move |a| Message::KbSetAction(id.clone(), a));
    let field = KeyField::KbTarget(input.clone(), Vec::new());
    row![
        source,
        text("▸"),
        action_editor(action, Button::South, KB_ACTION_KINDS, on_change, field, names),
        space::horizontal(),
        button(text("✕").size(14)).style(style::secondary).on_press(Message::KbRemove(input)),
    ]
    .spacing(10)
    .align_y(Alignment::Start)
    .into()
}

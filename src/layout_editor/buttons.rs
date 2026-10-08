//! The button pages of Edit Controls: what a button sends, its gestures (double and triple tap,
//! long press), and combos. Each one is a slot holding an action, so one page serves all three.

use std::collections::BTreeMap;

use crate::config::{Analog, Button, ButtonAction, GestureKind, MouseButton, Profile, Toggled, WheelDirection};

/// Where an action goes: a button's own action, one of its gestures, or a combo's action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Button(Button),
    Gesture(Button, GestureKind),
    Combo(usize),
    /// A button of a game's layer.
    Layer(usize, Button),
    /// A stick's or trigger's zone, by its index.
    Zone(Analog, usize),
}

/// One row of an action page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionRow {
    /// "Gamepad button…": opens the pad picker.
    Pad,
    /// "Gestures…": opens a button's gestures (buttons only).
    Gestures,
    /// One of `CHOICES`.
    Choice(usize),
    /// "Keys…": opens the key picker (buttons, gestures and combos; a layer button can too).
    Keys,
    /// Opens a picker of the game's layers, macros, menus or info overlays.
    Pick(Picker),
    /// Wraps the action in a toggle, or unwraps it.
    Wrap(Wrap),
    /// A setting of the wrapper the slot's action has now. Shown only while it has one.
    Tuning(Tuning),
    /// Clears a gesture, so it does nothing (gestures only).
    NotSet,
}

/// A wrapper around the action a slot already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrap {
    Toggle,
    Turbo,
}

/// The rate a new turbo presses at, in presses a second, and the rates its row cycles through.
const TURBO_RATE: f32 = 10.0;
const TURBO_RATES: [f32; 5] = [5.0, 10.0, 15.0, 20.0, 30.0];

/// A setting a wrapper has: a turbo's rate, or whether a toggle starts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tuning {
    TurboRate,
    ToggleStart,
}

/// The rows a slot's action page has for the wrapper its action has now: none unless it is
/// wrapped.
pub fn tuning_rows(current: &ButtonAction) -> Vec<ActionRow> {
    match current {
        ButtonAction::Turbo { .. } => vec![ActionRow::Tuning(Tuning::TurboRate)],
        ButtonAction::Toggle(_) => vec![ActionRow::Tuning(Tuning::ToggleStart)],
        _ => Vec::new(),
    }
}

/// Every row of a slot's action page, in order.
pub fn page_rows(slot: Slot, current: &ButtonAction) -> Vec<ActionRow> {
    let mut rows = action_rows(slot);
    rows.extend(tuning_rows(current));
    rows
}

/// The action after a setting of its wrapper changes: the turbo's next rate, or the toggle's
/// start state flipped.
pub fn tuned(current: &ButtonAction, tuning: Tuning) -> ButtonAction {
    match (tuning, current) {
        (Tuning::TurboRate, ButtonAction::Turbo { action, rate, every_ms }) => {
            let next = TURBO_RATES.iter().position(|r| (r - rate).abs() < 1e-3).map_or(0, |i| (i + 1) % TURBO_RATES.len());
            ButtonAction::Turbo { action: action.clone(), rate: TURBO_RATES[next], every_ms: *every_ms }
        }
        (Tuning::ToggleStart, ButtonAction::Toggle(t)) => ButtonAction::Toggle(Toggled { start_on: !t.start_on, ..t.clone() }),
        _ => current.clone(),
    }
}

/// The action a wrapper leaves a slot with: `current` wrapped, or unwrapped when it already is.
pub fn wrapped(current: &ButtonAction, wrap: Wrap) -> ButtonAction {
    match (wrap, current) {
        (Wrap::Toggle, ButtonAction::Toggle(t)) => *t.action.clone(),
        (Wrap::Toggle, _) => ButtonAction::Toggle(Toggled { action: Box::new(current.clone()), start_on: false }),
        (Wrap::Turbo, ButtonAction::Turbo { action, .. }) => *action.clone(),
        (Wrap::Turbo, _) => ButtonAction::Turbo { action: Box::new(current.clone()), rate: TURBO_RATE, every_ms: 0 },
    }
}

/// The actions offered, after the pad and gesture rows.
type Choice = (&'static str, fn() -> ButtonAction);

const CHOICES: [Choice; 17] = [
    ("Disabled", || ButtonAction::Disabled),
    ("Next profile", || ButtonAction::NextProfile),
    ("On-screen keyboard", || ButtonAction::ToggleOverlay),
    ("Screenshot", || ButtonAction::Screenshot),
    ("Left click", || ButtonAction::Mouse(MouseButton::Left)),
    ("Right click", || ButtonAction::Mouse(MouseButton::Right)),
    ("Middle click", || ButtonAction::Mouse(MouseButton::Middle)),
    ("Back click", || ButtonAction::Mouse(MouseButton::Back)),
    ("Forward click", || ButtonAction::Mouse(MouseButton::Forward)),
    ("Wheel up", || ButtonAction::Wheel(WheelDirection::Up)),
    ("Wheel down", || ButtonAction::Wheel(WheelDirection::Down)),
    ("Wheel left", || ButtonAction::Wheel(WheelDirection::Left)),
    ("Wheel right", || ButtonAction::Wheel(WheelDirection::Right)),
    ("Numpad", || ButtonAction::ToggleNumpad),
    ("Start / stop recording", || ButtonAction::ToggleRecording),
    ("Media controls", || ButtonAction::ToggleMedia),
    ("Force quit (hold)", || ButtonAction::ForceQuit),
];

/// What a picker lists from the game: its layers, macros, menus or info overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picker {
    Layer,
    Macro,
    Menu,
    Info,
    Log,
}

impl Picker {
    pub const ALL: [Picker; 5] = [Picker::Layer, Picker::Macro, Picker::Menu, Picker::Info, Picker::Log];

    /// The row that opens this picker.
    pub fn label(self) -> &'static str {
        match self {
            Picker::Layer => "Hold a layer…",
            Picker::Macro => "Play a macro…",
            Picker::Menu => "Open a menu…",
            Picker::Info => "Show an info overlay…",
            Picker::Log => "Show a log overlay…",
        }
    }

    /// What the picker's items are, for the page title.
    pub fn noun(self) -> &'static str {
        match self {
            Picker::Layer => "layer",
            Picker::Macro => "macro",
            Picker::Menu => "menu",
            Picker::Info => "info overlay",
            Picker::Log => "log overlay",
        }
    }

    /// The action that uses the item called `name`.
    pub fn action(self, name: &str) -> ButtonAction {
        match self {
            Picker::Layer => ButtonAction::Layer(name.into()),
            Picker::Macro => ButtonAction::Macro { name: name.into(), repeat: false },
            Picker::Menu => ButtonAction::OpenMenu(name.into()),
            Picker::Info => ButtonAction::ShowInfo(name.into()),
            Picker::Log => ButtonAction::ShowLog(name.into()),
        }
    }
}

/// The keys the key picker offers, by their codes. Keys outside this list can be set in the
/// pack file.
pub const KEY_PICKS: [&str; 26] = [
    "KEY_ENTER", "KEY_ESC", "KEY_SPACE", "KEY_TAB", "KEY_BACKSPACE", "KEY_LEFTSHIFT", "KEY_LEFTCTRL", "KEY_LEFTALT",
    "KEY_UP", "KEY_DOWN", "KEY_LEFT", "KEY_RIGHT", "KEY_Q", "KEY_E", "KEY_R", "KEY_F",
    "KEY_G", "KEY_1", "KEY_2", "KEY_3", "KEY_4", "KEY_F1", "KEY_F2", "KEY_F5",
    "KEY_F9", "KEY_F11",
];

/// The rows of a slot's action page, in order.
pub fn action_rows(slot: Slot) -> Vec<ActionRow> {
    let mut rows = vec![ActionRow::Pad];
    if matches!(slot, Slot::Button(_)) {
        rows.push(ActionRow::Gestures);
    }
    rows.extend((0..CHOICES.len()).map(ActionRow::Choice));
    rows.push(ActionRow::Keys);
    rows.extend(Picker::ALL.map(ActionRow::Pick));
    rows.push(ActionRow::Wrap(Wrap::Toggle));
    rows.push(ActionRow::Wrap(Wrap::Turbo));
    if matches!(slot, Slot::Gesture(..) | Slot::Layer(..)) {
        rows.push(ActionRow::NotSet);
    }
    rows
}

/// A row's label. The wrapper rows say what they will do to `current`, the slot's action now.
pub fn action_row_label(row: ActionRow, current: &ButtonAction) -> String {
    match row {
        ActionRow::Wrap(Wrap::Toggle) if matches!(current, ButtonAction::Toggle(_)) => "Stop toggling".into(),
        ActionRow::Wrap(Wrap::Toggle) => "Make it a toggle".into(),
        ActionRow::Wrap(Wrap::Turbo) if matches!(current, ButtonAction::Turbo { .. }) => "Stop turbo".into(),
        ActionRow::Wrap(Wrap::Turbo) => format!("Make it turbo ({TURBO_RATE:.0} a second)"),
        ActionRow::Tuning(Tuning::TurboRate) => match current {
            ButtonAction::Turbo { rate, .. } => format!("Rate: {rate:.0} a second"),
            _ => String::new(),
        },
        ActionRow::Tuning(Tuning::ToggleStart) => match current {
            ButtonAction::Toggle(t) => format!("Starts on: {}", if t.start_on { "yes" } else { "no" }),
            _ => String::new(),
        },
        ActionRow::Pad => "Gamepad button…".into(),
        ActionRow::Gestures => "Gestures…".into(),
        ActionRow::Choice(i) => CHOICES[i].0.into(),
        ActionRow::Keys => "Keys…".into(),
        ActionRow::Pick(p) => p.label().into(),
        ActionRow::NotSet => "Not set".into(),
    }
}

/// The action a row picks: a choice, or nothing for "Not set" (`None` for the other rows).
pub fn choice_action(row: ActionRow) -> Option<Option<ButtonAction>> {
    match row {
        ActionRow::Choice(i) => Some(Some(CHOICES[i].1())),
        ActionRow::NotSet => Some(None),
        ActionRow::Pad | ActionRow::Gestures | ActionRow::Keys | ActionRow::Pick(_) | ActionRow::Wrap(_) | ActionRow::Tuning(_) => None,
    }
}

/// The page title for a slot. `layer` is the name of the layer a layer's button belongs to.
pub fn action_title(slot: Slot, layer: Option<&str>) -> String {
    match slot {
        Slot::Button(b) => format!("{} button", b.short_name()),
        Slot::Layer(_, b) => format!("{} · {} button", layer.unwrap_or("Layer"), b.short_name()),
        Slot::Zone(_, i) => format!("Zone {}", i + 1),
        Slot::Gesture(b, kind) => format!("{} {}", b.short_name(), gesture_name(kind).to_lowercase()),
        Slot::Combo(i) => format!("Combo {}", i + 1),
    }
}

/// A key picker row: the key's name on the keyboard, as the key picker shows it.
pub fn key_label(code: &str) -> String {
    crate::keyboard::label(code)
}

pub fn gesture_name(kind: GestureKind) -> &'static str {
    match kind {
        GestureKind::DoubleTap => "Double tap",
        GestureKind::TripleTap => "Triple tap",
        GestureKind::LongPress => "Long press",
    }
}

/// Sets a slot's action. `None` clears a gesture; a button or combo gets Disabled instead.
pub fn set_slot(profile: &mut Profile, slot: Slot, action: Option<ButtonAction>) {
    match slot {
        Slot::Button(b) => {
            profile.buttons.insert(b, action.unwrap_or(ButtonAction::Disabled));
        }
        Slot::Gesture(b, kind) => {
            let empty = {
                let gestures = profile.gestures.entry(b).or_default();
                *gestures.slot(kind) = action;
                gestures.is_empty()
            };
            if empty {
                profile.gestures.remove(&b);
            }
        }
        // A layer's buttons live on the layer, and a zone's action on its stick or trigger; the
        // editor sets those itself.
        Slot::Layer(..) | Slot::Zone(..) => {}
        Slot::Combo(i) => {
            if let Some(combo) = profile.combos.get_mut(i) {
                combo.action = action.unwrap_or(ButtonAction::Disabled);
            }
        }
    }
}

/// Sets a layer's button. `None` removes it, so the layer doesn't bind that button.
pub fn set_layer_slot(bindings: &mut BTreeMap<Button, ButtonAction>, b: Button, action: Option<ButtonAction>) {
    match action {
        Some(action) => {
            bindings.insert(b, action);
        }
        None => {
            bindings.remove(&b);
        }
    }
}

/// The gesture of a button, as a row of its gestures page.
pub fn gesture_row_label(profile: Option<&Profile>, b: Button, kind: GestureKind) -> String {
    let action = profile.and_then(|p| p.gestures.get(&b)).and_then(|g| g.get(kind));
    format!("{}: {}", gesture_name(kind), action.map_or("not set".into(), ButtonAction::summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn a_gesture_slot_is_set_and_cleared_without_leaving_an_empty_entry() {
        let mut profile = Config::default().general.profiles[0].clone();
        let slot = Slot::Gesture(Button::South, GestureKind::DoubleTap);
        set_slot(&mut profile, slot, Some(ButtonAction::Screenshot));
        assert_eq!(profile.gestures[&Button::South].double_tap, Some(ButtonAction::Screenshot));
        set_slot(&mut profile, slot, None);
        assert!(!profile.gestures.contains_key(&Button::South));
    }

    #[test]
    fn a_layer_button_can_be_left_unbound() {
        let mut bindings = BTreeMap::new();
        set_layer_slot(&mut bindings, Button::South, Some(ButtonAction::Screenshot));
        assert_eq!(bindings.get(&Button::South), Some(&ButtonAction::Screenshot));
        set_layer_slot(&mut bindings, Button::South, None);
        assert!(bindings.is_empty());
        assert!(action_rows(Slot::Layer(0, Button::South)).contains(&ActionRow::NotSet));
    }

    #[test]
    fn only_buttons_have_gestures_and_only_gestures_can_be_not_set() {
        assert!(action_rows(Slot::Button(Button::South)).contains(&ActionRow::Gestures));
        assert!(!action_rows(Slot::Button(Button::South)).contains(&ActionRow::NotSet));
        assert!(action_rows(Slot::Gesture(Button::South, GestureKind::LongPress)).contains(&ActionRow::NotSet));
        assert!(!action_rows(Slot::Combo(0)).contains(&ActionRow::Gestures));
    }
}

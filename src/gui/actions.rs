//! The action editor (what an input does, nested in Multi, Toggle and Turbo), summaries of actions, and the on-screen key picker.

use iced::widget::{column, row};

use super::*;

/// The "+ New layer" choice in a layer picker; replaced by a new layer's name when chosen.
pub(super) const NEW_LAYER: &str = "+ New layer";

/// Address of a key field, so the on-screen keyboard can write its result back.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum KeyField {
    /// A `Keys` action at `target`, reached through these `Multi` list indices.
    Action { target: Target, path: Vec<usize> },
    /// One direction (0 up, 1 down, 2 left, 3 right) of a stick in direction-keys mode.
    StickDir { stick: Stick, dir: usize },
}

impl KeyField {
    pub(super) fn root(target: Target) -> Self {
        KeyField::Action { target, path: Vec::new() }
    }

    pub(super) fn child(&self, index: usize) -> Self {
        match self {
            KeyField::Action { target, path } => {
                let mut path = path.clone();
                path.push(index);
                KeyField::Action { target: *target, path }
            }
            other => other.clone(),
        }
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

/// One-line description of an action, for collapsed rows and the controller drawing.
pub(super) fn summarize(action: &ButtonAction) -> String {
    match action {
        ButtonAction::Disabled => "—".into(),
        ButtonAction::Gamepad(b) => format!("Pad {}", short_button(*b)),
        ButtonAction::Keys(keys) if keys.is_empty() => "(no key)".into(),
        ButtonAction::Keys(keys) => keys.iter().map(|k| keyboard::label(k)).collect::<Vec<_>>().join(" + "),
        ButtonAction::Mouse(m) => format!("{m} click"),
        ButtonAction::Wheel(d) => d.to_string(),
        ButtonAction::NextProfile => "Next profile".into(),
        ButtonAction::ToggleOverlay => "On-screen keyboard".into(),
        ButtonAction::ToggleNumpad => "On-screen numpad".into(),
        ButtonAction::OpenMenu(name) => format!("Menu “{name}”"),
        ButtonAction::ShowInfo(name) => format!("Info “{name}”"),
        ButtonAction::Multi(list) if list.is_empty() => "(nothing)".into(),
        ButtonAction::Multi(list) => list.iter().map(summarize).collect::<Vec<_>>().join(" & "),
        ButtonAction::Toggle(t) if t.start_on => format!("Toggle {} (starts on)", summarize(&t.action)),
        ButtonAction::Toggle(t) => format!("Toggle {}", summarize(&t.action)),
        ButtonAction::Turbo { action, rate } => format!("Turbo {} ({rate:.0}/s)", summarize(action)),
        ButtonAction::Macro { name, repeat: true } => format!("Macro “{name}” (repeat)"),
        ButtonAction::Macro { name, .. } => format!("Macro “{name}”"),
        ButtonAction::Layer(name) => format!("Layer “{name}”"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActionKind {
    Disabled,
    Gamepad,
    Keys,
    Mouse,
    Wheel,
    NextProfile,
    Overlay,
    Numpad,
    Toggle,
    Turbo,
    Macro,
    Menu,
    Info,
    Layer,
    Multiple,
}

impl fmt::Display for ActionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ActionKind::Disabled => "Disabled",
            ActionKind::Gamepad => "Gamepad button",
            ActionKind::Keys => "Keyboard",
            ActionKind::Mouse => "Mouse button",
            ActionKind::Wheel => "Scroll wheel",
            ActionKind::NextProfile => "Next profile",
            ActionKind::Overlay => "On-screen keyboard",
            ActionKind::Numpad => "On-screen numpad",
            ActionKind::Toggle => "Toggle (each press switches on / off)…",
            ActionKind::Macro => "Macro…",
            ActionKind::Menu => "Open menu…",
            ActionKind::Info => "Show info overlay…",
            ActionKind::Layer => "Layer…",
            ActionKind::Turbo => "Turbo (repeat while held)…",
            ActionKind::Multiple => "Multiple outputs…",
        })
    }
}

/// Every kind, for a top-level action.
pub(super) const ACTION_KINDS: [ActionKind; 15] = [
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::NextProfile,
    ActionKind::Overlay,
    ActionKind::Numpad,
    ActionKind::Toggle,
    ActionKind::Turbo,
    ActionKind::Macro,
    ActionKind::Menu,
    ActionKind::Info,
    ActionKind::Layer,
    ActionKind::Multiple,
];

/// Radial menu items: everything but opening another menu.
pub(super) const RADIAL_ITEM_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::NextProfile,
    ActionKind::Overlay,
    ActionKind::Numpad,
    ActionKind::Toggle,
    ActionKind::Macro,
    ActionKind::Multiple,
];

/// What a menu item can do: anything a button can, including opening a submenu.
pub(super) const MENU_ITEM_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::NextProfile,
    ActionKind::Overlay,
    ActionKind::Numpad,
    ActionKind::Toggle,
    ActionKind::Macro,
    ActionKind::Menu,
    ActionKind::Multiple,
];

/// What a macro step can press: plain outputs only.
pub(super) const MACRO_STEP_KINDS: &[ActionKind] =
    &[ActionKind::Keys, ActionKind::Mouse, ActionKind::Wheel, ActionKind::Gamepad];

/// Entries of a Multiple list.
pub(super) const MULTI_ENTRY_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::NextProfile,
];

/// What a Toggle can hold down: basic outputs, a turbo (toggleable auto-fire) or several.
pub(super) const TOGGLE_INNER_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::Turbo,
    // A repeating macro toggled on loops until toggled off.
    ActionKind::Macro,
    // Shown until toggled off.
    ActionKind::Info,
    // On until toggled off; how a menu item switches a layer.
    ActionKind::Layer,
    ActionKind::Multiple,
];

/// What a Turbo can repeat.
pub(super) const TURBO_INNER_KINDS: &[ActionKind] = &[
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::Wheel,
    ActionKind::Multiple,
];

pub(super) const DEFAULT_TURBO_RATE: f32 = 10.0;

/// Callback that turns an edited action into a message; lets editors nest inside `Multi`.
pub(super) type OnAction<'a> = Rc<dyn Fn(ButtonAction) -> Message + 'a>;

pub(super) fn set_action<'a>(target: Target) -> OnAction<'a> {
    Rc::new(move |a| Message::SetAction(target, a))
}

/// Editor for one action. `nested` editors (entries of a Multiple list) can't be Multiple.
#[expect(clippy::too_many_arguments, reason = "predates the size lints")]
pub(super) fn action_editor<'a>(
    action: &'a ButtonAction,
    default_button: Button,
    kinds: &'static [ActionKind],
    on_change: OnAction<'a>,
    field: KeyField,
    names: &Names,
) -> Element<'a, Message> {
    let kind = action_kind(action);
    // Shared items can't use layers, so they aren't offered there.
    let kinds: Vec<ActionKind> = kinds.iter().copied().filter(|k| *k != ActionKind::Layer || names.layers_allowed).collect();
    let kind_picker = {
        let on_change = on_change.clone();
        let (current, names) = (action.clone(), names.clone());
        dropdown(kinds, Some(kind), move |k| on_change(new_action(k, default_button, &current, &names))).width(170)
    };
    let value = action_value(action, default_button, on_change, field, names);
    row![kind_picker, value].spacing(8).align_y(Alignment::Start).into()
}

pub(super) fn action_kind(action: &ButtonAction) -> ActionKind {
    match action {
        ButtonAction::Disabled => ActionKind::Disabled,
        ButtonAction::Gamepad(_) => ActionKind::Gamepad,
        ButtonAction::Keys(_) => ActionKind::Keys,
        ButtonAction::Mouse(_) => ActionKind::Mouse,
        ButtonAction::Wheel(_) => ActionKind::Wheel,
        ButtonAction::NextProfile => ActionKind::NextProfile,
        ButtonAction::ToggleOverlay => ActionKind::Overlay,
        ButtonAction::ToggleNumpad => ActionKind::Numpad,
        ButtonAction::Multi(_) => ActionKind::Multiple,
        ButtonAction::Toggle(_) => ActionKind::Toggle,
        ButtonAction::Turbo { .. } => ActionKind::Turbo,
        ButtonAction::Macro { .. } => ActionKind::Macro,
        ButtonAction::OpenMenu(_) => ActionKind::Menu,
        ButtonAction::ShowInfo(_) => ActionKind::Info,
        ButtonAction::Layer(_) => ActionKind::Layer,
    }
}

/// A fresh action of kind `k`, replacing `current`.
pub(super) fn new_action(k: ActionKind, default_button: Button, current: &ButtonAction, names: &Names) -> ButtonAction {
    // When wrapping in Toggle/Turbo, keep a simple existing action as the thing wrapped.
    let wrappable = match current {
        ButtonAction::Gamepad(_) | ButtonAction::Keys(_) | ButtonAction::Mouse(_) | ButtonAction::Wheel(_) => {
            Some(current.clone())
        }
        _ => None,
    };
    match k {
        ActionKind::Disabled => ButtonAction::Disabled,
        ActionKind::Gamepad => ButtonAction::Gamepad(default_button),
        ActionKind::Keys => ButtonAction::Keys(Vec::new()),
        ActionKind::Mouse => ButtonAction::Mouse(MouseButton::Left),
        ActionKind::Wheel => ButtonAction::Wheel(WheelDirection::Up),
        ActionKind::NextProfile => ButtonAction::NextProfile,
        ActionKind::Overlay => ButtonAction::ToggleOverlay,
        ActionKind::Numpad => ButtonAction::ToggleNumpad,
        // Keep what was there as the first entry.
        ActionKind::Multiple => ButtonAction::Multi(wrappable.into_iter().collect()),
        ActionKind::Toggle => ButtonAction::toggle(wrappable.unwrap_or(ButtonAction::Keys(Vec::new()))),
        ActionKind::Turbo => ButtonAction::Turbo {
            action: Box::new(wrappable.unwrap_or(ButtonAction::Mouse(MouseButton::Left))),
            rate: DEFAULT_TURBO_RATE,
        },
        ActionKind::Macro => ButtonAction::Macro { name: names.macros.first().cloned().unwrap_or_default(), repeat: false },
        ActionKind::Menu => ButtonAction::OpenMenu(names.menus.first().cloned().unwrap_or_default()),
        ActionKind::Info => ButtonAction::ShowInfo(names.infos.first().cloned().unwrap_or_default()),
        // With no layers yet, picking the kind makes one.
        ActionKind::Layer => ButtonAction::Layer(names.layers.first().cloned().unwrap_or_else(|| NEW_LAYER.into())),
    }
}

/// The settings to the right of an action's kind picker.
#[expect(clippy::too_many_lines, reason = "predates the size lints")]
pub(super) fn action_value<'a>(
    action: &'a ButtonAction,
    default_button: Button,
    on_change: OnAction<'a>,
    field: KeyField,
    names: &Names,
) -> Element<'a, Message> {
    match action {
        ButtonAction::Gamepad(b) => dropdown(Button::EVERY, Some(*b), move |b| {
            on_change(ButtonAction::Gamepad(b))
        })
        .width(220)
        .into(),
        ButtonAction::Mouse(m) => dropdown(MouseButton::ALL, Some(*m), move |m| {
            on_change(ButtonAction::Mouse(m))
        })
        .width(220)
        .into(),
        ButtonAction::Wheel(d) => dropdown(WheelDirection::ALL, Some(*d), move |d| {
            on_change(ButtonAction::Wheel(d))
        })
        .width(220)
        .into(),
        ButtonAction::Keys(keys) => key_input(
            &keys_to_text(keys),
            "e.g. LEFTCTRL+C",
            move |s| on_change(ButtonAction::Keys(text_to_keys(&s))),
            Message::OpenKeyPicker(field, keys.clone(), false),
        ),
        ButtonAction::Multi(list) => multi_editor(list, default_button, on_change, field, names),
        ButtonAction::Toggle(Toggled { action: inner, start_on }) => {
            let start_on = *start_on;
            let parent = on_change.clone();
            let wrap: OnAction<'a> =
                Rc::new(move |a| parent(ButtonAction::Toggle(Toggled { action: Box::new(a), start_on })));
            let starting = {
                let inner = inner.clone();
                move |on: bool| on_change(ButtonAction::Toggle(Toggled { action: inner.clone(), start_on: on }))
            };
            column![
                row![
                    text("Each press turns this on or off:").size(12).color(MUTED_COLOR),
                    space::horizontal(),
                    tooltip(
                        checkbox(start_on).label("On when the game starts").size(14).text_size(12).on_toggle(starting),
                        container(
                            text(
                                "Switched on by itself the first time the game is focused after it starts, \
                                 e.g. a controls overlay that stays up until this is pressed."
                            )
                            .size(13)
                        )
                        .padding(8)
                        .max_width(320.0)
                        .style(style::tooltip),
                        tooltip::Position::Top,
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                action_editor(inner, default_button, TOGGLE_INNER_KINDS, wrap, field.child(0), names),
            ]
            .spacing(4)
            .into()
        }
        ButtonAction::Turbo { action: inner, rate } => {
            let rate = *rate;
            let parent = on_change.clone();
            let wrap: OnAction<'a> = Rc::new(move |a| parent(ButtonAction::Turbo { action: Box::new(a), rate }));
            let set_rate = {
                let inner = inner.clone();
                move |r: f32| on_change(ButtonAction::Turbo { action: inner.clone(), rate: r })
            };
            column![
                text("Repeats while held:").size(12).color(MUTED_COLOR),
                action_editor(inner, default_button, TURBO_INNER_KINDS, wrap, field.child(0), names),
                row![
                    slider(2.0..=30.0, rate, set_rate).step(1.0_f32).width(200),
                    text(format!("{rate:.0} presses/s")).size(13),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ]
            .spacing(4)
            .into()
        }
        ButtonAction::Macro { .. } if names.macros.is_empty() => {
            text("No macros yet. Create one on the Macros tab.").size(12).color(MUTED_COLOR).into()
        }
        ButtonAction::Macro { name, repeat } => {
            let repeat = *repeat;
            let pick = {
                let on_change = on_change.clone();
                dropdown(names.macros.clone(), Some(name.clone()), move |n| on_change(ButtonAction::Macro { name: n, repeat }))
                    .width(200)
            };
            let name = name.clone();
            let missing = !names.macros.contains(&name);
            let mut line = row![
                pick,
                checkbox(repeat)
                    .label("Repeat while held")
                    .on_toggle(move |r| on_change(ButtonAction::Macro { name: name.clone(), repeat: r })),
            ]
            .spacing(12)
            .align_y(Alignment::Center);
            if missing {
                line = line.push(text("missing macro").size(12).color(ERROR_COLOR));
            }
            line.into()
        }
        ButtonAction::Disabled | ButtonAction::NextProfile => space().into(),
        ButtonAction::ToggleOverlay | ButtonAction::ToggleNumpad => {
            text("Hold B on the controller to close it.").size(12).color(MUTED_COLOR).into()
        }
        ButtonAction::ShowInfo(_) if names.infos.is_empty() => {
            text("No info overlays to show. Create one on the Info overlays tab (one that isn't set to always show).")
                .size(12)
                .color(MUTED_COLOR)
                .into()
        }
        ButtonAction::ShowInfo(name) => {
            let mut line = row![dropdown(names.infos.clone(), Some(name.clone()), move |n| on_change(ButtonAction::ShowInfo(n))).width(200)]
                .spacing(8)
                .align_y(Alignment::Center);
            if names.always_infos.contains(name) {
                line = line.push(text("always on screen, so it can't be triggered").size(12).color(ERROR_COLOR));
            } else if !names.infos.contains(name) {
                line = line.push(text("missing info overlay").size(12).color(ERROR_COLOR));
            }
            line.into()
        }
        ButtonAction::OpenMenu(_) if names.menus.is_empty() => {
            text("No menus to open. Create one on the Menus tab.").size(12).color(MUTED_COLOR).into()
        }
        ButtonAction::Layer(name) => {
            let mut options = names.layers.clone();
            options.push(NEW_LAYER.to_string());
            let mut line = row![dropdown(options, Some(name.clone()), move |n| on_change(ButtonAction::Layer(n))).width(200)]
                .spacing(8)
                .align_y(Alignment::Center);
            if !names.layers.contains(name) {
                line = line.push(text("missing layer").size(12).color(ERROR_COLOR));
            }
            line.into()
        }
        ButtonAction::OpenMenu(name) => {
            let mut line = row![dropdown(names.menus.clone(), Some(name.clone()), move |n| on_change(ButtonAction::OpenMenu(n))).width(200)]
                .spacing(8)
                .align_y(Alignment::Center);
            if !names.menus.contains(name) {
                line = line.push(text("missing menu").size(12).color(ERROR_COLOR));
            }
            line.into()
        }
    }
}

/// List of simultaneous actions, each with its own editor and a remove button.
#[expect(clippy::needless_pass_by_value, reason = "editors take their callbacks by value")]
pub(super) fn multi_editor<'a>(
    list: &'a [ButtonAction],
    default_button: Button,
    on_change: OnAction<'a>,
    field: KeyField,
    names: &Names,
) -> Element<'a, Message> {
    let with = |f: &dyn Fn(&mut Vec<ButtonAction>)| {
        let mut v = list.to_vec();
        f(&mut v);
        on_change(ButtonAction::Multi(v))
    };
    let mut col = column![].spacing(6);
    for (i, sub) in list.iter().enumerate() {
        let parent = on_change.clone();
        let entry: OnAction<'a> = Rc::new(move |a| {
            let mut v = list.to_vec();
            v[i] = a;
            parent(ButtonAction::Multi(v))
        });
        col = col.push(
            row![
                action_editor(sub, default_button, MULTI_ENTRY_KINDS, entry, field.child(i), names),
                button(text("✕").size(13)).style(style::secondary).on_press(with(&|v| {
                    v.remove(i);
                })),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    col.push(
        button(text("+ Add output").size(13))
            .style(style::secondary)
            .on_press(with(&|v| v.push(ButtonAction::Keys(Vec::new())))),
    )
    .into()
}

/// Text input for evdev key names (without the `KEY_` prefix) that turns red when invalid.
/// Also offers a button that opens the on-screen keyboard (`open_picker`).
pub(super) fn key_input<'a>(
    value: &str,
    placeholder: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    open_picker: Message,
) -> Element<'a, Message> {
    let valid = value.is_empty()
        || value.split('+').all(|k| KeyCode::from_str(&format!("KEY_{k}")).is_ok());
    let input = field(placeholder, value).on_input(on_input).width(180);
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

pub(super) fn keys_to_text(keys: &[String]) -> String {
    keys.iter().map(|k| short_key(k)).collect::<Vec<_>>().join("+")
}

pub(super) fn text_to_keys(s: &str) -> Vec<String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();
    if s.is_empty() {
        return Vec::new();
    }
    s.split('+').map(|k| format!("KEY_{k}")).collect()
}

pub(super) fn single_key(s: &str) -> String {
    text_to_keys(s).into_iter().next().unwrap_or_default()
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
            KeyField::StickDir { stick, dir } => {
                if let StickAction::Keys { up, down, left, right } = &mut p.stick_mut(stick).action {
                    let slot = match dir {
                        0 => up,
                        1 => down,
                        2 => left,
                        _ => right,
                    };
                    *slot = keys.into_iter().next().unwrap_or_default();
                }
            }
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
                    // Handled above, outside any profile.
                    Target::MacroStep(..) | Target::MenuItem(..) => None,
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
    fn single_picker_sets_stick_direction() {
        let mut app = app();
        app.config.general.profiles[0].left_stick.action = wasd();
        let _ = app.update(Message::OpenKeyPicker(KeyField::StickDir { stick: Stick::Left, dir: 2 }, vec![], true));
        // Single mode closes itself by scheduling PickerClose; run it like the runtime would.
        let _ = app.update(Message::PickerKey("KEY_LEFT"));
        let _ = app.update(Message::PickerClose { apply: true });
        let StickAction::Keys { left, up, .. } = &app.config.general.profiles[0].left_stick.action else { panic!() };
        assert_eq!((left.as_str(), up.as_str()), ("KEY_LEFT", "KEY_W"));
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
    fn summaries_read_like_the_mapping() {
        let keys = |k: &[&str]| ButtonAction::Keys(k.iter().map(std::string::ToString::to_string).collect());
        assert_eq!(summarize(&keys(&["KEY_LEFTCTRL", "KEY_C"])), "Left Ctrl + C");
        assert_eq!(summarize(&ButtonAction::Disabled), "—");
        assert_eq!(
            summarize(&ButtonAction::toggle(ButtonAction::Turbo {
                action: Box::new(ButtonAction::Mouse(MouseButton::Left)),
                rate: 12.0
            })),
            "Toggle Turbo Left click (12/s)"
        );
        assert_eq!(summarize(&ButtonAction::Macro { name: "QCF".into(), repeat: true }), "Macro “QCF” (repeat)");
        assert_eq!(summarize(&ButtonAction::Gamepad(Button::RightStickRight)), "Pad RS→");
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
        for s in ["", "LEFTCTRL+C", "LEFTCTRL+", "A"] {
            assert_eq!(keys_to_text(&text_to_keys(s)), s);
        }
        assert_eq!(text_to_keys("leftctrl + c"), vec!["KEY_LEFTCTRL", "KEY_C"]);
    }
}

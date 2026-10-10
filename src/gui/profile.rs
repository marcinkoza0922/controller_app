//! The profile editor (buttons, combos, gyro), also used for layers, and the controller drawing.

use iced::widget::{column, row};

use super::*;
use crate::info::{Glyphs, button_name};
use super::guide_tab::{self, GuideView};
use crate::pad_widget::controller_drawing;

/// Sections of the profile editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProfileTab {
    Buttons,
    Sticks,
    Combos,
    Gyro,
    Guide,
}

/// What the free view functions need to know about the app beyond the profile itself.
pub(super) struct Ui<'a> {
    pub(super) names: &'a Names,
    pub(super) expanded: &'a HashSet<Target>,
    /// Row picked by "Find by pressing", highlighted and open.
    pub(super) found: Option<Button>,
    pub(super) analog_triggers: bool,
    pub(super) any_gyro: bool,
    pub(super) any_paddles: bool,
    /// Live stick positions (left, right) for previews, when a controller is connected.
    pub(super) live_sticks: Option<[(f32, f32); 2]>,
    /// Whose button glyphs to draw (see `App::glyph_family`).
    pub(super) family: PadFamily,
    /// Whose button names the controller in use has (see `App::shown_pad`).
    pub(super) in_use: PadFamily,
    /// How the Nintendo layout shows on this profile's face buttons (see `App::swap_for`).
    pub(super) swap: Swap,
    /// Set while editing a layer: the profile shown is the layer over `base`.
    pub(super) layer: Option<LayerMarks<'a>>,
}

/// A layer being edited, over the profile it's compared with.
#[derive(Clone, Copy)]
pub(super) struct LayerMarks<'a> {
    pub(super) layer: &'a crate::config::Layer,
    pub(super) base: &'a Profile,
}

/// Something a layer overrides as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LayerPart {
    /// A button's action and gestures.
    Button(Button),
    Stick(Stick),
    /// A trigger's action and zones.
    Trigger(Trigger),
    Gyro,
}

impl LayerMarks<'_> {
    pub(super) fn overrides(&self, part: LayerPart) -> bool {
        let l = self.layer;
        match part {
            LayerPart::Button(b) => l.buttons.contains_key(&b) || l.gestures.contains_key(&b),
            LayerPart::Stick(Stick::Left) => l.left_stick.is_some(),
            LayerPart::Stick(Stick::Right) => l.right_stick.is_some(),
            LayerPart::Trigger(Trigger::Left) => l.left_trigger.is_some(),
            LayerPart::Trigger(Trigger::Right) => l.right_trigger.is_some(),
            LayerPart::Gyro => l.gyro.is_some(),
        }
    }
}

/// In a layer, a part it doesn't override reads "Same as Gameplay: …" with an Override
/// button; an overridden one gets its editor and "Back to base". Outside layers, just the
/// editor.
#[expect(clippy::needless_pass_by_value, reason = "labels go into `text`, which needs them owned for 'a")]
pub(super) fn layer_part<'a>(ui: &Ui, part: LayerPart, label: String, summary: String, editor: impl FnOnce() -> Vec<Element<'a, Message>>) -> Vec<Element<'a, Message>> {
    let Some(marks) = ui.layer else { return editor() };
    if marks.overrides(part) {
        let mut rows = vec![
            row![
                text(label).size(13).color(style_accent()),
                space::horizontal(),
                button(text("Back to base").size(13)).style(button::text).on_press(Message::RevertInput(part)),
            ]
            .align_y(Alignment::Center)
            .into(),
        ];
        rows.extend(editor());
        return rows;
    }
    vec![
        row![
            text(label).width(LABEL_WIDTH).color(MUTED_COLOR),
            text(format!("Same as {}: {summary}", marks.base.name)).color(MUTED_COLOR),
            space::horizontal(),
            button(text("Override").size(13)).style(style::secondary).on_press(Message::OverrideInput(part)),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into(),
    ]
}

impl Ui<'_> {
    pub(super) fn is_open(&self, target: Target) -> bool {
        self.expanded.contains(&target) || matches!(target, Target::Button(b) if self.found == Some(b))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Template {
    Gamepad,
    Desktop,
    Action,
    GyroAim,
    FlickStick,
    Strategy,
    Platformer,
    Duplicate,
}

impl Template {
    /// A new profile from this template (Duplicate makes a passthrough one; copy instead).
    pub(super) fn make(self, name: &str) -> Profile {
        match self {
            Template::Gamepad | Template::Duplicate => Profile::passthrough(name),
            Template::Desktop => Profile::desktop(name),
            Template::Action => Profile::pc_action(name),
            Template::GyroAim => Profile::gyro_aim(name),
            Template::FlickStick => Profile::flick_stick(name),
            Template::Strategy => Profile::strategy(name),
            Template::Platformer => Profile::platformer(name),
        }
    }

    /// Offered in the "New from template" list (Duplicate has its own button).
    pub(super) const NEW: [Template; 7] = [
        Template::Gamepad,
        Template::Action,
        Template::GyroAim,
        Template::FlickStick,
        Template::Strategy,
        Template::Platformer,
        Template::Desktop,
    ];

    pub(super) fn base_name(self) -> &'static str {
        match self {
            Template::Gamepad => "Gamepad",
            Template::Desktop => "Desktop",
            Template::Action => "PC Action",
            Template::GyroAim => "Gyro Aim",
            Template::FlickStick => "Flick Stick",
            Template::Strategy => "Strategy",
            Template::Platformer => "Platformer",
            Template::Duplicate => "Copy",
        }
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Template::Gamepad => "Gamepad passthrough",
            Template::Desktop => "Desktop navigation",
            Template::Action => "PC action (WASD + mouse look)",
            Template::GyroAim => "PC action with gyro aiming (needs a gyro)",
            Template::FlickStick => "PC action with a flick stick (needs a gyro)",
            Template::Strategy => "Strategy (mouse pointer + hotkeys)",
            Template::Platformer => "Retro / platformer (arrows + Z/X/C)",
            Template::Duplicate => "Copy of this profile",
        })
    }
}

/// Labels for the controller drawing: every input that doesn't simply pass through. Guide's
/// usual mapping is left out too: the engine ignores it (Guide is built in).
pub(super) fn drawing_labels(p: &Profile) -> Vec<(pad_svg::Spot, String)> {
    let mut labels = Vec::new();
    for b in Button::ALL {
        let usual = if b == Button::Guide { ButtonAction::guide_hold() } else { ButtonAction::Gamepad(b) };
        let gestures = p.gestures(b).is_some();
        if *p.button(b) == usual && !gestures {
            continue;
        }
        let mut label = summarize(p.button(b));
        if gestures {
            label.push_str(" +");
        }
        labels.push((pad_svg::Spot::Button(b), label));
    }
    for t in [Trigger::Left, Trigger::Right] {
        let label = match p.trigger(t) {
            TriggerAction::Gamepad(out) if *out == t => continue,
            TriggerAction::Gamepad(out) => format!("Pad {}", if *out == Trigger::Left { "LT" } else { "RT" }),
            TriggerAction::Disabled => "Disabled".into(),
            TriggerAction::Button { action, .. } => summarize(action),
        };
        labels.push((pad_svg::Spot::Trigger(t), label));
    }
    labels
}

#[expect(clippy::too_many_lines, reason = "predates the size lints")]
pub(super) fn view_profile<'a>(p: &'a Profile, ui: &Ui, tab: ProfileTab, guide: GuideView<'a>) -> Element<'a, Message> {
    let names = ui.names;
    let sections: Vec<Element<'a, Message>> = match tab {
        ProfileTab::Buttons => vec![
            section(
                "Buttons",
                Some(
                    "Click a button's name to edit it. Add a double tap, triple tap or long press \
                     with the \"+\" buttons under its actions. Buttons with gestures act once the gesture \
                     is decided: a quick tap fires after the tap window; held past the tap window, the \
                     button's own action presses \
                     and holds until release (unless a long press is set, which takes over when held)."
                        .into(),
                ),
                button_rows(p, ui),
            ),
            section(
                "Back paddles",
                Some(
                    "The extra buttons on the back of some controllers (Xbox Elite, DualSense Edge, Steam Deck). \
                     Map them like any button."
                        .into(),
                ),
                paddle_rows(p, ui),
            ),
        ],
        ProfileTab::Sticks => {
            let mut sticks = Vec::new();
            for s in [Stick::Left, Stick::Right] {
                let base = ui.layer.map_or(p.stick(s), |m| m.base.stick(s));
                sticks.extend(layer_part(ui, LayerPart::Stick(s), s.to_string(), stick_summary(base), || vec![stick_editor(s, p.stick(s), names, ui.expanded, ui.live_sticks.map(|l| l[usize::from(s == Stick::Right)]))]));
            }
            let mut triggers = Vec::new();
            for t in [Trigger::Left, Trigger::Right] {
                let base = ui.layer.map_or(p, |m| m.base);
                let base = if t == Trigger::Left { &base.left_trigger } else { &base.right_trigger };
                triggers.extend(layer_part(ui, LayerPart::Trigger(t), t.to_string(), trigger_summary(base), || {
                    vec![trigger_editor(t, p.trigger(t), p.zones(Analog::Trigger(t)), ui.analog_triggers, names)]
                }));
            }
            vec![
                section(
                    "Sticks",
                    Some(
                        "A stick's directions (Left Stick Up, …) act as buttons on top of the stick's mode. \
                         They can't have actions of their own, but they can be used in combos \
                         (e.g. LB + Right Stick Right), in macros, and as the output of other buttons. \
                         They press at the stick's \"Directions press at\" threshold. For per-direction \
                         actions, use a button ring."
                            .into(),
                    ),
                    sticks,
                ),
                section("Triggers", None, triggers),
            ]
        }
        ProfileTab::Combos => {
            let mut sections = vec![section(
                if ui.layer.is_some() { "This layer's combos" } else { "Combos" },
                Some(format!(
                    "Buttons pressed together, within {} ms of each other, act as one input. A button that is part \
                     of a combo waits that long before acting alone, unless it's set to Disabled: then it \
                     works only as a modifier, with no time limit.",
                    p.combo_window_ms
                )),
                combo_rows(p, ui),
            )];
            if let Some(marks) = ui.layer {
                sections.push(base_combos(marks));
            }
            sections
        }
        ProfileTab::Guide => guide_tab::sections(p, guide, ui.layer.is_some()),
        ProfileTab::Gyro => {
            let base = ui.layer.map_or(&p.gyro, |m| &m.base.gyro);
            let summary = match base.mode {
                GyroMode::Off => "off".to_string(),
                _ => "on".to_string(),
            };
            let mut sections = vec![section(
                "Gyro",
                Some("Aim by tilting the controller. If the aim drifts, calibrate the gyro from the controller list on the Overview page.".into()),
                layer_part(ui, LayerPart::Gyro, "Gyro".into(), summary, || gyro_rows(&p.gyro, ui.any_gyro)),
            )];
            if ui.layer.is_none() {
                sections.push(section("Controller requirements", None, requirement_rows(p, Feature::Gyro)));
            }
            sections
        }
    };
    column(sections).spacing(16).into()
}

/// The compared profile's combos, each of which a layer can switch off while it's on.
pub(super) fn base_combos<'a>(marks: LayerMarks<'_>) -> Element<'a, Message> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    for combo in &marks.base.combos {
        let key = crate::config::combo_key(&combo.buttons);
        let off = marks.layer.disabled_combos.iter().any(|d| crate::config::combo_key(d) == key);
        let replaced = marks.layer.combos.iter().any(|c| crate::config::combo_key(&c.buttons) == key);
        let name = combo.buttons.iter().map(|b| short_button(*b)).collect::<Vec<_>>().join(" + ");
        let mut line = row![text(name).width(LABEL_WIDTH), text(summarize(&combo.action)).color(MUTED_COLOR), space::horizontal()]
            .spacing(10)
            .align_y(Alignment::Center);
        line = if replaced {
            line.push(text("replaced by this layer's").size(12).color(MUTED_COLOR))
        } else {
            line.push(checkbox(off).label("Off in this layer").on_toggle(move |_| Message::ToggleBaseCombo(key.clone())))
        };
        rows.push(line.into());
    }
    if rows.is_empty() {
        rows.push(text(format!("{} has no combos.", marks.base.name)).size(13).color(MUTED_COLOR).into());
    }
    section(
        "The profile's combos",
        Some("They stay on while the layer is held, unless switched off here or replaced by a layer combo with the same buttons.".into()),
        rows,
    )
}

impl ProfileTab {
    pub(super) const ALL: [ProfileTab; 5] = [ProfileTab::Buttons, ProfileTab::Sticks, ProfileTab::Combos, ProfileTab::Gyro, ProfileTab::Guide];
}

impl fmt::Display for ProfileTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ProfileTab::Buttons => "Buttons",
            ProfileTab::Sticks => "Sticks & triggers",
            ProfileTab::Combos => "Combos",
            ProfileTab::Gyro => "Gyro",
            ProfileTab::Guide => "Guide",
        })
    }
}

/// A button pressed (or stick pushed firmly) in `now` that wasn't in `before`, for
/// "Find by pressing".
pub(super) fn newly_pressed(before: Option<&InputSnapshot>, now: &InputSnapshot) -> Option<Button> {
    let was_down = |b: &Button| before.is_some_and(|s| s.buttons.contains(b));
    if let Some(b) = now.buttons.iter().find(|b| !was_down(b)) {
        return Some(*b);
    }
    const FIRM: f32 = 0.7;
    let pushed = |s: &InputSnapshot, stick: Stick| -> Option<Button> {
        let (x, y) = if stick == Stick::Left { s.left_stick } else { s.right_stick };
        let [up, down, left, right] = Button::stick_directions(stick);
        [(up, -y), (down, y), (left, -x), (right, x)].into_iter().find(|(_, v)| *v > FIRM).map(|(b, _)| b)
    };
    [Stick::Left, Stick::Right]
        .into_iter()
        .find_map(|stick| pushed(now, stick).filter(|b| before.and_then(|s| pushed(s, stick)) != Some(*b)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GyroModeKind {
    Off,
    Mouse,
    Stick,
    Steering,
}

impl fmt::Display for GyroModeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            GyroModeKind::Off => "Off",
            GyroModeKind::Mouse => "Mouse (gyro aiming)",
            GyroModeKind::Stick => "Gamepad stick (gyro aiming)",
            GyroModeKind::Steering => "Steering (tilt to steer)",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActivationKind {
    Always,
    WhileHeld,
    UnlessHeld,
    Toggle,
}

impl fmt::Display for ActivationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ActivationKind::Always => "Always on",
            ActivationKind::WhileHeld => "Only while held",
            ActivationKind::UnlessHeld => "Off while held (clutch)",
            ActivationKind::Toggle => "Toggle with",
        })
    }
}

/// A recenter choice for a pick list, where `None` means no recenter input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RecenterChoice(pub(super) Option<GyroInput>);

impl fmt::Display for RecenterChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(input) => write!(f, "{input}"),
            None => f.write_str("(none)"),
        }
    }
}

/// Where the profile's author says what it can't be played without. Never worked out from
/// the settings: a profile that adds gyro aiming to a scheme that works without it leaves
/// the box unticked.
fn requirement_rows(p: &Profile, f: Feature) -> Vec<Element<'_, Message>> {
    let has = p.requires.contains(&f);
    let toggled = move |on: bool| {
        let mut all: Vec<Feature> = p.requires.iter().copied().filter(|x| *x != f).collect();
        all.extend(on.then_some(f));
        Message::SetRequires(all)
    };
    let hint = match f {
        Feature::Gyro => {
            "You decide this; it isn't detected. Tick it when the profile depends on the feature (for example a flick stick \
             setup that turns vertically with gyro). Leave it unticked when it only adds to a scheme that works without. \
             Players whose controller lacks a ticked feature aren't offered the profile, and Guide skips it."
        }
        Feature::BackPaddles => {
            "You decide this; it isn't detected. Tick it when the profile needs a paddle to be played (a jump that's only on \
             a paddle, say). Leave it unticked when paddles only add extra inputs to a scheme that works without them. \
             Players whose controller has no paddles aren't offered the profile, and Guide skips it."
        }
    };
    vec![
        checkbox(has).label(format!("Can't be played without {}", f.label())).on_toggle(toggled).into(),
        text(hint).size(12).color(MUTED_COLOR).into(),
    ]
}

/// The back paddles' mappings, with the box saying whether the profile needs them. Layers
/// can override them like any button, so the box is left out there.
pub(super) fn paddle_rows<'a>(p: &'a Profile, ui: &Ui) -> Vec<Element<'a, Message>> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    if ui.layer.is_none() {
        if !ui.any_paddles {
            rows.push(
                text("None of your managed controllers have back paddles. These mappings take effect once one is connected.")
                    .size(13)
                    .color(MUTED_COLOR)
                    .into(),
            );
        }
        rows.extend(requirement_rows(p, Feature::BackPaddles));
    }
    for b in Button::PADDLES {
        rows.extend(button_row(p, b, ui));
    }
    rows
}

#[expect(clippy::too_many_lines, reason = "predates the size lints")]
pub(super) fn gyro_rows(cfg: &GyroConfig, any_gyro: bool) -> Vec<Element<'_, Message>> {
    let with = move |f: &dyn Fn(&mut GyroConfig)| {
        let mut c = cfg.clone();
        f(&mut c);
        Message::SetGyro(c)
    };
    // Without a gyro controller this is a status worth seeing, not just help.
    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    if !any_gyro {
        rows.push(
            text(
                "None of your managed controllers have a gyro (PlayStation and Switch controllers do). \
                 These settings take effect once one is connected.",
            )
            .size(13)
            .color(MUTED_COLOR)
            .into(),
        );
    }

    let kind = match cfg.mode {
        GyroMode::Off => GyroModeKind::Off,
        GyroMode::Mouse { .. } => GyroModeKind::Mouse,
        GyroMode::Stick { .. } => GyroModeKind::Stick,
        GyroMode::Steering { .. } => GyroModeKind::Steering,
    };
    let kinds = [GyroModeKind::Off, GyroModeKind::Mouse, GyroModeKind::Stick, GyroModeKind::Steering];
    rows.push(labeled(
        "Gyro",
        dropdown(kinds, Some(kind), move |k| {
            let mode = match k {
                GyroModeKind::Off => GyroMode::Off,
                GyroModeKind::Mouse => GyroMode::Mouse { sensitivity: 15.0 },
                GyroModeKind::Stick => GyroMode::Stick { stick: Stick::Right, full_rate: 360.0, anti_deadzone: 0.15 },
                GyroModeKind::Steering => GyroMode::Steering { stick: Stick::Left, max_angle: 45.0 },
            };
            with(&|c| c.mode = mode.clone())
        })
        .width(260)
        .into(),
    ));

    match cfg.mode {
        GyroMode::Off => return rows,
        GyroMode::Mouse { sensitivity } => {
            rows.push(value_slider("    Sensitivity", 2.0..=60.0, sensitivity, 1.0, "px per degree", move |v| {
                with(&|c| c.mode = GyroMode::Mouse { sensitivity: v })
            }));
        }
        GyroMode::Stick { stick, full_rate, anti_deadzone } => {
            rows.push(labeled(
                "    Output stick",
                dropdown([Stick::Left, Stick::Right], Some(stick), move |s| {
                    with(&|c| c.mode = GyroMode::Stick { stick: s, full_rate, anti_deadzone })
                })
                .width(170)
                .into(),
            ));
            rows.push(value_slider("    Full tilt at", 60.0..=720.0, full_rate, 10.0, "°/s turning", move |v| {
                with(&|c| c.mode = GyroMode::Stick { stick, full_rate: v, anti_deadzone })
            }));
            rows.push(value_slider("    Anti-deadzone", 0.0..=0.4, anti_deadzone, 0.01, "(beats the game's stick deadzone)", move |v| {
                with(&|c| c.mode = GyroMode::Stick { stick, full_rate, anti_deadzone: v })
            }));
        }
        GyroMode::Steering { stick, max_angle } => {
            rows.push(labeled(
                "    Output stick",
                dropdown([Stick::Left, Stick::Right], Some(stick), move |s| {
                    with(&|c| c.mode = GyroMode::Steering { stick: s, max_angle })
                })
                .width(170)
                .into(),
            ));
            rows.push(value_slider("    Full lock at", 15.0..=90.0, max_angle, 1.0, "° tilt", move |v| {
                with(&|c| c.mode = GyroMode::Steering { stick, max_angle: v })
            }));
        }
    }

    let steering = matches!(cfg.mode, GyroMode::Steering { .. });
    if !steering {
        rows.push(labeled(
            "    Horizontal from",
            dropdown(GyroHorizontal::ALL, Some(cfg.horizontal), move |h| with(&|c| c.horizontal = h))
                .width(170)
                .into(),
        ));
    }
    let mut inverts = row![checkbox(cfg.invert_x).label("Invert horizontal").on_toggle(move |v| with(&|c| c.invert_x = v))]
        .spacing(16);
    if !steering {
        inverts = inverts.push(checkbox(cfg.invert_y).label("Invert vertical").on_toggle(move |v| with(&|c| c.invert_y = v)));
    }
    rows.push(labeled("", inverts.into()));

    let (activation_kind, activation_input) = match cfg.activation {
        GyroActivation::Always => (ActivationKind::Always, None),
        GyroActivation::WhileHeld(i) => (ActivationKind::WhileHeld, Some(i)),
        GyroActivation::UnlessHeld(i) => (ActivationKind::UnlessHeld, Some(i)),
        GyroActivation::Toggle(i) => (ActivationKind::Toggle, Some(i)),
    };
    let activation = move |kind: ActivationKind, input: GyroInput| match kind {
        ActivationKind::Always => GyroActivation::Always,
        ActivationKind::WhileHeld => GyroActivation::WhileHeld(input),
        ActivationKind::UnlessHeld => GyroActivation::UnlessHeld(input),
        ActivationKind::Toggle => GyroActivation::Toggle(input),
    };
    let current_input = activation_input.unwrap_or(GyroInput::LeftTrigger);
    let activation_kinds = [ActivationKind::Always, ActivationKind::WhileHeld, ActivationKind::UnlessHeld, ActivationKind::Toggle];
    let mut active_row = row![dropdown(activation_kinds, Some(activation_kind), move |k| {
        with(&|c| c.activation = activation(k, current_input))
    })
    .width(220)]
    .spacing(8)
    .align_y(Alignment::Center);
    if activation_input.is_some() {
        active_row = active_row.push(
            dropdown(GyroInput::all(), activation_input, move |i| with(&|c| c.activation = activation(activation_kind, i)))
                .width(200),
        );
    }
    rows.push(labeled("    Active", active_row.into()));

    let mut recenter_choices = vec![RecenterChoice(None)];
    recenter_choices.extend(GyroInput::all().into_iter().map(|i| RecenterChoice(Some(i))));
    rows.push(labeled(
        "    Recenter with",
        row![
            dropdown(recenter_choices, Some(RecenterChoice(cfg.recenter)), move |r: RecenterChoice| {
                with(&|c| c.recenter = r.0)
            })
            .width(200),
            text(if steering { "sets the current tilt as straight" } else { "clears leftover motion" })
                .size(12)
                .color(MUTED_COLOR),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
    ));
    if !steering {
        rows.push(value_slider("    Ignore jitter below", 0.0..=5.0, cfg.noise_threshold, 0.1, "°/s", move |v| {
            with(&|c| c.noise_threshold = v)
        }));
    }
    rows
}

pub(super) fn button_rows<'a>(p: &'a Profile, ui: &Ui) -> Vec<Element<'a, Message>> {
    // Layers use their profile's timings.
    let mut rows: Vec<Element<'a, Message>> = if ui.layer.is_some() {
        Vec::new()
    } else {
        vec![
            value_slider("Tap window", 100.0..=600.0, p.tap_window_ms as f32, 10.0, "ms", Message::SetTapWindow),
            value_slider("Long press after", 200.0..=1500.0, p.long_press_ms as f32, 50.0, "ms", Message::SetLongPress),
        ]
    };
    // Guide is built in: the Guide tab says what it does, so it has no row here.
    for b in Button::ALL.into_iter().filter(|b| *b != Button::Guide) {
        rows.extend(button_row(p, b, ui));
    }
    rows
}

/// The label column of a collapsible row: click to open or close it.
pub(super) fn row_toggle<'a>(label: &str, target: Target, open: bool, problem: bool) -> Element<'a, Message> {
    let chevron = if open { "▾" } else { "▸" };
    let label = text(format!("{chevron} {label}"));
    let label = if problem { label.color(ERROR_COLOR) } else { label };
    button(label)
        .style(button::text)
        .padding([4, 0])
        .width(LABEL_WIDTH)
        .on_press(Message::ToggleExpanded(target))
        .into()
}

/// One button: collapsed to a summary ("A ▸ E · double tap: Q"), or open with its action
/// editor, gesture rows and a "+ Double tap" style button for each gesture not yet set.
#[expect(clippy::too_many_lines, reason = "predates the size lints")]
pub(super) fn button_row<'a>(p: &'a Profile, b: Button, ui: &Ui) -> Vec<Element<'a, Message>> {
    if let Some(marks) = ui.layer
        && !marks.overrides(LayerPart::Button(b))
    {
        let mut summary = summarize(marks.base.button(b));
        if marks.base.gestures(b).is_some() {
            summary.push_str(" +");
        }
        return layer_part(ui, LayerPart::Button(b), button_name(b, ui.in_use), summary, Vec::new);
    }
    let target = Target::Button(b);
    let open = ui.is_open(target);
    let gestures = p.gestures.get(&b);
    let set_gestures: Vec<(GestureKind, &'a ButtonAction)> =
        GestureKind::ALL.into_iter().filter_map(|k| gestures.and_then(|g| g.get(k)).map(|a| (k, a))).collect();
    let problem = std::iter::once(p.button(b))
        .chain(set_gestures.iter().map(|(_, a)| *a))
        .find_map(|a| action_problem(a, ui.names));
    let mut rows: Vec<Element<'a, Message>> = Vec::new();

    if !open {
        let disabled = matches!(p.button(b), ButtonAction::Disabled) && set_gestures.is_empty();
        let passthrough = *p.button(b) == ButtonAction::Gamepad(b) && set_gestures.is_empty();
        let summary = piece_line(p.button(b).pieces(), ui.family, ui.swap, disabled || passthrough);
        let mut line = row![row_toggle(&button_name(b, ui.in_use), target, false, problem.is_some()), summary]
            .spacing(10)
            .align_y(Alignment::Center);
        // Each gesture is its own segment, set off by a muted bar and its name in muted text.
        for (k, a) in &set_gestures {
            line = line
                .push(text("│").color(MUTED_COLOR))
                .push(text(format!("{k}:")).color(MUTED_COLOR))
                .push(piece_line(a.pieces(), ui.family, ui.swap, false));
        }
        if let Some(problem) = problem {
            line = line.push(space::horizontal()).push(text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR));
        }
        if ui.layer.is_some() {
            line = line.push(space::horizontal()).push(
                button(text("Back to base").size(13)).style(button::text).on_press(Message::RevertInput(LayerPart::Button(b))),
            );
        }
        rows.push(line.into());
        return rows;
    }

    let missing: Vec<GestureKind> = GestureKind::ALL
        .into_iter()
        .filter(|k| gestures.and_then(|g| g.get(*k)).is_none())
        .collect();
    // The name gets a line of its own so the open editor reads as sitting under it.
    let mut header = row![row_toggle(&button_name(b, ui.in_use), target, true, problem.is_some()), space::horizontal()]
        .spacing(10)
        .align_y(Alignment::Center);
    if ui.layer.is_some() {
        header = header.push(
            button(text("Back to base").size(13)).style(button::text).on_press(Message::RevertInput(LayerPart::Button(b))),
        );
    }
    rows.push(labeled(
        "    Press",
        fill_x(action_editor(p.button(b), b, &ACTION_KINDS, set_action(target), KeyField::root(target), ui.names)),
    ));

    for (kind, action) in set_gestures {
        rows.push(rule::horizontal(1).into());
        // Laid out like the Press row, with a ✕ at the end as other removable rows have.
        let target = Target::Gesture(b, kind);
        rows.push(labeled(
            format!("    {kind}"),
            row![
                fill_x(action_editor(action, b, &ACTION_KINDS, set_action(target), KeyField::root(target), ui.names)),
                button(text("✕").size(13)).style(style::secondary).on_press(Message::RemoveGesture(b, kind)),
            ]
            .spacing(8)
            .align_y(Alignment::Start)
            .into(),
        ));
    }
    // Adding a gesture sits under the rows it extends, one click per kind.
    if !missing.is_empty() {
        rows.push(rule::horizontal(1).into());
        let adds = missing.into_iter().map(|k| {
            button(text(format!("+ {k}")).size(13))
                .style(style::secondary)
                .padding([3, 10])
                .on_press(Message::AddGesture(b, k))
                .into()
        });
        rows.push(labeled("", row(adds).spacing(6).into()));
    }
    if let Some(problem) = problem {
        rows.push(labeled("", text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR).into()));
    }
    // The details sit in a box under the name, like a combo's, picked out in the accent color
    // when the button was just found. The name stays outside so it doesn't move on opening.
    let found = ui.found == Some(b);
    let details = container(column(rows).spacing(8)).padding(10).width(Length::Fill).style(move |t| {
        let mut s = style::inset(t);
        if found {
            s.border.color = style_accent();
            s.border.width = 2.0;
        }
        s
    });
    vec![column![header, details].spacing(4).into()]
}

#[expect(clippy::too_many_lines, reason = "predates the size lints")]
pub(super) fn combo_rows<'a>(p: &'a Profile, ui: &Ui) -> Vec<Element<'a, Message>> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    if ui.layer.is_none() {
        rows.push(value_slider("Combo window", 20.0..=300.0, p.combo_window_ms as f32, 5.0, "ms", Message::SetComboWindow));
    }
    for (i, combo) in p.combos.iter().enumerate() {
        let target = Target::Combo(i);
        let open = ui.is_open(target);
        let name = combo.buttons.iter().map(|b| short_button(*b)).collect::<Vec<_>>().join(" + ");
        let name = if name.is_empty() { "(no buttons)".to_string() } else { name };
        let problem = if combo.buttons.len() < 2 {
            Some("needs at least two buttons".to_string())
        } else {
            action_problem(&combo.action, ui.names)
        };
        let remove = button(text("Remove combo").size(13)).style(button::danger).on_press(Message::RemoveCombo(i));
        if !open {
            let mut line = row![row_toggle(&name, target, false, problem.is_some()), text(summarize(&combo.action)), space::horizontal()]
                .spacing(10)
                .align_y(Alignment::Center);
            if let Some(problem) = problem {
                line = line.push(text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR));
            }
            rows.push(line.push(remove).into());
            continue;
        }
        let mut members = row![].spacing(6).align_y(Alignment::Center);
        for (n, &b) in combo.buttons.iter().enumerate() {
            if n > 0 {
                members = members.push(text("+"));
            }
            members = members.push(
                button(text(format!("{} ✕", short_button(b))).size(13))
                    .style(style::secondary)
                    .on_press(Message::RemoveComboButton(i, b)),
            );
        }
        let remaining: Vec<Button> =
            Button::EVERY.into_iter().filter(|b| !combo.buttons.contains(b)).collect();
        members = members.push(
            dropdown(remaining, None::<Button>, move |b| Message::AddComboButton(i, b))
                .placeholder("Add button…")
                .width(170),
        );
        let mut body = column![
            row![
                row_toggle(&name, target, true, problem.is_some()),
                members,
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            labeled("    Action", fill_x(action_editor(&combo.action, Button::South, &ACTION_KINDS, set_action(target), KeyField::root(target), ui.names))),
        ]
        .spacing(8);
        if let Some(problem) = problem {
            body = body.push(labeled("", text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR).into()));
        }
        // Removing sits beside the box rather than inside it, apart from the combo's own controls.
        rows.push(
            row![
                container(body).padding(10).style(style::inset).width(Length::Fill),
                remove,
            ]
            .spacing(8)
            .into(),
        );
    }
    rows.push(button(text("+ Add combo")).style(style::secondary).on_press(Message::AddCombo).into());
    rows
}

pub(super) fn short_button(b: Button) -> &'static str {
    b.short_name()
}

impl App {
    #[expect(clippy::too_many_lines, clippy::cognitive_complexity, reason = "predates the size lints")]
    pub(super) fn update_profile(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SetRequires(requires) => {
                if let Some(p) = self.profile_mut() {
                    p.requires = requires;
                }
            }
            Message::SetGyro(gyro) => {
                if let Some(p) = self.profile_mut() {
                    p.gyro = gyro;
                }
            }
            Message::EditProfile(name) => {
                if let Some(i) = self.game().profiles.iter().position(|p| p.name == name) {
                    self.editing = i;
                    self.reload_guide_notes();
                }
            }
            Message::RenameProfile(name) => {
                let editing = self.editing;
                let taken = self.game().profiles.iter().enumerate().any(|(i, p)| i != editing && p.name == name);
                let Some(old) = self.profile().map(|p| p.name.clone()).filter(|_| !taken) else { return Task::none() };
                let key = self.game_key().map(str::to_string);
                let game = self.game_mut();
                game.profiles[editing].name = name.clone();
                // Keep the game's rules, and the auto-switch default, pointing at it.
                for rule in game.rules.iter_mut().filter(|r| r.profile == old) {
                    rule.profile = name.clone();
                }
                let rename = Rename::Profile { game: key.clone(), old: old.clone(), new: name.clone() };
                if let Some(d) = self.config.auto_switch.default_profile.as_mut().filter(|d| d.game == key && d.profile == old) {
                    d.profile = name;
                }
                self.config.active = follow_renames(std::slice::from_ref(&rename), self.config.active.clone());
                self.renames.push(rename);
            }
            Message::AddProfile(template) => {
                let name = unique_name(template.base_name(), |n| self.game().profile(n).is_some());
                let profile = match template {
                    Template::Duplicate => {
                        let mut p = self.profile().cloned().unwrap_or_else(|| Profile::passthrough(""));
                        p.name = name;
                        p
                    }
                    other => other.make(&name),
                };
                self.game_mut().profiles.push(profile);
                self.editing = self.game().profiles.len() - 1;
            }
            Message::DeleteProfile => {
                let editing = self.editing;
                let game = self.game_mut();
                if game.profiles.len() > 1 && editing < game.profiles.len() {
                    game.profiles.remove(editing);
                    self.editing = editing.min(self.game().profiles.len() - 1);
                }
            }
            Message::SetAction(target, action) => {
                if let Some(p) = self.profile_mut() {
                    match target {
                        Target::Button(b) => p.set_button(b, action),
                        Target::Trigger(t) => {
                            if let TriggerAction::Button { action: a, .. } = p.trigger_mut(t) {
                                *a = action;
                            }
                        }
                        Target::Combo(i) => {
                            if let Some(c) = p.combos.get_mut(i) {
                                c.action = action;
                            }
                        }
                        Target::Gesture(b, kind) => {
                            *p.gestures.entry(b).or_default().slot(kind) = Some(action);
                        }
                        Target::Zone(a, i) => {
                            if let Some(z) = p.zones_mut(a).get_mut(i) {
                                z.action = action;
                            }
                        }
                        Target::RingSector(st, i) => {
                            if let Some(a) = p.stick_mut(st).action.ring_actions_mut().get_mut(i) {
                                *a = action;
                            }
                        }
                        Target::MacroStep(..) | Target::MenuItem(..) | Target::StickResponse(_) => {}
                    }
                }
            }
            Message::AddGesture(b, kind) => {
                if let Some(p) = self.profile_mut() {
                    let slot = p.gestures.entry(b).or_default().slot(kind);
                    if slot.is_none() {
                        *slot = Some(ButtonAction::Keys(Vec::new()));
                    }
                }
            }
            Message::RemoveGesture(b, kind) => {
                if let Some(p) = self.profile_mut()
                    && let Some(g) = p.gestures.get_mut(&b)
                {
                    *g.slot(kind) = None;
                    if g.is_empty() {
                        p.gestures.remove(&b);
                    }
                }
            }
            Message::AddZone(a, preset) => {
                if let Some(p) = self.profile_mut() {
                    p.zones_mut(a).push(match preset {
                        ZonePreset::Empty => Zone { min: 0.0, max: 0.5, action: ButtonAction::Keys(Vec::new()) },
                        ZonePreset::Walk => Zone {
                            min: 0.0,
                            max: 0.75,
                            action: ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into()]),
                        },
                    });
                }
            }
            Message::RemoveZone(a, i) => {
                if let Some(p) = self.profile_mut()
                    && i < p.zones(a).len()
                {
                    p.zones_mut(a).remove(i);
                }
            }
            Message::SetZoneRange(a, i, min, max) => {
                if let Some(z) = self.profile_mut().and_then(|p| p.zones_mut(a).get_mut(i)) {
                    z.min = min;
                    z.max = max;
                }
            }
            Message::SelectProfileTab(tab) => {
                self.profile_tab = tab;
                self.found = None;
                if tab == ProfileTab::Guide {
                    self.reload_guide_notes();
                }
            }
            Message::Guide(msg) => self.update_guide(msg),
            Message::TogglePicture => self.picture_hidden = !self.picture_hidden,
            Message::ToggleExpanded(target) => {
                if !self.expanded.remove(&target) {
                    self.expanded.insert(target);
                }
                if let Target::Button(b) = target
                    && self.found == Some(b)
                {
                    self.found = None;
                    self.expanded.remove(&target);
                }
            }
            Message::ExpandAll(open) => {
                let targets: Vec<Target> = match self.profile_tab {
                    ProfileTab::Buttons => Button::ALL.into_iter().chain(Button::PADDLES).map(Target::Button).collect(),
                    ProfileTab::Sticks => Vec::new(),
                    ProfileTab::Combos => {
                        (0..self.profile().map_or(0, |p| p.combos.len())).map(Target::Combo).collect()
                    }
                    ProfileTab::Gyro | ProfileTab::Guide => Vec::new(),
                };
                for t in targets {
                    if open {
                        self.expanded.insert(t);
                    } else {
                        self.expanded.remove(&t);
                    }
                }
                self.found = None;
            }
            Message::StartFind => {
                self.finding = true;
                self.found = None;
            }
            Message::CancelFind => self.finding = false,
            Message::SetTapWindow(ms) => {
                if let Some(p) = self.profile_mut() {
                    p.tap_window_ms = ms.round() as u64;
                }
            }
            Message::SetLongPress(ms) => {
                if let Some(p) = self.profile_mut() {
                    p.long_press_ms = ms.round() as u64;
                }
            }
            Message::AddCombo => {
                if let Some(p) = self.profile_mut() {
                    p.combos.push(Combo {
                        buttons: vec![Button::LeftBumper, Button::RightBumper],
                        action: ButtonAction::Keys(Vec::new()),
                    });
                    let new = Target::Combo(p.combos.len() - 1);
                    self.expanded.insert(new);
                }
            }
            Message::RemoveCombo(i) => {
                if let Some(p) = self.profile_mut()
                    && i < p.combos.len()
                {
                    p.combos.remove(i);
                    // Indices shift, so open state can't carry over.
                    self.expanded.retain(|t| !matches!(t, Target::Combo(_)));
                }
            }
            Message::AddComboButton(i, b) => {
                if let Some(c) = self.profile_mut().and_then(|p| p.combos.get_mut(i))
                    && !c.buttons.contains(&b)
                {
                    c.buttons.push(b);
                }
            }
            Message::RemoveComboButton(i, b) => {
                if let Some(c) = self.profile_mut().and_then(|p| p.combos.get_mut(i)) {
                    c.buttons.retain(|x| *x != b);
                }
            }
            Message::SetComboWindow(ms) => {
                if let Some(p) = self.profile_mut() {
                    p.combo_window_ms = ms.round() as u64;
                }
            }
            Message::SetStick(s, cfg) => {
                if let Some(p) = self.profile_mut() {
                    *p.stick_mut(s) = cfg;
                }
            }
            Message::SetTrigger(t, action) => {
                if let Some(p) = self.profile_mut() {
                    *p.trigger_mut(t) = action;
                }
            }
            other => return self.update_actions(other),
        }
        Task::none()
    }

    /// Opens and highlights `b`'s row and scrolls near it.
    pub(super) fn jump_to(&mut self, b: Button) -> Task<Message> {
        self.finding = false;
        self.found = Some(b);
        if self.game_tab != GameTab::Layers {
            self.game_tab = GameTab::Profiles;
        }
        let (tab, position) = match Button::ALL.iter().position(|x| *x == b) {
            Some(i) => (ProfileTab::Buttons, i as f32 / Button::ALL.len() as f32),
            None if Button::PADDLES.contains(&b) => {
                let i = Button::PADDLES.iter().position(|x| *x == b).unwrap_or_default();
                (ProfileTab::Buttons, (Button::ALL.len() + i) as f32 / (Button::ALL.len() + Button::PADDLES.len()) as f32)
            }
            None => {
                let right = b.stick_direction().is_some_and(|(s, _)| s == Stick::Right);
                (ProfileTab::Sticks, if right { 0.75 } else { 0.25 })
            }
        };
        self.profile_tab = tab;
        iced::widget::operation::snap_to("main", scrollable::RelativeOffset { x: Some(0.0), y: Some(position) })
    }

    pub(super) fn view_profile_tab<'a>(&'a self, names: &Names) -> Element<'a, Message> {
        let col = column![self.view_profile_bar()].spacing(16);
        let Some(p) = self.profile() else { return col.into() };
        col.push(self.view_profile_editor(p, names, None)).into()
    }

    /// The live drawing, "Find by pressing", and the Buttons, Sticks & triggers, Combos and
    /// Gyro tabs for `p`: a profile, or (with `layer`) a layer over one.
    pub(super) fn view_profile_editor<'a>(&'a self, p: &'a Profile, names: &Names, layer: Option<LayerMarks<'a>>) -> Element<'a, Message> {
        // The active profile's drawing also shows the layers that are on right now.
        let live = (layer.is_none() && self.profile_ref().is_some_and(|at| at == self.saved.active)).then(|| self.with_active_layers(p));
        let mut col = column![self.view_live(Some(live.as_ref().unwrap_or(p)), true)].spacing(16);

        let find: Element<'_, Message> = if self.finding {
            row![
                text("Press a button or push a stick on your controller…").color(style_accent()),
                button(text("Cancel")).style(style::secondary).on_press(Message::CancelFind),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
        } else {
            row![
                button(text("Find by pressing")).style(style::secondary).on_press_maybe(self.status.is_some().then_some(Message::StartFind)),
                help("Press a button or push a stick on your controller to jump to its mapping.".into()),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        };
        let family = self.glyph_family();
        let sub_tab = |t: ProfileTab| {
            let label = if section_has_problem(p, t, names) { format!("{t}  ⚠") } else { t.to_string() };
            button(row![t.glyph(family), text(label).size(14)].spacing(6).align_y(Alignment::Center))
                .style(style::segment(self.profile_tab == t))
                .padding([5, 14])
                .on_press(Message::SelectProfileTab(t))
        };
        let mut segments = row![].spacing(2);
        for t in ProfileTab::ALL {
            segments = segments.push(sub_tab(t));
        }
        let mut tabs = row![container(segments).padding(3).style(style::segments), space::horizontal()]
            .spacing(6)
            .align_y(Alignment::Center);
        if self.profile_tab != ProfileTab::Gyro {
            tabs = tabs
                .push(button(text("Expand all").size(13)).style(button::text).on_press(Message::ExpandAll(true)))
                .push(button(text("Collapse all").size(13)).style(button::text).on_press(Message::ExpandAll(false)));
        }
        col = col.push(row![find].align_y(Alignment::Center)).push(tabs);

        let ui = Ui {
            names,
            expanded: &self.expanded,
            found: self.found,
            analog_triggers: self.analog_triggers(),
            any_gyro: self.any_gyro(),
            any_paddles: self.any_paddles(),
            live_sticks: self.shown_input().map(|l| [l.left_stick, l.right_stick]),
            family: self.glyph_family(),
            in_use: self.shown_pad().family,
            swap: self.swap_for(p),
            layer,
        };
        let guide = GuideView { notes: &self.guide_notes, family: ui.family, swap: ui.swap };
        col.push(view_profile(p, &ui, self.profile_tab, guide)).into()
    }

    /// The live controller drawing, labeled with `labels_from`'s mappings.
    pub(super) fn view_live(&self, labels_from: Option<&Profile>, foldable: bool) -> Element<'_, Message> {
        let caption = match (&self.live, &self.status) {
            (Some(live), _) => match live.gyro {
                Some([pitch, yaw, roll]) => {
                    let [pitch, yaw, roll] = [pitch, yaw, roll].map(|v| crate::monitor::signed(v, 0));
                    format!("{} · gyro pitch {pitch} yaw {yaw} roll {roll} °/s", live.device)
                }
                None => live.device.clone(),
            },
            (None, Some(_)) => "Press a button on a managed controller".into(),
            (None, None) => NEEDS_DAEMON.into(),
        };
        let layers = self.status.as_ref().map(|s| s.active_layers.as_slice()).unwrap_or_default();
        let caption = if layers.is_empty() { caption } else { format!("{caption} · Layers: {}", layers.join(" + ")) };
        let folded = foldable && self.picture_hidden;
        let title: Element<'_, Message> = if foldable {
            row![
                text("Live input").size(20),
                button(text(if folded { "Show" } else { "Hide" }).size(13)).style(button::text).on_press(Message::TogglePicture),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        } else {
            text("Live input").size(20).into()
        };
        if folded {
            return title;
        }
        let pad = self.shown_pad();
        let labels = labels_from.map(drawing_labels).unwrap_or_default();
        // Without callouts the picture alone doesn't say whether anything is mapped.
        let caption = if labels_from.is_some() && labels.is_empty() { format!("Every input passes through unchanged · {caption}") } else { caption };
        column![
            title,
            container(controller_drawing(
                    self.shown_input(),
                    pad.model,
                    // The picture is the controller itself, so it keeps the controller's own labels.
                    Glyphs { family: pad.family, nintendo_layout: false },
                    &labels,
                    self.status.is_none(),
                )).center_x(Length::Fill),
            container(text(caption).size(13).color(MUTED_COLOR)).center_x(Length::Fill),
        ]
        .spacing(8)
        .into()
    }

    pub(super) fn view_profile_bar(&self) -> Element<'_, Message> {
        let names: Vec<String> = self.game().profiles.iter().map(|p| p.name.clone()).collect();
        let current = self.profile().map(|p| p.name.clone());
        let can_delete = names.len() > 1;
        let active = self.profile_ref().is_some_and(|at| at == self.saved.active);
        let activate: Element<'_, Message> = match self.profile_ref() {
            Some(at) if !active && self.status.is_some() => {
                button(text("Make active").size(13)).style(style::secondary).on_press(Message::ActivateProfile(at)).into()
            }
            _ if active => text("● active").size(13).color(style_accent()).into(),
            _ => space().into(),
        };
        column![
            row![text("Edit profile").size(20), activate].spacing(12).align_y(Alignment::Center),
            row![
                dropdown(names, current.clone(), Message::EditProfile).width(170),
                field("Profile name", current.as_deref().unwrap_or(""))
                    .on_input(Message::RenameProfile)
                    .width(170),
                space::horizontal(),
                dropdown(Template::NEW, None::<Template>, Message::AddProfile)
                    .placeholder("+ From template…")
                    .width(200),
                button(text("Duplicate")).style(style::secondary).on_press(Message::AddProfile(Template::Duplicate)),
                button(text("Delete"))
                    .style(style::danger)
                    .on_press_maybe(can_delete.then_some(Message::DeleteProfile)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .into()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::tests::*;

    #[test]
    fn trigger_depth_settings_hide_only_when_all_pads_are_digital() {
        let mut app = app();
        let status = |devices| Status { enabled: true, active_profile: "Gamepad".into(), devices, ..Default::default() };
        assert!(app.analog_triggers(), "no daemon: assume analog");

        app.status = Some(status(vec![device("Pro Controller", false, false)]));
        assert!(!app.analog_triggers());

        app.status = Some(status(vec![device("Pro Controller", false, false), device("Xbox", true, false)]));
        assert!(app.analog_triggers());

        // An ignored Xbox pad doesn't count.
        app.status = Some(status(vec![device("Pro Controller", false, false), device("Xbox", true, true)]));
        assert!(!app.analog_triggers());

        app.status = Some(status(vec![]));
        assert!(app.analog_triggers());
    }

    #[test]
    fn renaming_a_profile_updates_rules_default_and_the_active_one() {
        let mut app = with_game();
        app.config.auto_switch.default_profile = Some(ProfileRef::new(Some("Doom"), "Play"));
        // The daemon says Doom › Play is active.
        let status = Status { active_profile: "Play".into(), active_game: Some("Doom".into()), ..Default::default() };
        let _ = app.update(Message::StatusLoaded(Ok(status.clone())));
        let _ = app.update(Message::RenameProfile("Fight".into()));
        assert_eq!(app.game().rules[0].profile, "Fight");
        assert_eq!(app.config.auto_switch.default_profile, Some(ProfileRef::new(Some("Doom"), "Fight")));
        // Until saved, the daemon keeps reporting the old name; it's followed.
        let _ = app.update(Message::StatusLoaded(Ok(status)));
        assert_eq!(app.config.active, ProfileRef::new(Some("Doom"), "Fight"));
        assert_eq!(app.saved.active, ProfileRef::new(Some("Doom"), "Play"));
        assert_eq!(app.validate(), None);

        let _ = app.update(Message::SetDefaultProfile(DefaultChoice(None)));
        assert_eq!(app.config.auto_switch.default_profile, None);
    }

    #[test]
    fn new_profile_from_template_gets_unique_name_and_is_edited() {
        let mut app = app();
        let _ = app.update(Message::AddProfile(Template::Action));
        let _ = app.update(Message::AddProfile(Template::Action));
        let names: Vec<&str> = app.config.general.profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Gamepad", "Desktop", "PC Action", "PC Action 2"]);
        assert_eq!(app.profile().unwrap().name, "PC Action 2");
        assert_eq!(app.validate(), None);
    }

    #[test]
    fn stick_directions_are_combo_members_and_outputs_not_action_holders() {
        let mut app = app();
        // Combos accept stick directions as members.
        let _ = app.update(Message::AddCombo);
        let _ = app.update(Message::RemoveComboButton(0, Button::RightBumper));
        let _ = app.update(Message::AddComboButton(0, Button::RightStickRight));
        assert_eq!(app.config.general.profiles[0].combos[0].buttons, [Button::LeftBumper, Button::RightStickRight]);
        // Any button can output a stick direction.
        let _ = app.update(Message::SetAction(Target::Button(Button::South), ButtonAction::Gamepad(Button::RightStickUp)));
        assert_eq!(app.config.general.profiles[0].button(Button::South), &ButtonAction::Gamepad(Button::RightStickUp));
        // Validation still checks the actions that are editable.
        let _ = app.update(Message::SetAction(
            Target::Button(Button::South),
            ButtonAction::Keys(vec!["KEY_NOPE".into()]),
        ));
        assert!(app.validate().unwrap().contains("NOPE"), "{:?}", app.validate());
    }

    #[test]
    fn find_by_pressing_detects_new_presses_and_firm_stick_pushes() {
        let held = snapshot(&[Button::South], (0.0, 0.0));
        assert_eq!(newly_pressed(None, &held), Some(Button::South));
        // Still held from before: not new. A second button is.
        assert_eq!(newly_pressed(Some(&held), &held), None);
        assert_eq!(newly_pressed(Some(&held), &snapshot(&[Button::South, Button::West], (0.0, 0.0))), Some(Button::West));
        // A light touch on a stick doesn't count; a firm push does.
        assert_eq!(newly_pressed(Some(&held), &snapshot(&[Button::South], (0.4, 0.0))), None);
        assert_eq!(newly_pressed(Some(&held), &snapshot(&[Button::South], (0.0, -0.9))), Some(Button::RightStickUp));
    }

    #[test]
    fn find_by_pressing_opens_and_highlights_the_row() {
        let mut app = app();
        let _ = app.update(Message::StartFind);
        assert!(app.finding);
        let _ = app.update(Message::LiveInput(Some(snapshot(&[Button::West], (0.0, 0.0)))));
        assert!(!app.finding, "one press ends find mode");
        assert_eq!((app.game_tab, app.profile_tab, app.found), (GameTab::Profiles, ProfileTab::Buttons, Some(Button::West)));
        let names = Names::default();
        let ui = Ui { names: &names, expanded: &app.expanded, found: app.found, analog_triggers: true, any_gyro: false, any_paddles: false, live_sticks: None, family: PadFamily::default(), in_use: PadFamily::default(), swap: Swap::Off, layer: None };
        assert!(ui.is_open(Target::Button(Button::West)));
        // Presses while not finding don't move the editor.
        let _ = app.update(Message::LiveInput(Some(snapshot(&[Button::West, Button::North], (0.0, 0.0)))));
        assert_eq!(app.found, Some(Button::West));
        // A stick push jumps to the Sticks & triggers tab.
        let _ = app.update(Message::StartFind);
        let _ = app.update(Message::LiveInput(Some(snapshot(&[], (0.95, 0.0)))));
        assert_eq!((app.profile_tab, app.found), (ProfileTab::Sticks, Some(Button::RightStickRight)));
    }

    #[test]
    fn rows_expand_and_collapse() {
        let mut app = app();
        let a = Target::Button(Button::South);
        let _ = app.update(Message::ToggleExpanded(a));
        assert!(app.expanded.contains(&a));
        let _ = app.update(Message::ToggleExpanded(a));
        assert!(!app.expanded.contains(&a));
        let _ = app.update(Message::ExpandAll(true));
        assert_eq!(app.expanded.len(), Button::ALL.len() + Button::PADDLES.len());
        let _ = app.update(Message::SelectProfileTab(ProfileTab::Sticks));
        // The Sticks tab has no button rows to open, so expanding it changes nothing.
        let _ = app.update(Message::ExpandAll(true));
        assert_eq!(app.expanded.len(), Button::ALL.len() + Button::PADDLES.len());
        let _ = app.update(Message::ExpandAll(false));
        assert_eq!(app.expanded.len(), Button::ALL.len() + Button::PADDLES.len());
        let _ = app.update(Message::SelectProfileTab(ProfileTab::Buttons));
        let _ = app.update(Message::ExpandAll(false));
        assert_eq!(app.expanded.len(), 0, "collapse all only touches the current section");
    }
}

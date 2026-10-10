//! Triggers on the Sticks & triggers tab, and the zones both sticks and triggers can have: extra
//! actions held within a range of travel.

use iced::widget::{column, row};

use super::*;

/// A trigger's action in a few words.
pub(super) fn trigger_summary(cfg: &crate::config::TriggerConfig) -> String {
    let action = match &cfg.action {
        TriggerAction::Disabled => "Disabled".to_string(),
        TriggerAction::Gamepad(t) => format!("pad {t}"),
        TriggerAction::Button { action, .. } => summarize(action),
    };
    format!("{action}{}", zones_note(cfg.zones.len()))
}

/// " + 2 zones", or nothing.
pub(super) fn zones_note(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => " + 1 zone".into(),
        n => format!(" + {n} zones"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ZonePreset {
    Empty,
    /// Left Shift while a stick is only partly pushed (walk in WASD games).
    Walk,
}

/// Extra actions held while the stick/trigger is within a range of travel.
pub(super) fn zone_editor<'a>(analog: Analog, zones: &'a [Zone], names: &Names) -> Element<'a, Message> {
    let what = match analog {
        Analog::Stick(_) => "pushed",
        Analog::Trigger(_) => "pulled",
    };
    let zones_help = help(format!(
        "Extra outputs held while it is {what} within a range (0 = just past rest, 1 = all the \
         way), on top of its main action."
    ));
    let mut col = column![].spacing(8);

    for (i, zone) in zones.iter().enumerate() {
        let (min, max) = (zone.min, zone.max);
        let range = row![
            text("From").size(13),
            slider(0.0..=1.0, min, move |v| Message::SetZoneRange(analog, i, v, max)).step(0.05_f32).width(140),
            text(format!("{min:.2}")).size(13),
            text("to").size(13),
            slider(0.0..=1.0, max, move |v| Message::SetZoneRange(analog, i, min, v)).step(0.05_f32).width(140),
            text(format!("{max:.2}")).size(13),
            space::horizontal(),
            button(text("✕").size(13)).style(style::secondary).on_press(Message::RemoveZone(analog, i)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let mut body = column![
            range,
            fill_x(action_editor(&zone.action, Button::South, &ACTION_KINDS, set_action(Target::Zone(analog, i)), KeyField::root(Target::Zone(analog, i)), names)),
        ]
        .spacing(8);
        if min >= max {
            body = body.push(text("The range must start below where it ends.").size(12).color(ERROR_COLOR));
        }
        col = col.push(container(body).padding(10).style(style::inset));
    }

    let mut buttons = row![
        button(text("+ Add zone").size(13)).style(style::secondary).on_press(Message::AddZone(analog, ZonePreset::Empty)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if matches!(analog, Analog::Stick(_)) {
        buttons = buttons.push(
            button(text("+ Walk modifier (Shift on partial push)").size(13))
                .style(style::secondary)
                .on_press(Message::AddZone(analog, ZonePreset::Walk)),
        );
    }
    labeled("    Zones", col.push(buttons.push(zones_help)).into())
}

/// What a trigger does, in one list: pass through as an analog trigger, or act as a button
/// with any button action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TriggerChoice {
    Analog(Trigger),
    Action(ActionKind),
}

impl fmt::Display for TriggerChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TriggerChoice::Analog(Trigger::Left) => f.write_str("Gamepad LT (analog)"),
            TriggerChoice::Analog(Trigger::Right) => f.write_str("Gamepad RT (analog)"),
            TriggerChoice::Action(k) => k.fmt(f),
        }
    }
}

#[expect(clippy::too_many_lines, reason = "predates the size lints")]
pub(super) fn trigger_editor<'a>(
    t: Trigger,
    action: &'a TriggerAction,
    zones: &'a [Zone],
    analog: bool,
    names: &Names,
) -> Element<'a, Message> {
    let choice = match action {
        TriggerAction::Disabled => TriggerChoice::Action(ActionKind::Disabled),
        TriggerAction::Gamepad(out) => TriggerChoice::Analog(*out),
        TriggerAction::Button { action, .. } => TriggerChoice::Action(action_kind(action)),
    };
    // Analog outputs first, then everything a button can do.
    let other = if t == Trigger::Left { Trigger::Right } else { Trigger::Left };
    let mut choices = vec![TriggerChoice::Action(ActionKind::Disabled), TriggerChoice::Analog(t), TriggerChoice::Analog(other)];
    choices.extend(ACTION_KINDS.into_iter().skip(1).map(TriggerChoice::Action));
    // A trigger acting as a button presses the matching bumper by default.
    let default_button = if t == Trigger::Left { Button::LeftBumper } else { Button::RightBumper };
    let (threshold, current) = match action {
        TriggerAction::Button { action, threshold } => (*threshold, action.clone()),
        _ => (0.5, ButtonAction::Disabled),
    };
    let picker = {
        let names = names.clone();
        dropdown(choices, Some(choice), move |c| {
            Message::SetTrigger(
                t,
                match c {
                    TriggerChoice::Analog(out) => TriggerAction::Gamepad(out),
                    TriggerChoice::Action(ActionKind::Disabled) => TriggerAction::Disabled,
                    TriggerChoice::Action(k) => {
                        TriggerAction::Button { action: new_action(k, default_button, &current, &names), threshold }
                    }
                },
            )
        })
        .width(215)
    };

    let mut line = row![text(t.to_string()).width(LABEL_WIDTH), picker].spacing(10).align_y(Alignment::Start);
    let mut rows = column![].spacing(8);
    if let TriggerAction::Button { action: inner, threshold } = action {
        let threshold = *threshold;
        let on_change: OnAction<'a> = Rc::new(move |a| Message::SetTrigger(t, TriggerAction::Button { action: a, threshold }));
        line = line.push(action_value(inner, default_button, on_change, KeyField::root(Target::Trigger(t)), names));
        rows = rows.push(line);
        if analog {
            let inner = inner.clone();
            rows = rows.push(value_slider("    Presses at", 0.05..=0.95, threshold, 0.05, "", move |v| {
                Message::SetTrigger(t, TriggerAction::Button { action: inner.clone(), threshold: v })
            }));
        }
    } else {
        rows = rows.push(line);
    }
    if analog {
        return rows.push(zone_editor(Analog::Trigger(t), zones, names)).into();
    }
    rows = rows.push(labeled(
        "",
        text("Your controller's triggers are on/off only, so pull thresholds and zones don't apply.")
            .size(12)
            .color(MUTED_COLOR)
            .into(),
    ));
    // Keep existing zones reachable so they can be removed rather than silently lingering.
    if !zones.is_empty() {
        rows = rows
            .push(labeled(
                "",
                text("This profile still has zones for this trigger; a full pull acts as the top zone.")
                    .size(12)
                    .color(ERROR_COLOR)
                    .into(),
            ))
            .push(zone_editor(Analog::Trigger(t), zones, names));
    }
    rows.into()
}

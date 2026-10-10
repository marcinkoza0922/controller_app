//! Sticks on the Sticks & triggers tab: each stick's mode (gamepad, mouse, scroll, button ring,
//! flick stick) and its settings.

use iced::widget::{Column, column, row};

use super::*;
use crate::config::{FlickVertical, MouseResponse};

/// A stick's mode in a few words.
pub(super) fn stick_summary(cfg: &StickConfig) -> String {
    let mode = match &cfg.action {
        StickAction::Disabled => "Disabled".to_string(),
        StickAction::Gamepad { stick, .. } => format!("pad {stick}"),
        StickAction::Mouse { speed, response, .. } if response.accel > 0.0 => {
            format!("mouse, {speed:.0} px/s, accel {:.0}%", response.accel * 100.0)
        }
        StickAction::Mouse { speed, .. } => format!("mouse, {speed:.0} px/s"),
        StickAction::Scroll { speed, .. } => format!("scroll, {speed:.0} notches/s"),
        StickAction::Ring { sectors, .. } => format!("button ring, {sectors} sectors"),
        StickAction::Flick { full_turn_px, .. } => format!("flick stick, {full_turn_px:.0} px/turn"),
    };
    format!("{mode}{}", zones_note(cfg.zones.len()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StickKind {
    Disabled,
    Gamepad,
    Mouse,
    Scroll,
    Ring,
    Flick,
}

impl fmt::Display for StickKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StickKind::Disabled => "Disabled",
            StickKind::Gamepad => "Gamepad stick",
            StickKind::Mouse => "Mouse pointer",
            StickKind::Scroll => "Scroll wheel",
            StickKind::Ring => "Button ring",
            StickKind::Flick => "Flick stick",
        })
    }
}

#[expect(clippy::too_many_lines, reason = "predates the size lints")]
pub(super) fn stick_editor<'a>(
    s: Stick,
    cfg: &'a StickConfig,
    names: &Names,
    expanded: &HashSet<Target>,
    live: Option<(f32, f32)>,
) -> Element<'a, Message> {
    let kind = match cfg.action {
        StickAction::Disabled => StickKind::Disabled,
        StickAction::Gamepad { .. } => StickKind::Gamepad,
        StickAction::Mouse { .. } => StickKind::Mouse,
        StickAction::Scroll { .. } => StickKind::Scroll,
        StickAction::Ring { .. } => StickKind::Ring,
        StickAction::Flick { .. } => StickKind::Flick,
    };
    let with = move |action: StickAction| {
        let mut c = cfg.clone();
        c.action = action;
        Message::SetStick(s, c)
    };
    let kinds = [
        StickKind::Disabled,
        StickKind::Gamepad,
        StickKind::Mouse,
        StickKind::Scroll,
        StickKind::Ring,
        StickKind::Flick,
    ];
    let picker = dropdown(kinds, Some(kind), move |k| {
        with(match k {
            StickKind::Disabled => StickAction::Disabled,
            StickKind::Gamepad => StickAction::Gamepad { stick: s, invert_y: false },
            StickKind::Mouse => StickAction::mouse(1200.0),
            StickKind::Scroll => StickAction::Scroll { speed: 15.0, invert_y: false },
            StickKind::Ring => StickAction::ring(8),
            StickKind::Flick => StickAction::flick(),
        })
    })
    .width(170);

    let mut rows = column![labeled(s.to_string(), picker.into())].spacing(8);

    match &cfg.action {
        StickAction::Gamepad { stick, invert_y } => {
            let invert_y = *invert_y;
            let stick = *stick;
            rows = rows.push(labeled(
                "    Output",
                row![
                    dropdown([Stick::Left, Stick::Right], Some(stick), move |t| {
                        with(StickAction::Gamepad { stick: t, invert_y })
                    })
                    .width(170),
                    checkbox(invert_y)
                        .label("Invert Y")
                        .on_toggle(move |inv| with(StickAction::Gamepad { stick, invert_y: inv })),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
            ));
        }
        StickAction::Mouse { .. } => {
            rows = mouse_rows(rows, s, cfg, expanded.contains(&Target::StickResponse(s)), with);
        }
        StickAction::Ring { sectors, start_angle, inner_radius, .. } => {
            rows = rows.push(super::ring_preview::view(*sectors, *start_angle, *inner_radius, live));
            rows = ring_rows(rows, s, cfg, names, with);
        }
        StickAction::Flick { .. } => {
            rows = flick_rows(rows, cfg, expanded.contains(&Target::StickResponse(s)), s, with);
        }
        StickAction::Scroll { speed, invert_y } => {
            let (speed, invert_y) = (*speed, *invert_y);
            rows = rows.push(value_slider("    Speed", 1.0..=60.0, speed, 1.0, "notches/s", move |v| {
                with(StickAction::Scroll { speed: v, invert_y })
            }));
            rows = rows.push(labeled(
                "    Output",
                checkbox(invert_y)
                    .label("Invert Y")
                    .on_toggle(move |inv| with(StickAction::Scroll { speed, invert_y: inv }))
                    .into(),
            ));
        }
        StickAction::Disabled => {}
    }

    if !matches!(cfg.action, StickAction::Disabled) {
        rows = rows.push(value_slider("    Deadzone", 0.0..=0.5, cfg.deadzone, 0.01, "", move |v| {
            let mut c = cfg.clone();
            c.deadzone = v;
            Message::SetStick(s, c)
        }));
    }
    if matches!(cfg.action, StickAction::Mouse { .. } | StickAction::Scroll { .. }) {
        rows = rows.push(value_slider("    Curve", 1.0..=3.0, cfg.curve, 0.1, "(1 = linear)", move |v| {
            let mut c = cfg.clone();
            c.curve = v;
            Message::SetStick(s, c)
        }));
    }
    // Used by the stick's direction buttons.
    rows = rows.push(value_slider("    Directions press at", 0.05..=0.95, cfg.key_threshold, 0.05, "", move |v| {
        let mut c = cfg.clone();
        c.key_threshold = v;
        Message::SetStick(s, c)
    }));
    rows.push(zone_editor(Analog::Stick(s), &cfg.zones, names)).into()
}

/// A button ring's settings and one action editor per sector.
fn ring_rows<'a>(
    mut rows: Column<'a, Message>,
    s: Stick,
    cfg: &'a StickConfig,
    names: &Names,
    with: impl Fn(StickAction) -> Message + Copy + 'a,
) -> Column<'a, Message> {
    let StickAction::Ring { sectors, start_angle, inner_radius, hysteresis, actions } = &cfg.action else {
        return rows;
    };
    let (sectors, start_angle, inner_radius, hysteresis) = (*sectors, *start_angle, *inner_radius, *hysteresis);
    let keep = actions.clone();
    let set = move |sectors: u8, start_angle: f32, inner_radius: f32, hysteresis: f32| {
        let mut actions = keep.clone();
        actions.resize(usize::from(sectors), ButtonAction::Disabled);
        with(StickAction::Ring { sectors, start_angle, inner_radius, hysteresis, actions })
    };
    rows = rows
        .push(labeled(
            "    Sectors",
            dropdown([4_u8, 8, 12], Some(sectors), {
                let set = set.clone();
                move |n| set(n, start_angle, inner_radius, hysteresis)
            })
            .width(170)
            .into(),
        ))
        .push(value_slider("    First sector at", 0.0..=345.0, start_angle, 15.0, "° from up", {
            let set = set.clone();
            move |v| set(sectors, v, inner_radius, hysteresis)
        }))
        .push(value_slider("    Starts at", 0.05..=0.95, inner_radius, 0.05, "of the way out", {
            let set = set.clone();
            move |v| set(sectors, start_angle, v, hysteresis)
        }))
        .push(value_slider("    Stickiness", 0.0..=0.4, hysteresis, 0.05, "of a sector", move |v| {
            set(sectors, start_angle, inner_radius, v)
        }))
        .push(labeled(
            "    Presets",
            row![
                button(text("WASD").size(13)).style(style::secondary).on_press(with(StickAction::wasd())),
                button(text("Arrows").size(13)).style(style::secondary).on_press(with(StickAction::arrows())),
            ]
            .spacing(8)
            .into(),
        ));
    let width = 360.0 / f32::from(sectors.max(1));
    for i in 0..usize::from(sectors) {
        let angle = start_angle + i as f32 * width;
        let arrow = ["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"][((angle / 45.0).round() as usize) % 8];
        let action = actions.get(i).unwrap_or(&ButtonAction::Disabled);
        let target = Target::RingSector(s, i);
        rows = rows.push(labeled(
            format!("    {arrow} Sector {}", i + 1),
            container(fill_x(action_editor(action, Button::South, &ACTION_KINDS, set_action(target), KeyField::root(target), names)))
                .padding(10)
                .style(style::inset)
                .into(),
        ));
    }
    rows
}

/// A flick stick's settings: the turn size and trigger point, and the rest behind "Advanced".
fn flick_rows<'a>(
    mut rows: Column<'a, Message>,
    cfg: &StickConfig,
    open: bool,
    s: Stick,
    with: impl Fn(StickAction) -> Message + Copy + 'a,
) -> Column<'a, Message> {
    let StickAction::Flick { full_turn_px, flick_threshold, flick_time_ms, rotate_smoothing_ms, forward_deadzone, vertical, vertical_speed } =
        cfg.action
    else {
        return rows;
    };
    let set = move |f: &dyn Fn(&mut StickAction)| {
        let mut a = StickAction::Flick { full_turn_px, flick_threshold, flick_time_ms, rotate_smoothing_ms, forward_deadzone, vertical, vertical_speed };
        f(&mut a);
        with(a)
    };
    macro_rules! field {
        ($name:ident, $value:expr) => {
            set(&|a| {
                if let StickAction::Flick { $name, .. } = a {
                    *$name = $value;
                }
            })
        };
    }
    rows = rows
        .push(value_slider("    Turn size", 1000.0..=30000.0, full_turn_px, 100.0, "px per 360°", move |v| field!(full_turn_px, v)))
        .push(labeled(
            "    ",
            row![
                button(text("Test turn").size(13)).style(style::secondary).on_press(Message::TestTurn(full_turn_px as i32)),
                text(
                    "Moves the mouse one full turn to the right after 3 seconds, so you can watch your game. \
                     If it turned N°, set the size to this × 360 / N."
                )
                .size(12)
                .color(MUTED_COLOR),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
        ))
        .push(value_slider("    Flick at", 0.5..=1.0, flick_threshold, 0.05, "of the way out", move |v| field!(flick_threshold, v)));
    rows = rows.push(labeled("    ", row_toggle("Advanced flick", Target::StickResponse(s), open, false)));
    if open {
        rows = rows
            .push(value_slider("        Flick time", 0.0..=300.0, flick_time_ms as f32, 10.0, "ms", move |v| field!(flick_time_ms, v as u32)))
            .push(value_slider("        Turning smoothing", 0.0..=200.0, rotate_smoothing_ms as f32, 5.0, "ms", move |v| {
                field!(rotate_smoothing_ms, v as u32)
            }))
            .push(value_slider("        Forward dead angle", 0.0..=30.0, forward_deadzone, 1.0, "°", move |v| field!(forward_deadzone, v)))
            .push(labeled(
                "        ",
                checkbox(vertical == FlickVertical::Look)
                    .label("Also look up and down with the stick")
                    .on_toggle(move |on| field!(vertical, if on { FlickVertical::Look } else { FlickVertical::Off }))
                    .into(),
            ));
        if vertical == FlickVertical::Look {
            rows = rows.push(value_slider("        Vertical speed", 100.0..=4000.0, vertical_speed, 50.0, "px/s", move |v| field!(vertical_speed, v)));
        }
    }
    rows
}

/// A mouse stick's speed and response rows, the fine-tuning ones behind "Advanced response".
fn mouse_rows<'a>(
    mut rows: Column<'a, Message>,
    s: Stick,
    cfg: &StickConfig,
    open: bool,
    with: impl Fn(StickAction) -> Message + Copy + 'a,
) -> Column<'a, Message> {
    let StickAction::Mouse { speed, response, invert_y } = cfg.action else {
        return rows;
    };
    rows = rows.push(value_slider("    Speed", 100.0..=4000.0, speed, 50.0, "px/s", move |v| {
        with(StickAction::Mouse { speed: v, response, invert_y })
    }));
    rows = rows.push(value_slider("    Acceleration", 0.0..=1.0, response.accel, 0.05, "(0 = off)", move |v| {
        with(StickAction::Mouse { speed, response: MouseResponse { accel: v, ..response }, invert_y })
    }));
    rows = rows.push(labeled(
        "    Output",
        checkbox(invert_y)
            .label("Invert Y")
            .on_toggle(move |inv| with(StickAction::Mouse { speed, response, invert_y: inv }))
            .into(),
    ));
    rows = rows.push(labeled("    ", row_toggle("Advanced response", Target::StickResponse(s), open, false)));
    if open {
        let set = move |r: MouseResponse| with(StickAction::Mouse { speed, response: r, invert_y });
        rows = rows
            .push(value_slider("        Ramp time", 100.0..=2000.0, response.accel_ramp_ms as f32, 50.0, "ms", move |v| {
                set(MouseResponse { accel_ramp_ms: v as u32, ..response })
            }))
            .push(value_slider("        Outer boost", 0.0..=1.0, response.outer_boost, 0.05, "", move |v| {
                set(MouseResponse { outer_boost: v, ..response })
            }))
            .push(value_slider("        Vertical speed", 0.25..=2.0, response.y_scale, 0.05, "× horizontal", move |v| {
                set(MouseResponse { y_scale: v, ..response })
            }))
            .push(value_slider("        Smoothing", 0.0..=200.0, response.smoothing_ms as f32, 5.0, "ms", move |v| {
                set(MouseResponse { smoothing_ms: v as u32, ..response })
            }));
    }
    rows
}

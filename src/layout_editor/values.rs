//! The number and mode pages of Edit Controls: sticks, triggers and the gyro. Each takes a row of
//! its page and changes the active profile's setting for it.

use crate::{
    config::{Button, ButtonAction, GyroActivation, GyroConfig, GyroInput, GyroMode, Profile, Stick, StickAction, StickConfig, Trigger, TriggerAction, TriggerConfig},
    menu::ItemView,
    system_menu::active_profile_mut,
};

use super::{EditStep, item};

/// Rows of a stick's page.
pub const STICK_ACTION: usize = 0;
pub const STICK_SPEED: usize = 1;
pub const STICK_INVERT: usize = 2;
pub const STICK_DEADZONE: usize = 3;
pub const STICK_ROWS: usize = 4;

/// Rows of a trigger's page.
pub const TRIGGER_ACTION: usize = 0;
pub const TRIGGER_ACTS: usize = 1;
pub const TRIGGER_THRESHOLD: usize = 2;
pub const TRIGGER_ROWS: usize = 3;

/// Rows of the gyro page.
pub const GYRO_MODE: usize = 0;
pub const GYRO_SENSITIVITY: usize = 1;
pub const GYRO_INVERT_X: usize = 2;
pub const GYRO_INVERT_Y: usize = 3;
pub const GYRO_NOISE: usize = 4;
pub const GYRO_ACTIVATION: usize = 5;
pub const GYRO_ROWS: usize = 6;

/// The activations "Activation" cycles through: always on, held, or held off, with the usual inputs.
const GYRO_ACTIVATIONS: [GyroActivation; 7] = [
    GyroActivation::Always,
    GyroActivation::WhileHeld(GyroInput::LeftTrigger),
    GyroActivation::WhileHeld(GyroInput::RightTrigger),
    GyroActivation::UnlessHeld(GyroInput::LeftTrigger),
    GyroActivation::UnlessHeld(GyroInput::RightTrigger),
    GyroActivation::Toggle(GyroInput::LeftTrigger),
    GyroActivation::Toggle(GyroInput::RightTrigger),
];

/// Buttons a trigger can act as, in the order "Acts as" cycles through them.
const TRIGGER_BUTTONS: [Button; 6] = [Button::South, Button::East, Button::North, Button::West, Button::LeftBumper, Button::RightBumper];

/// Pixels per second for a new mouse stick, and notches per second for a new scroll stick.
const MOUSE_SPEED: f32 = 1000.0;
const SCROLL_SPEED: f32 = 5.0;
/// Degrees per pixel for a new gyro mouse.
const GYRO_MOUSE_SENSITIVITY: f32 = 15.0;
/// One step of a multiplied number is this ratio.
const RATIO_STEP: f32 = 1.1;
const SPEED_RANGE: (f32, f32) = (10.0, 10_000.0);
const SENSITIVITY_RANGE: (f32, f32) = (0.1, 1_000.0);
const DEADZONE_STEP: f32 = 0.01;
const DEADZONE_RANGE: (f32, f32) = (0.0, 0.5);
const THRESHOLD_STEP: f32 = 0.05;
const THRESHOLD_RANGE: (f32, f32) = (0.05, 0.95);
const NOISE_STEP: f32 = 0.1;
const NOISE_RANGE: (f32, f32) = (0.0, 10.0);

/// A choose on a stick row: the action and Invert Y have their own choose; the rest step up.
pub fn stick_choose(stick: Stick, row: usize, config: &mut crate::config::Config) -> EditStep {
    match row {
        STICK_ACTION => EditStep::changed(edit_stick(config, stick, |cfg| {
            let next = next_stick_action(&cfg.action, stick);
            let changed = next != cfg.action;
            cfg.action = next;
            changed
        })),
        STICK_INVERT => EditStep::changed(edit_stick(config, stick, |cfg| toggle_invert(&mut cfg.action))),
        _ => stick_adjust(stick, row, 1, config),
    }
}

/// One step of a stick's speed or deadzone (`dir` is -1 or 1).
pub fn stick_adjust(stick: Stick, row: usize, dir: i32, config: &mut crate::config::Config) -> EditStep {
    let factor = ratio(dir);
    let step = dir as f32;
    match row {
        STICK_SPEED => EditStep::changed(edit_stick(config, stick, |cfg| match &mut cfg.action {
            StickAction::Mouse { speed, .. } | StickAction::Scroll { speed, .. } => {
                *speed = (*speed * factor).clamp(SPEED_RANGE.0, SPEED_RANGE.1);
                true
            }
            _ => false,
        })),
        STICK_DEADZONE => EditStep::changed(edit_stick(config, stick, |cfg| {
            cfg.deadzone = (cfg.deadzone + DEADZONE_STEP * step).clamp(DEADZONE_RANGE.0, DEADZONE_RANGE.1);
            true
        })),
        _ => EditStep::Stay,
    }
}

pub fn trigger_choose(trigger: Trigger, row: usize, config: &mut crate::config::Config) -> EditStep {
    match row {
        TRIGGER_ACTION => EditStep::changed(edit_trigger(config, trigger, |cfg| {
            cfg.action = next_trigger_action(&cfg.action, trigger);
            true
        })),
        TRIGGER_ACTS => EditStep::changed(edit_trigger(config, trigger, |cfg| match &mut cfg.action {
            TriggerAction::Button { action, .. } => {
                *action = next_trigger_button(action);
                true
            }
            _ => false,
        })),
        _ => trigger_adjust(trigger, row, 1, config),
    }
}

pub fn trigger_adjust(trigger: Trigger, row: usize, dir: i32, config: &mut crate::config::Config) -> EditStep {
    if row != TRIGGER_THRESHOLD {
        return EditStep::Stay;
    }
    let step = dir as f32;
    EditStep::changed(edit_trigger(config, trigger, |cfg| match &mut cfg.action {
        TriggerAction::Button { threshold, .. } => {
            *threshold = (*threshold + THRESHOLD_STEP * step).clamp(THRESHOLD_RANGE.0, THRESHOLD_RANGE.1);
            true
        }
        _ => false,
    }))
}

pub fn gyro_choose(row: usize, config: &mut crate::config::Config) -> EditStep {
    match row {
        GYRO_MODE => EditStep::changed(edit_gyro(config, |g| {
            match g.mode {
                GyroMode::Off => g.mode = GyroMode::Mouse { sensitivity: GYRO_MOUSE_SENSITIVITY },
                GyroMode::Mouse { .. } => g.mode = GyroMode::Off,
                // Stick and steering modes are edited in the pack file; choosing does nothing.
                _ => return false,
            }
            true
        })),
        GYRO_INVERT_X => EditStep::changed(edit_gyro(config, |g| {
            g.invert_x = !g.invert_x;
            true
        })),
        GYRO_INVERT_Y => EditStep::changed(edit_gyro(config, |g| {
            g.invert_y = !g.invert_y;
            true
        })),
        _ => gyro_adjust(row, 1, config),
    }
}

pub fn gyro_adjust(row: usize, dir: i32, config: &mut crate::config::Config) -> EditStep {
    let factor = ratio(dir);
    let step = dir as f32;
    match row {
        GYRO_SENSITIVITY => EditStep::changed(edit_gyro(config, |g| match &mut g.mode {
            GyroMode::Mouse { sensitivity } => {
                *sensitivity = (*sensitivity * factor).clamp(SENSITIVITY_RANGE.0, SENSITIVITY_RANGE.1);
                true
            }
            _ => false,
        })),
        GYRO_ACTIVATION => EditStep::changed(edit_gyro(config, |g| {
            let next = GYRO_ACTIVATIONS.iter().position(|a| *a == g.activation).map_or(0, |i| (i + 1) % GYRO_ACTIVATIONS.len());
            g.activation = GYRO_ACTIVATIONS[next];
            true
        })),
        GYRO_NOISE => EditStep::changed(edit_gyro(config, |g| {
            g.noise_threshold = (g.noise_threshold + NOISE_STEP * step).clamp(NOISE_RANGE.0, NOISE_RANGE.1);
            true
        })),
        _ => EditStep::Stay,
    }
}

fn ratio(dir: i32) -> f32 {
    if dir < 0 { 1.0 / RATIO_STEP } else { RATIO_STEP }
}

/// The title, rows and hint of a stick's page.
pub fn stick_page(stick: Stick, profile: Option<&Profile>) -> (Vec<ItemView>, &'static str) {
    let cfg = profile.map(|p| if stick == Stick::Left { &p.left_stick } else { &p.right_stick });
    let action = cfg.map(|c| &c.action);
    let speed = match action {
        Some(StickAction::Mouse { speed, .. }) => format!("{speed:.0} px/s"),
        Some(StickAction::Scroll { speed, .. }) => format!("{speed:.1} notches/s"),
        _ => "—".into(),
    };
    let invert = on_off(action.is_some_and(invert_of));
    let deadzone = cfg.map_or(0.0, |c| c.deadzone);
    (
        vec![
            item(format!("Action: {}", action.map_or("Disabled", stick_action_name))),
            item(format!("Speed: {speed}")),
            item(format!("Invert Y: {invert}")),
            item(format!("Deadzone: {deadzone:.2}")),
        ],
        "A change · ◀ ▶ adjust · B back",
    )
}

pub fn trigger_page(trigger: Trigger, profile: Option<&Profile>) -> (Vec<ItemView>, &'static str) {
    let cfg = profile.map(|p| if trigger == Trigger::Left { &p.left_trigger } else { &p.right_trigger });
    let action = cfg.map(|c| &c.action);
    let (acts, threshold) = match action {
        Some(TriggerAction::Button { action, threshold }) => (action.summary(), format!("{threshold:.2}")),
        _ => ("—".into(), "—".into()),
    };
    (
        vec![
            item(format!("Action: {}", action.map_or("Disabled", trigger_action_name))),
            item(format!("Acts as: {acts}")),
            item(format!("Press at: {threshold}")),
        ],
        "A change · ◀ ▶ adjust · B back",
    )
}

pub fn gyro_page(profile: Option<&Profile>) -> (Vec<ItemView>, &'static str) {
    let gyro = profile.map(|p| &p.gyro);
    let mode = gyro.map(|g| &g.mode);
    let sensitivity = match mode {
        Some(GyroMode::Mouse { sensitivity }) => format!("{sensitivity:.1} px/°"),
        _ => "—".into(),
    };
    (
        vec![
            item(format!("Mode: {}", mode.map_or("Off", gyro_mode_name))),
            item(format!("Sensitivity: {sensitivity}")),
            item(format!("Invert X: {}", on_off(gyro.is_some_and(|g| g.invert_x)))),
            item(format!("Invert Y: {}", on_off(gyro.is_some_and(|g| g.invert_y)))),
            item(format!("Noise threshold: {:.1}", gyro.map_or(0.0, |g| g.noise_threshold))),
            item(format!("Activation: {}", gyro.map_or("always".into(), |g| activation_name(g.activation)))),
        ],
        "A change · ◀ ▶ adjust · B back",
    )
}

pub fn side(left: bool) -> &'static str {
    if left { "Left" } else { "Right" }
}

fn activation_name(activation: GyroActivation) -> String {
    let input = |i: GyroInput| match i {
        GyroInput::Button(b) => b.short_name().to_string(),
        GyroInput::LeftTrigger => "LT".into(),
        GyroInput::RightTrigger => "RT".into(),
    };
    match activation {
        GyroActivation::Always => "always".into(),
        GyroActivation::WhileHeld(i) => format!("while {} held", input(i)),
        GyroActivation::UnlessHeld(i) => format!("off while {} held", input(i)),
        GyroActivation::Toggle(i) => format!("toggle with {}", input(i)),
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

fn stick_action_name(action: &StickAction) -> &'static str {
    match action {
        StickAction::Disabled => "Disabled",
        StickAction::Gamepad { .. } => "Gamepad stick",
        StickAction::Mouse { .. } => "Mouse",
        StickAction::Scroll { .. } => "Scroll",
        _ => "Other (edit in the pack file)",
    }
}

fn trigger_action_name(action: &TriggerAction) -> &'static str {
    match action {
        TriggerAction::Disabled => "Disabled",
        TriggerAction::Gamepad(_) => "Gamepad trigger",
        TriggerAction::Button { .. } => "Button",
    }
}

fn gyro_mode_name(mode: &GyroMode) -> &'static str {
    match mode {
        GyroMode::Off => "Off",
        GyroMode::Mouse { .. } => "Mouse",
        GyroMode::Stick { .. } => "Stick (edit in the pack file)",
        GyroMode::Steering { .. } => "Steering (edit in the pack file)",
    }
}

fn invert_of(action: &StickAction) -> bool {
    match action {
        StickAction::Gamepad { invert_y, .. } | StickAction::Mouse { invert_y, .. } | StickAction::Scroll { invert_y, .. } => *invert_y,
        _ => false,
    }
}

/// Flips a stick's Y inversion. False for kinds that have none.
fn toggle_invert(action: &mut StickAction) -> bool {
    match action {
        StickAction::Gamepad { invert_y, .. } | StickAction::Mouse { invert_y, .. } | StickAction::Scroll { invert_y, .. } => {
            *invert_y = !*invert_y;
            true
        }
        _ => false,
    }
}

/// The next stick action in the cycle Disabled → Gamepad → Mouse → Scroll → Disabled. Kinds the
/// editor doesn't cycle through (rings, flick) stay as they are.
fn next_stick_action(action: &StickAction, stick: Stick) -> StickAction {
    match action {
        StickAction::Disabled => StickAction::Gamepad { stick, invert_y: false },
        StickAction::Gamepad { .. } => StickAction::Mouse { speed: MOUSE_SPEED, response: Default::default(), invert_y: false },
        StickAction::Mouse { .. } => StickAction::Scroll { speed: SCROLL_SPEED, invert_y: false },
        StickAction::Scroll { .. } => StickAction::Disabled,
        other => other.clone(),
    }
}

/// The next trigger action: Disabled → Gamepad trigger → Button → Disabled.
fn next_trigger_action(action: &TriggerAction, trigger: Trigger) -> TriggerAction {
    match action {
        TriggerAction::Disabled => TriggerAction::Gamepad(trigger),
        TriggerAction::Gamepad(_) => TriggerAction::Button { action: ButtonAction::Gamepad(Button::South), threshold: 0.5 },
        TriggerAction::Button { .. } => TriggerAction::Disabled,
    }
}

/// The next pad button a trigger acts as, from `TRIGGER_BUTTONS`.
fn next_trigger_button(action: &ButtonAction) -> ButtonAction {
    let current = match action {
        ButtonAction::Gamepad(b) => TRIGGER_BUTTONS.iter().position(|x| x == b),
        _ => None,
    };
    let next = current.map_or(0, |i| (i + 1) % TRIGGER_BUTTONS.len());
    ButtonAction::Gamepad(TRIGGER_BUTTONS[next])
}

/// Changes one stick of the active profile. `change` says whether it did change anything.
fn edit_stick(config: &mut crate::config::Config, stick: Stick, change: impl FnOnce(&mut StickConfig) -> bool) -> bool {
    let Some(profile) = active_profile_mut(config) else { return false };
    let cfg = if stick == Stick::Left { &mut profile.left_stick } else { &mut profile.right_stick };
    change(cfg)
}

fn edit_trigger(config: &mut crate::config::Config, trigger: Trigger, change: impl FnOnce(&mut TriggerConfig) -> bool) -> bool {
    let Some(profile) = active_profile_mut(config) else { return false };
    let cfg = if trigger == Trigger::Left { &mut profile.left_trigger } else { &mut profile.right_trigger };
    change(cfg)
}

fn edit_gyro(config: &mut crate::config::Config, change: impl FnOnce(&mut GyroConfig) -> bool) -> bool {
    let Some(profile) = active_profile_mut(config) else { return false };
    change(&mut profile.gyro)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn a_stick_cycles_its_action_and_adjusts_speed_and_deadzone() {
        let mut config = Config::default();
        assert_eq!(stick_choose(Stick::Left, STICK_ACTION, &mut config), EditStep::Changed);
        let action = config.active().unwrap().left_stick.action.clone();
        assert!(matches!(action, StickAction::Mouse { speed, .. } if speed == MOUSE_SPEED));
        assert_eq!(stick_adjust(Stick::Left, STICK_SPEED, 1, &mut config), EditStep::Changed);
        assert!(matches!(config.active().unwrap().left_stick.action, StickAction::Mouse { speed, .. } if speed > MOUSE_SPEED));
        for _ in 0..20 {
            stick_adjust(Stick::Left, STICK_DEADZONE, -1, &mut config);
        }
        assert_eq!(config.active().unwrap().left_stick.deadzone, 0.0);
        stick_choose(Stick::Left, STICK_INVERT, &mut config);
        assert!(matches!(config.active().unwrap().left_stick.action, StickAction::Mouse { invert_y: true, .. }));
    }

    #[test]
    fn a_stick_in_a_mode_the_editor_does_not_cycle_is_left_alone() {
        let mut config = Config::default();
        let ring = StickAction::Ring { sectors: 4, start_angle: 0.0, inner_radius: 0.3, hysteresis: 0.1, actions: Vec::new() };
        active_profile_mut(&mut config).unwrap().right_stick.action = ring.clone();
        assert_eq!(stick_choose(Stick::Right, STICK_ACTION, &mut config), EditStep::Stay);
        assert_eq!(config.active().unwrap().right_stick.action, ring);
    }

    #[test]
    fn stick_cycle_wraps_back_to_disabled() {
        let mut action = StickAction::Disabled;
        let mut seen = Vec::new();
        for _ in 0..4 {
            action = next_stick_action(&action, Stick::Right);
            seen.push(stick_action_name(&action));
        }
        assert_eq!(seen, ["Gamepad stick", "Mouse", "Scroll", "Disabled"]);
    }

    #[test]
    fn a_trigger_can_act_as_a_button_at_a_threshold() {
        let mut config = Config::default();
        active_profile_mut(&mut config).unwrap().left_trigger.action = TriggerAction::Disabled;
        trigger_choose(Trigger::Left, TRIGGER_ACTION, &mut config); // Gamepad trigger
        trigger_choose(Trigger::Left, TRIGGER_ACTION, &mut config); // Button
        assert!(matches!(config.active().unwrap().left_trigger.action, TriggerAction::Button { threshold, .. } if threshold == 0.5));
        trigger_adjust(Trigger::Left, TRIGGER_THRESHOLD, 1, &mut config);
        trigger_choose(Trigger::Left, TRIGGER_ACTS, &mut config);
        // The threshold is still the one it was adjusted to.
        assert!(matches!(
            &config.active().unwrap().left_trigger.action,
            TriggerAction::Button { action: ButtonAction::Gamepad(Button::East), threshold } if (threshold - 0.55).abs() < 1e-4
        ));
    }

    #[test]
    fn gyro_mouse_mode_turns_on_and_off_and_takes_a_sensitivity() {
        let mut config = Config::default();
        gyro_choose(GYRO_MODE, &mut config);
        assert_eq!(config.active().unwrap().gyro.mode, GyroMode::Mouse { sensitivity: GYRO_MOUSE_SENSITIVITY });
        gyro_adjust(GYRO_SENSITIVITY, -1, &mut config);
        assert!(matches!(config.active().unwrap().gyro.mode, GyroMode::Mouse { sensitivity } if sensitivity < GYRO_MOUSE_SENSITIVITY));
        gyro_choose(GYRO_MODE, &mut config);
        assert_eq!(config.active().unwrap().gyro.mode, GyroMode::Off);
    }
}

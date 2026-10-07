use std::time::{Duration, Instant};

use super::{
    view::{current_input, sequences},
    *,
};
use crate::{
    config::{InputLogSettings, LogEnd, OverlayStyle},
    info::PadFamily,
};

const GAP: Duration = Duration::from_millis(250);

fn ms(t0: Instant, n: u64) -> Instant {
    t0 + Duration::from_millis(n)
}

fn button(log: &mut InputLog, b: Button, pressed: bool, at: Instant) {
    log.event(InputEvent::Button(b, pressed), Thresholds::default(), at);
}

fn axis(log: &mut InputLog, a: Axis, v: f32, at: Instant) {
    log.event(InputEvent::Axis(a, v), Thresholds::default(), at);
}

fn inputs(log: &InputLog) -> Vec<Input> {
    log.entries().iter().map(|e| e.input.clone()).collect()
}

fn dpad(d: Dir) -> Input {
    Input::Direction(None, d)
}

#[test]
fn dpad_motion_reads_as_diagonals() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    button(&mut log, Button::DpadDown, true, t0);
    button(&mut log, Button::DpadRight, true, ms(t0, 30));
    button(&mut log, Button::DpadDown, false, ms(t0, 60));
    button(&mut log, Button::West, true, ms(t0, 80));
    button(&mut log, Button::DpadRight, false, ms(t0, 100));
    button(&mut log, Button::West, false, ms(t0, 200));
    assert_eq!(inputs(&log), [dpad(Dir::Down), dpad(Dir::DownRight), dpad(Dir::Right), Input::Button(Button::West)]);
    let e = log.entries();
    assert_eq!(e[0].released, Some(ms(t0, 30)), "a direction ends when the pad moves on");
    assert_eq!(e[1].released, Some(ms(t0, 60)));
    assert_eq!(sequences(e, GAP).len(), 1);
}

#[test]
fn stick_motion_quantizes_to_eight_directions_with_hysteresis() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    axis(&mut log, Axis::LeftY, 1.0, t0);
    axis(&mut log, Axis::LeftX, 0.8, ms(t0, 20));
    axis(&mut log, Axis::LeftY, 0.0, ms(t0, 40));
    // Wobbling just under the threshold stays put.
    axis(&mut log, Axis::LeftX, 0.52, ms(t0, 50));
    axis(&mut log, Axis::LeftX, 0.0, ms(t0, 60));
    let stick = |d| Input::Direction(Some(Stick::Left), d);
    assert_eq!(inputs(&log), [stick(Dir::Down), stick(Dir::DownRight), stick(Dir::Right)]);
    assert!(log.entries().iter().all(|e| e.released.is_some()), "centering ends the last direction");
}

#[test]
fn a_pause_longer_than_the_gap_splits_sequences() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    button(&mut log, Button::South, true, t0);
    button(&mut log, Button::South, false, ms(t0, 50));
    button(&mut log, Button::East, true, ms(t0, 400));
    button(&mut log, Button::East, false, ms(t0, 450));
    assert_eq!(sequences(log.entries(), GAP).len(), 2);
    assert_eq!(sequences(log.entries(), Duration::from_millis(400)).len(), 1);
}

#[test]
fn a_held_input_keeps_the_sequence_going() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    button(&mut log, Button::LeftBumper, true, t0);
    button(&mut log, Button::South, true, ms(t0, 1000));
    assert_eq!(sequences(log.entries(), GAP).len(), 1);
}

#[test]
fn triggers_press_past_their_threshold_once() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    for (i, v) in [0.3, 0.6, 0.48, 0.7, 0.2].into_iter().enumerate() {
        axis(&mut log, Axis::RightTrigger, v, ms(t0, i as u64 * 10));
    }
    assert_eq!(inputs(&log), [Input::Trigger(Trigger::Right)]);
    assert_eq!(log.entries()[0].released, Some(ms(t0, 40)));
}

#[test]
fn actions_label_their_press_and_pass_through_gets_none() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    button(&mut log, Button::South, true, t0);
    let from = FiredFrom::Button(Button::South);
    log.attach(&from, label_for(&from, &ButtonAction::Keys(vec!["KEY_SPACE".into()])));
    button(&mut log, Button::East, true, ms(t0, 10));
    let from = FiredFrom::Button(Button::East);
    log.attach(&from, label_for(&from, &ButtonAction::Gamepad(Button::East)));
    assert_eq!(log.entries()[0].label.as_deref(), Some("Space"));
    assert_eq!(log.entries()[1].label, None);
}

#[test]
fn a_late_gesture_labels_the_released_press() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    button(&mut log, Button::North, true, t0);
    button(&mut log, Button::North, false, ms(t0, 80));
    log.attach(&FiredFrom::Button(Button::North), Some("Reload".into()));
    assert_eq!(log.entries()[0].label.as_deref(), Some("Reload"));
}

#[test]
fn a_combo_becomes_one_entry_released_with_its_last_member() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    button(&mut log, Button::LeftBumper, true, t0);
    button(&mut log, Button::West, true, ms(t0, 20));
    log.attach(&FiredFrom::Combo(vec![Button::LeftBumper, Button::West]), Some("Heal".into()));
    assert_eq!(inputs(&log), [Input::Combo(vec![Button::LeftBumper, Button::West])]);
    assert_eq!(log.entries()[0].label.as_deref(), Some("Heal"));
    button(&mut log, Button::West, false, ms(t0, 100));
    assert_eq!(log.entries()[0].released, None);
    button(&mut log, Button::LeftBumper, false, ms(t0, 150));
    assert_eq!(log.entries()[0].released, Some(ms(t0, 150)));
}

#[test]
fn dpad_actions_label_the_direction_holding_them() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    button(&mut log, Button::DpadDown, true, t0);
    button(&mut log, Button::DpadRight, true, ms(t0, 20));
    log.attach(&FiredFrom::Button(Button::DpadRight), Some("Dash".into()));
    assert_eq!(log.entries()[1].input, dpad(Dir::DownRight));
    assert_eq!(log.entries()[1].label.as_deref(), Some("Dash"));
}

#[test]
fn repeats_merge_with_their_average_hold() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    for (i, hold) in [40, 60, 80].into_iter().enumerate() {
        let at = i as u64 * 100;
        button(&mut log, Button::South, true, ms(t0, at));
        button(&mut log, Button::South, false, ms(t0, at + hold));
    }
    button(&mut log, Button::East, true, ms(t0, 300));
    let view = log_view(log.entries(), &InputLogSettings::default(), &OverlayStyle::log(), PadFamily::Xbox, ms(t0, 310));
    let cells = &view.lines[0].cells;
    assert_eq!((cells[0].count, cells[0].hold_ms, cells[0].held), (3, Some(60), false));
    assert_eq!((cells[1].count, cells[1].hold_ms, cells[1].held), (1, None, true));
    let apart = InputLogSettings { merge_repeats: false, ..InputLogSettings::default() };
    let view = log_view(log.entries(), &apart, &OverlayStyle::log(), PadFamily::Xbox, ms(t0, 310));
    assert_eq!(view.lines[0].cells.len(), 4);
}

#[test]
fn log_lines_keep_the_newest_and_fade_after_their_end() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    for i in 0..5 {
        button(&mut log, Button::South, true, ms(t0, i * 1000));
        button(&mut log, Button::South, false, ms(t0, i * 1000 + 50));
    }
    let s = InputLogSettings { lines: 3, fade_after: 2.0, ..InputLogSettings::default() };
    let now = ms(t0, 4100);
    let view = log_view(log.entries(), &s, &OverlayStyle::log(), PadFamily::Xbox, now);
    // Lines ended at 2.05 s (gone at 4.05), 3.05 s and 4.05 s; newest on top.
    assert_eq!(view.lines.len(), 2);
    let bottom = InputLogSettings { newest: LogEnd::Bottom, ..s };
    let view = log_view(log.entries(), &bottom, &OverlayStyle::log(), PadFamily::Xbox, ms(t0, 3700));
    assert_eq!(view.lines.len(), 3);
    assert!(view.lines[0].opacity < view.lines[2].opacity, "the oldest line, at the top, is fading");
}

#[test]
fn current_input_shows_the_latest_sequence_until_it_has_stayed() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    button(&mut log, Button::DpadDown, true, t0);
    button(&mut log, Button::DpadDown, false, ms(t0, 50));
    button(&mut log, Button::South, true, ms(t0, 60));
    button(&mut log, Button::South, false, ms(t0, 100));
    let stay = Duration::from_secs(1);
    let shown = current_input(log.entries(), GAP, stay, PadFamily::Xbox, ms(t0, 500));
    assert_eq!(shown.len(), 2);
    assert!(current_input(log.entries(), GAP, stay, PadFamily::Xbox, ms(t0, 1350)).is_empty());
    let due = next_change(log.entries(), GAP, GAP + stay, ms(t0, 500), Duration::from_millis(16));
    assert_eq!(due, Some(ms(t0, 1350) - crate::info::FADE_OUT));
}

#[test]
fn history_is_capped() {
    let t0 = Instant::now();
    let mut log = InputLog::default();
    for i in 0..(CAPACITY as u64 + 10) {
        button(&mut log, Button::South, true, ms(t0, i * 10));
        button(&mut log, Button::South, false, ms(t0, i * 10 + 5));
    }
    assert!(log.entries().len() <= CAPACITY);
}

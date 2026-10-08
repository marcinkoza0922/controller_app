//! Timing benchmarks for the daemon's and config's hot paths. They are `#[ignore]`d, so the
//! normal test run skips them; `scripts/bench.sh` runs them and prints one line per case.

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use crate::{
    config::{Button, Config, Game, Profile, ProfileRef},
    engine::Engine,
    input::{Axis, InputEvent},
    ipc::InputSnapshot,
    pack,
    pad_svg::{self, Spot},
};

/// Runs `f` for about `iters` iterations (after a short warm-up) and prints the mean.
#[expect(clippy::print_stdout, reason = "benchmarks report their timings on stdout")]
pub fn bench(name: &str, iters: u32, mut f: impl FnMut(u32)) {
    for i in 0..iters.min(20) {
        f(i);
    }
    let start = Instant::now();
    for i in 0..iters {
        f(i);
    }
    let per = start.elapsed() / iters;
    println!("bench {name:<36} {:>10} /iter  ({iters} iters)", format_duration(per));
}

fn format_duration(d: Duration) -> String {
    let ns = d.as_nanos();
    if ns < 1_000 {
        format!("{ns} ns")
    } else if ns < 1_000_000 {
        format!("{:.2} µs", ns as f64 / 1e3)
    } else {
        format!("{:.3} ms", ns as f64 / 1e6)
    }
}

/// A config with `games` games of four profiles each, the last one active: the worst case
/// for lookups by name.
pub fn busy_config(games: usize) -> Config {
    let mut config = Config::default();
    for g in 0..games {
        let profiles = vec![
            Profile::pc_action("Play"),
            Profile::desktop("Menus"),
            Profile::passthrough("Gamepad"),
            Profile::pc_action("Strategy"),
        ];
        config.games.push(Game::new(&format!("Game {g}"), profiles));
    }
    let last = format!("Game {}", games.saturating_sub(1));
    config.active = ProfileRef::new(Some(&last), "Strategy");
    config
}

#[test]
#[ignore = "benchmark; run with scripts/bench.sh"]
fn bench_engine() {
    let profile = Profile::pc_action("Play");

    let mut engine = Engine::default();
    bench("engine: button press + release", 200_000, |i| {
        let mut out = Vec::new();
        let now = Instant::now();
        let pressed = InputEvent::Button(Button::South, i % 2 == 0);
        black_box(engine.handle(&profile, pressed, now, &mut out));
        black_box(out);
    });

    let mut engine = Engine::default();
    bench("engine: stick move (event)", 200_000, |i| {
        let mut out = Vec::new();
        let x = if i % 2 == 0 { 0.8 } else { -0.8 };
        black_box(engine.handle(&profile, InputEvent::Axis(Axis::LeftX, x), Instant::now(), &mut out));
        black_box(out);
    });

    // The daemon ticks every 4 ms per device, even with nothing pressed.
    let mut engine = Engine::default();
    bench("engine: tick, idle", 200_000, |_| {
        let mut out = Vec::new();
        engine.tick(&profile, 0.004, &mut out);
        black_box(out);
    });

    let mut engine = Engine::default();
    let mut out = Vec::new();
    engine.handle(&profile, InputEvent::Axis(Axis::LeftX, 1.0), Instant::now(), &mut out);
    bench("engine: tick, stick held", 200_000, |_| {
        let mut out = Vec::new();
        engine.tick(&profile, 0.004, &mut out);
        black_box(out);
    });
}

#[test]
#[ignore = "benchmark; run with scripts/bench.sh"]
fn bench_config() {
    let config = busy_config(40);
    bench("config: active() with 40 games", 500_000, |_| {
        black_box(config.active());
    });
    bench("config: active_ref() with 40 games", 500_000, |_| {
        black_box(config.active_ref());
    });

    let game = &config.games[39];
    bench("pack: item_hashes(one game)", 2_000, |_| {
        black_box(pack::item_hashes(game));
    });
}

#[test]
#[ignore = "benchmark; run with scripts/bench.sh"]
fn bench_pad_svg() {
    let labels: Vec<(Spot, String)> = [
        (Spot::Button(Button::South), "Jump"),
        (Spot::Button(Button::East), "Pause"),
        (Spot::Button(Button::LeftBumper), "Next profile"),
        (Spot::Trigger(crate::config::Trigger::Left), "Aim"),
        (Spot::Button(Button::LeftStick), "Sprint"),
    ]
    .into_iter()
    .map(|(s, t)| (s, t.to_string()))
    .collect();
    bench("pad_svg: render (no input)", 20_000, |_| {
        black_box(pad_svg::render(None, &labels));
    });
    bench("pad_svg: render (live input)", 20_000, |i| {
        let snapshot = snapshot_at(i);
        black_box(pad_svg::render(Some(&snapshot), &labels));
    });
}

/// A controller reading with the right stick sweeping around, as during real use.
pub fn snapshot_at(i: u32) -> InputSnapshot {
    let angle = i as f32 * 0.05;
    InputSnapshot {
        device: "Pad".into(),
        buttons: vec![Button::South],
        left_stick: (0.0, 0.0),
        right_stick: (angle.sin(), angle.cos()),
        left_trigger: 0.0,
        right_trigger: 0.0,
        gyro: None,
    }
}

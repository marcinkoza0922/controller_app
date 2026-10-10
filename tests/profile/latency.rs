//! Input latency: from a South press on the controller to the daemon's virtual pad reporting it,
//! with the controller idle and then under a 1 kHz stick flood, as a busy controller would be.
//!
//! The test runs several sessions, each with a fresh daemon, so run-to-run variation shows up in
//! the figures. The samples of all sessions are pooled for the distribution. Each suite run also
//! appends its summary to `history.jsonl`, which the plots use for the trend.

use std::{io::Write, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

use evdev::InputEvent;
use serde_json::{Value, json};

use super::support::{Output, Pad, Rng, Running, latency_stats, now_ns, output_dir, padwight, report, south, wait_managed};
use crate::common::{default_config, install_config, serial, temp_root, TIMEOUT};

/// Presses per condition, per session. At 20 to 60 ms between them, a session takes about 30 s.
const SAMPLES: usize = 300;
/// The gap before each press, in milliseconds: random, so presses don't line up with daemon timers.
const GAP_MS: (u64, u64) = (20, 60);
/// How long to wait for the daemon's output before a press counts as missed.
const WAIT: Duration = Duration::from_secs(2);
/// Sessions per run, overridable with `PADWIGHT_LATENCY_REPEATS`.
const REPEATS: usize = 3;
const NAME: &str = "Profile Controller";

/// The samples and counts of one condition in one session.
#[derive(Default)]
struct Condition {
    samples_ns: Vec<u64>,
    missed: u32,
    seconds: f64,
    reports_per_second: f64,
    output_events_per_second: f64,
    dropped_total: u64,
}

#[test]
#[ignore]
fn profile_input_latency() {
    let _serial = serial();
    let repeats = repeats();
    let mut idle = Condition::default();
    let mut flood = Condition::default();
    let mut sessions = Vec::new();
    for repeat in 0..repeats {
        let (i, f) = session(repeat);
        sessions.push(json!({
            "repeat": repeat,
            "idle": summary(&i),
            "flood_1khz": summary(&f),
        }));
        idle.samples_ns.extend(i.samples_ns);
        idle.missed += i.missed;
        flood.samples_ns.extend(f.samples_ns);
        flood.missed += f.missed;
    }
    let idle_json = condition_json(&idle);
    let flood_json = condition_json(&flood);
    append_history(repeats, &idle_json, &flood_json);

    report(
        "input_latency",
        json!({
            "what": "South press written to the uhid pad, to the kernel time stamp of BTN_SOUTH on the daemon's virtual pad",
            "samples_per_condition_per_session": SAMPLES,
            "sessions": repeats,
            "idle": idle_json,
            "flood_1khz": flood_json,
            "per_session": sessions,
        }),
    );
}

/// Sessions per run: `PADWIGHT_LATENCY_REPEATS` if it is a positive number, else [`REPEATS`].
fn repeats() -> usize {
    std::env::var("PADWIGHT_LATENCY_REPEATS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(REPEATS)
}

/// One session: a fresh daemon, the idle condition, then the flood condition.
fn session(repeat: usize) -> (Condition, Condition) {
    let root = temp_root();
    install_config(&root.join("config/padwight"), &default_config());
    // The pad exists before the daemon starts, as a controller plugged in first would.
    let mut pad = Pad::new(NAME);
    let daemon = Running(padwight(&root, &["daemon"]).spawn().expect("starting the daemon"));
    wait_managed(Instant::now(), &root.join("run"), NAME, TIMEOUT).expect("the daemon grabbed the pad");
    let mut out = Output::open();
    warm_up(&mut pad, &mut out);

    let idle = measure(&mut pad, &mut out, false, seed(repeat, 0));
    let flood = measure(&mut pad, &mut out, true, seed(repeat, 1));

    drop(daemon);
    let _ = std::fs::remove_dir_all(&root);
    (idle, flood)
}

/// A different, non-zero generator seed for each session and condition.
fn seed(repeat: usize, condition: u64) -> u64 {
    let base: u64 = 0x9E37_79B9_7F4A_7C15;
    (base ^ ((repeat as u64 + 1).wrapping_mul(0x1000_0000_01B3)) ^ (condition << 32)).max(1)
}

/// Presses until one comes out, so the daemon is converting the pad before anything is timed.
fn warm_up(pad: &mut Pad, out: &mut Output) {
    for _ in 0..10 {
        pad.set_south(true);
        let seen = out.wait(Duration::from_millis(300), |ev| south(ev, 1), || pad.tick());
        pad.set_south(false);
        out.wait(Duration::from_millis(300), |ev| south(ev, 0), || pad.tick());
        if seen.is_some() {
            return;
        }
    }
    panic!("the daemon never output a South press from the uhid pad");
}

/// One condition: press South [`SAMPLES`] times, timing each press from the write to the output.
/// With `flood`, the stick is swept at 1 kHz the whole time, including during each wait.
fn measure(pad: &mut Pad, out: &mut Output, flood: bool, seed: u64) -> Condition {
    pad.flood = flood;
    let mut rng = Rng(seed);
    let mut result = Condition::default();
    let sent_before = pad.sent;
    let events_before = out.events;
    let dropped_before = out.dropped;
    let started = Instant::now();
    for _ in 0..SAMPLES {
        let gap = Duration::from_millis(rng.range(GAP_MS.0, GAP_MS.1));
        out.wait(gap, |_: &InputEvent| false, || pad.tick());
        let sent = now_ns();
        pad.set_south(true);
        match out.wait(WAIT, |ev| south(ev, 1), || pad.tick()) {
            Some(seen) => result.samples_ns.push(seen.saturating_sub(sent)),
            None => result.missed += 1,
        }
        pad.set_south(false);
        out.wait(WAIT, |ev| south(ev, 0), || pad.tick());
    }
    pad.flood = false;
    result.seconds = started.elapsed().as_secs_f64();
    result.reports_per_second = (pad.sent - sent_before) as f64 / result.seconds;
    result.output_events_per_second = (out.events - events_before) as f64 / result.seconds;
    result.dropped_total = out.dropped - dropped_before;
    result
}

/// Latency summary and counts of one condition, with its raw samples in microseconds.
fn condition_json(c: &Condition) -> Value {
    let samples_us: Vec<f64> = c.samples_ns.iter().map(|ns| *ns as f64 / 1000.0).collect();
    json!({
        "latency": latency_stats(c.samples_ns.clone()),
        "samples_us": samples_us,
        "missed": c.missed,
        "seconds": c.seconds,
        "reports_per_second": c.reports_per_second,
        "output_events_per_second": c.output_events_per_second,
        "dropped_output_events": c.dropped_total,
    })
}

/// Per-session figures without the samples.
fn summary(c: &Condition) -> Value {
    let stats = latency_stats(c.samples_ns.clone());
    json!({ "p50_us": stats["p50_us"], "p90_us": stats["p90_us"], "p99_us": stats["p99_us"], "max_us": stats["max_us"], "missed": c.missed })
}

/// Appends one line to `history.jsonl` in the output directory: the pooled p50 and p99 of each
/// condition, with the time and commit, so the trend across suite runs can be plotted.
fn append_history(sessions: usize, idle: &Value, flood: &Value) {
    let line = json!({
        "time": SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        "commit": super::support::commit(),
        "sessions": sessions,
        "idle_p50_us": idle["latency"]["p50_us"],
        "idle_p99_us": idle["latency"]["p99_us"],
        "flood_p50_us": flood["latency"]["p50_us"],
        "flood_p99_us": flood["latency"]["p99_us"],
    });
    let dir = output_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("history.jsonl")).unwrap();
    writeln!(file, "{line}").unwrap();
}

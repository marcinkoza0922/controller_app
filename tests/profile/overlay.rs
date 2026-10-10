//! The on-screen keyboard. The daemon part: how long it takes from a request (or a stick push) to
//! the next overlay frame reaching subscribers, for opening, moving the cursor, and closing. It
//! runs everywhere. The window part needs a Wayland session, since the overlay is a layer-shell
//! surface: how much the overlay process uses while hidden and while navigating, and how long it
//! takes from receiving a frame to having built what it draws.
//!
//! The window part shows the keyboard on the real screen for a few seconds, and moves its cursor.

use std::{
    fs,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::support::{Monitor, Pad, Running, Watcher, latency_stats, now_ns, padwight, report, send_request, wait_managed};
use crate::common::{default_config, install_config, serial, temp_root, TIMEOUT};

/// Open, move and close, this many times. The first cycle is a warm-up and is not counted.
const CYCLES: usize = 40;
/// How long a frame is waited for before it counts as missed.
const WAIT: Duration = Duration::from_secs(2);
/// The stick pushed as far right as it goes; the overlay moves its cursor once per push.
const PUSH: i16 = i16::MAX;
/// How long the stick rests after a push, so the next one is a new push.
const REST: Duration = Duration::from_millis(150);
/// Cursor moves made while the window is shown, and how long the window is watched when hidden.
const SHOWN_MOVES: usize = 20;
const HIDDEN_FOR: Duration = Duration::from_secs(5);
const NAME: &str = "Profile Controller";

/// Times for one step, in nanoseconds, and how many steps never produced a frame.
#[derive(Default)]
struct Steps {
    open: Vec<u64>,
    navigate: Vec<u64>,
    close: Vec<u64>,
    missed: u32,
}

#[test]
#[ignore]
fn profile_overlay() {
    let _serial = serial();
    let root = temp_root();
    install_config(&root.join("config/padwight"), &default_config());
    let runtime = root.join("run");
    let timing_log = root.join("overlay-timing.log");
    let mut pad = Pad::new(NAME);
    // The overlay process inherits the daemon's environment, which names the timing log. The
    // compositor's socket is in the session's runtime directory, which `padwight()` replaces, so
    // the overlay is given the socket's full path.
    let mut command = padwight(&root, &["daemon"]);
    command.env("PADWIGHT_OVERLAY_TIMING", &timing_log);
    if let (Some(dir), Some(display)) = (std::env::var_os("XDG_RUNTIME_DIR"), std::env::var_os("WAYLAND_DISPLAY")) {
        command.env("WAYLAND_DISPLAY", std::path::Path::new(&dir).join(display));
    }
    let daemon = Running(command.spawn().expect("starting the daemon"));
    wait_managed(Instant::now(), &runtime, NAME, TIMEOUT).expect("the daemon grabbed the pad");
    let mut watch = Watcher::connect(&runtime);

    let mut steps = Steps::default();
    for cycle in 0..=CYCLES {
        let counted = cycle > 0;
        let one = cycle_once(&mut pad, &mut watch, &runtime);
        if counted {
            steps.open.extend(one.open);
            steps.navigate.extend(one.navigate);
            steps.close.extend(one.close);
            steps.missed += one.missed;
        }
    }
    let daemon_side = json!({
        "open": step_json(&steps.open),
        "navigate": step_json(&steps.navigate),
        "close": step_json(&steps.close),
        "missed": steps.missed,
    });

    let window = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        window(&daemon, &runtime, &timing_log, &mut pad, &mut watch)
    } else {
        json!("skipped: no Wayland display, and the overlay is a layer-shell surface")
    };
    drop(daemon);
    let _ = fs::remove_dir_all(&root);

    report(
        "overlay",
        json!({
            "what": "request (or stick push) written, to the next overlay frame arriving on the daemon's WatchOverlay subscription",
            "cycles": CYCLES,
            "daemon_response": daemon_side,
            "window": window,
        }),
    );
}

#[derive(Default)]
struct Cycle {
    open: Option<u64>,
    navigate: Option<u64>,
    close: Option<u64>,
    missed: u32,
}

/// One open, cursor move and close. Each step's time runs from its request or push to its frame.
fn cycle_once(pad: &mut Pad, watch: &mut Watcher, runtime: &std::path::Path) -> Cycle {
    let mut cycle = Cycle::default();
    let sent = now_ns();
    send_request(runtime, "\"ToggleOverlay\"");
    cycle.open = step(watch.wait_for(WAIT, |f| !f["active"].is_null()), sent, &mut cycle.missed);

    let before = watch.active.clone();
    let sent = now_ns();
    pad.set_x(PUSH);
    cycle.navigate = step(watch.wait_for(WAIT, |f| f["active"] != before), sent, &mut cycle.missed);
    pad.set_x(0);
    watch.wait_for(REST, |_| false);

    let sent = now_ns();
    send_request(runtime, "\"ToggleOverlay\"");
    cycle.close = step(watch.wait_for(WAIT, |f| f["active"].is_null()), sent, &mut cycle.missed);
    cycle
}

/// The time from `sent` to the frame, or a missed step.
fn step(arrived: Option<u64>, sent: u64, missed: &mut u32) -> Option<u64> {
    match arrived {
        Some(at) => Some(at.saturating_sub(sent)),
        None => {
            *missed += 1;
            None
        }
    }
}

/// One step's latency summary, with its raw samples in microseconds, so a slow tail can be counted.
fn step_json(samples_ns: &[u64]) -> Value {
    let samples_us: Vec<f64> = samples_ns.iter().map(|ns| *ns as f64 / 1000.0).collect();
    let mut stats = latency_stats(samples_ns.to_vec());
    stats["samples_us"] = json!(samples_us);
    stats
}

/// The overlay window: its CPU and memory while hidden and while the cursor moves, and the time it
/// takes from a frame arriving to the widgets for it being built. The timing log is cleared before
/// the shown phase, so only that phase's draws are counted.
fn window(daemon: &Running, runtime: &std::path::Path, timing_log: &std::path::Path, pad: &mut Pad, watch: &mut Watcher) -> Value {
    let process = wait_for_overlay(daemon.pid()).expect("the daemon started the overlay process");
    // Shown, with the cursor moving: the usual use of the window.
    send_request(runtime, "\"ToggleOverlay\"");
    watch.wait_for(WAIT, |f| !f["active"].is_null());
    let _ = fs::write(timing_log, "");
    let shown = Monitor::start(process);
    for _ in 0..SHOWN_MOVES {
        let before = watch.active.clone();
        pad.set_x(PUSH);
        watch.wait_for(WAIT, |f| f["active"] != before);
        pad.set_x(0);
        watch.wait_for(REST, |_| false);
    }
    let shown_usage = shown.finish();
    let draws = draw_stats(&fs::read_to_string(timing_log).unwrap_or_default());

    // Hidden: the process stays up as a small surface, so the next open is quick.
    send_request(runtime, "\"ToggleOverlay\"");
    watch.wait_for(WAIT, |f| f["active"].is_null());
    let hidden = Monitor::start(process);
    watch.wait_for(HIDDEN_FOR, |_| false);
    let hidden_usage = hidden.finish();

    json!({
        "shown_while_moving": shown_usage,
        "hidden": hidden_usage,
        "draw": draws,
    })
}

/// Waits for the daemon's overlay child process to exist, and returns its pid.
fn wait_for_overlay(daemon: u32) -> Option<u32> {
    let started = Instant::now();
    loop {
        if let Some(pid) = overlay_pid(daemon) {
            return Some(pid);
        }
        if started.elapsed() > TIMEOUT {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The child of `daemon` whose command line is `padwight overlay`.
fn overlay_pid(daemon: u32) -> Option<u32> {
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
        let Some(after_name) = stat.rfind(')').map(|i| &stat[i + 2..]) else { continue };
        let parent = after_name.split_whitespace().nth(1).and_then(|p| p.parse::<u32>().ok());
        let Ok(cmdline) = fs::read(format!("/proc/{pid}/cmdline")) else { continue };
        let args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
        if parent == Some(daemon) && args.contains(&b"overlay".as_slice()) {
            return Some(pid);
        }
    }
    None
}

/// Frame-to-widgets times from the overlay's timing log. `received_to_built` runs from a frame
/// arriving to the next build of the widgets, which includes waiting for the window to redraw.
/// `build` is how long one build takes.
fn draw_stats(log: &str) -> Value {
    let mut received: Option<u128> = None;
    let mut to_built = Vec::new();
    let mut build = Vec::new();
    let mut frames = 0_u32;
    for line in log.lines() {
        let mut parts = line.split_whitespace();
        match (parts.next(), parts.next().and_then(|t| t.parse::<u128>().ok())) {
            (Some("recv"), Some(at)) => {
                frames += 1;
                received = Some(at);
            }
            (Some("built"), Some(at)) => {
                if let Some(start) = received.take() {
                    to_built.push(at.saturating_sub(start) as u64);
                }
                if let Some(ns) = parts.next().and_then(|t| t.parse::<u64>().ok()) {
                    build.push(ns);
                }
            }
            _ => {}
        }
    }
    json!({
        "frames": frames,
        "received_to_built": latency_stats(to_built),
        "build": latency_stats(build),
    })
}

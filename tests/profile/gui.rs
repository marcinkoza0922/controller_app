//! Settings window start-up with the daemon already running: from `padwight gui` to the window
//! being mapped, which `xwininfo` reads from the X server. Then how much the window uses once it
//! is up.
//!
//! The window is looked for on X11, so `WAYLAND_DISPLAY` is removed for the window and it runs
//! through XWayland (or Xvfb). That measures the X11 start-up path, not the native Wayland one.
//! Only one Padwight window may be open while this runs, since the match is by title.

use std::{
    process::Command,
    time::{Duration, Instant},
};

use serde_json::json;

use super::support::{Monitor, Running, elapsed_until, padwight, report, status, summarize_ms};
use crate::common::{default_config, install_config, serial, temp_root};

const RUNS: usize = 5;
/// Longest a window may take to appear, or to go away after the window is closed.
const LIMIT: Duration = Duration::from_secs(30);
/// How long the window is watched once it is up.
const SETTLE: Duration = Duration::from_secs(3);
const TITLE: &str = "Padwight";

#[test]
#[ignore]
fn profile_gui_startup() {
    let _serial = serial();
    if std::env::var_os("DISPLAY").is_none() {
        eprintln!("profile_gui_startup skipped: no X display (run it under xvfb-run or on a desktop with XWayland)");
        return;
    }
    let root = temp_root();
    install_config(&root.join("config/padwight"), &default_config());
    let runtime = root.join("run");
    let _daemon = Running(padwight(&root, &["daemon"]).spawn().expect("starting the daemon"));
    crate::common::wait_for("the daemon's socket", || status(&runtime).map(|_| ()));

    let runs: Vec<_> = (0..RUNS).map(|_| run_once(&root)).collect();

    let shown: Vec<f64> = runs.iter().filter_map(|r| r.shown_ms).collect();
    let _ = std::fs::remove_dir_all(&root);
    report(
        "gui_startup",
        json!({
            "what": "padwight gui to the window titled \"Padwight\" in the X server, with the daemon already running",
            "window_ms": summarize_ms(shown),
            "runs": runs.iter().map(|r| json!({ "window_ms": r.shown_ms, "usage_while_up": r.usage })).collect::<Vec<_>>(),
        }),
    );
}

struct GuiRun {
    shown_ms: Option<f64>,
    usage: serde_json::Value,
}

/// Opens the window, watches it for [`SETTLE`], closes it, and waits for it to go.
fn run_once(root: &std::path::Path) -> GuiRun {
    let mut command = padwight(root, &["gui"]);
    command.env_remove("WAYLAND_DISPLAY");
    let start = Instant::now();
    let gui = Running(command.spawn().expect("starting the settings window"));
    let shown = elapsed_until(start, LIMIT, window_shown);
    let monitor = Monitor::start(gui.pid());
    std::thread::sleep(SETTLE);
    let usage = monitor.finish();
    drop(gui);
    let _ = elapsed_until(Instant::now(), LIMIT, || !window_shown());
    GuiRun { shown_ms: shown.map(|d| d.as_secs_f64() * 1000.0), usage }
}

/// True when the X server lists a top-level window with [`TITLE`].
fn window_shown() -> bool {
    let Ok(tree) = Command::new("xwininfo").args(["-root", "-tree"]).output() else { return false };
    String::from_utf8_lossy(&tree.stdout).contains(&format!("\"{TITLE}\""))
}

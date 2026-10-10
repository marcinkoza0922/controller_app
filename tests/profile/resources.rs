//! CPU and memory use of the daemon with a controller attached: idle, then under a 1 kHz stick
//! flood for a while. Memory growth across the flood is the leak check. Also records the size of
//! the daemon binary and how much its log grew.

use std::{fs, time::{Duration, Instant}};

use serde_json::json;

use super::support::{Monitor, Output, Pad, Running, padwight, report, wait_managed};
use crate::common::{default_config, install_config, serial, temp_root, TIMEOUT};

const NAME: &str = "Profile Controller";
const IDLE: Duration = Duration::from_secs(10);
const FLOOD: Duration = Duration::from_secs(10);

#[test]
#[ignore]
fn profile_resources() {
    let _serial = serial();
    let root = temp_root();
    install_config(&root.join("config/padwight"), &default_config());
    let mut pad = Pad::new(NAME);
    let daemon = Running(padwight(&root, &["daemon"]).spawn().expect("starting the daemon"));
    wait_managed(Instant::now(), &root.join("run"), NAME, TIMEOUT).expect("the daemon grabbed the pad");
    let mut out = Output::open();

    // Let start-up work finish, so idle is idle.
    out.wait(Duration::from_secs(2), |_| false, || pad.tick());
    let idle = Monitor::start(daemon.pid());
    out.wait(IDLE, |_| false, || pad.tick());
    let idle = idle.finish();

    pad.flood = true;
    let events_before = out.events;
    let dropped_before = out.dropped;
    let sent_before = pad.sent;
    let flooding = Monitor::start(daemon.pid());
    let started = Instant::now();
    out.wait(FLOOD, |_| false, || pad.tick());
    let seconds = started.elapsed().as_secs_f64();
    let flood = flooding.finish();
    pad.flood = false;

    let flood_io = json!({
        "reports_per_second": (pad.sent - sent_before) as f64 / seconds,
        "output_events_per_second": (out.events - events_before) as f64 / seconds,
        "dropped_output_events": out.dropped - dropped_before,
    });
    let log_bytes = fs::metadata(root.join("daemon.log")).map_or(0, |m| m.len());
    drop(daemon);
    let _ = fs::remove_dir_all(&root);

    report(
        "resources",
        json!({
            "daemon_binary_bytes": fs::metadata(env!("CARGO_BIN_EXE_padwight")).map_or(0, |m| m.len()),
            "idle": idle,
            "flood_1khz": { "usage": flood, "io": flood_io },
            "daemon_log_bytes": log_bytes,
        }),
    );
}

//! Daemon start-up. A cold start is a first launch: no config, no state, and the binary and its
//! libraries evicted from the page cache. A warm start is a later launch: the config from the
//! first one is there and the binary is cached. Each start is timed to the daemon answering on
//! its socket, and to it grabbing a controller that was already plugged in.

use std::{path::{Path, PathBuf}, time::{Duration, Instant}};

use serde_json::json;

use super::support::{Pad, Running, evict, binary_and_libraries, elapsed_until, padwight, report, status, summarize_ms, wait_managed};
use crate::common::{serial, temp_root};

const RUNS: usize = 5;
/// Longest a start may take before the test gives up on it.
const LIMIT: Duration = Duration::from_secs(30);
const NAME: &str = "Profile Controller";

/// Times from the launch: when the daemon answered on its socket, and when it grabbed the pad.
struct Start {
    socket: Duration,
    managed: Option<Duration>,
}

#[test]
#[ignore]
fn profile_daemon_startup() {
    let _serial = serial();
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_padwight"));
    let files = binary_and_libraries(&binary);

    let cold: Vec<Start> = (0..RUNS)
        .map(|_| {
            let root = temp_root();
            for file in &files {
                evict(file);
            }
            launch(&root)
        })
        .collect();

    // One launch to create the config and warm the cache, then the runs that count.
    let root = temp_root();
    launch(&root);
    let warm: Vec<Start> = (0..RUNS).map(|_| launch(&root)).collect();
    let _ = std::fs::remove_dir_all(&root);

    report(
        "daemon_startup",
        json!({
            "what": "launch to the daemon answering Status on its socket, and to the pad it was given at launch being managed",
            "cold": summarize(&cold),
            "warm": summarize(&warm),
        }),
    );
}

/// Starts the daemon in `root` with a pad plugged in, and stops it again.
fn launch(root: &Path) -> Start {
    // Created before the clock starts: the pad was already plugged in, so only the daemon is timed.
    let _pad = Pad::new(NAME);
    let runtime = root.join("run");
    let start = Instant::now();
    let daemon = Running(padwight(root, &["daemon"]).spawn().expect("starting the daemon"));
    let socket = elapsed_until(start, LIMIT, || status(&runtime).is_some()).expect("the daemon answers on its socket");
    let managed = wait_managed(start, &runtime, NAME, LIMIT);
    drop(daemon);
    Start { socket, managed }
}

fn summarize(starts: &[Start]) -> serde_json::Value {
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let socket = summarize_ms(starts.iter().map(|s| ms(s.socket)).collect());
    let managed = summarize_ms(starts.iter().filter_map(|s| s.managed.map(ms)).collect());
    json!({ "socket": socket, "pad_managed": managed })
}

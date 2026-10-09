//! How the running daemon behaves while a controller sits idle: its CPU use (H-7) and its log, which
//! must have no panics or errors once it has stopped (I-8). The controller is made with uhid, so the
//! daemon sees a real HID device.
//!
//! Needs /dev/uinput and /dev/uhid, and read access to /dev/input, so it is opt-in:
//! `cargo test --test daemon_health -- --ignored`, or `scripts/kernel-test-docker.sh idle_`.

mod common;

use std::{
    fs,
    thread,
    time::{Duration, Instant},
};

use common::{
    assert_log_clean, serial, spawn_daemon, temp_root, wait_for,
    input::{find_node, grabbed},
    uhid::HidPad,
};

const NAME: &str = "Test Controller";
/// How long the pad sits idle while CPU use is measured.
const IDLE: Duration = Duration::from_secs(5);
/// The most CPU the idle daemon may use, as a percentage of one core. An idle daemon should be near
/// zero; this leaves room for a loaded machine.
const LIMIT_PERCENT: f64 = 1.0;

/// CPU time the process has used so far, in clock ticks: user plus system (fields 14 and 15 of
/// `/proc/<pid>/stat`, counted after the parenthesised command name).
fn cpu_ticks(pid: u32) -> u64 {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let after_name = &stat[stat.rfind(')').unwrap() + 2..];
    let fields: Vec<&str> = after_name.split_whitespace().collect();
    fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
}

#[test]
#[ignore]
fn idle_daemon_uses_almost_no_cpu() {
    let _serial = serial();
    let root = temp_root();
    let run = root.join("run");

    let _pad = HidPad::new(NAME);
    let pad_node = wait_for("the controller's event node", || find_node(NAME));
    let mut daemon = spawn_daemon(&root, Some(&run));
    wait_for("the daemon to grab the controller", || grabbed(&pad_node).then_some(()));

    // Let start-up work finish before the measurement begins.
    thread::sleep(Duration::from_secs(1));
    let pid = daemon.0.id();
    let ticks_before = cpu_ticks(pid);
    let started = Instant::now();
    thread::sleep(IDLE);
    let ticks = cpu_ticks(pid) - ticks_before;
    let elapsed = started.elapsed().as_secs_f64();

    // SAFETY: sysconf only reads a system constant.
    let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    let percent = ticks as f64 / ticks_per_second / elapsed * 100.0;

    // SIGTERM is the normal stop (systemctl stop sends it), so the log is complete once it exits.
    // SAFETY: `kill` only sends a signal, to a child we started and have not yet waited for.
    let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    assert_eq!(rc, 0, "sending SIGTERM");
    wait_for("the daemon to exit", || daemon.0.try_wait().unwrap());

    assert_log_clean(&root);
    assert!(
        percent < LIMIT_PERCENT,
        "the idle daemon used {percent:.2}% of a core over {elapsed:.1} s (limit {LIMIT_PERCENT}%)"
    );
    let _ = fs::remove_dir_all(&root);
}

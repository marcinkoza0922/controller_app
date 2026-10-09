//! Starts the real settings window (`padwight gui`) against a running daemon, and checks that it
//! opens, draws and keeps running with no panic (U-partial). Every page and tab is built by the
//! in-process test `gui::tests::every_page_tab_and_dialog_builds`; this covers the window itself,
//! with layout and drawing, which needs a display. Clicking through the tabs in the window is
//! not done here: nothing drives the window.
//!
//! Needs a display, so it is opt-in and runs under a virtual X server:
//! `xvfb-run -a cargo test --test gui_smoke -- --ignored`, or `scripts/kernel-test-docker.sh gui_`
//! (which uses Xvfb in the container). It also needs `xwininfo` (x11-utils), to see the window. The daemon it starts uses uinput and uhid, so the first
//! also needs those devices.

mod common;

use std::{
    fs,
    os::unix::net::UnixStream,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use common::{install_config, Daemon, TIMEOUT, assert_log_clean, daemon_log, default_config, serial, spawn_daemon, temp_root, wait_for};

/// How long the window is left open. It draws its first page at once and then polls the daemon once
/// a second, so this covers several polls.
const OPEN_FOR: Duration = Duration::from_secs(5);

#[test]
#[ignore]
fn gui_opens_and_keeps_running() {
    let _serial = serial();
    assert!(std::env::var_os("DISPLAY").is_some(), "the window needs a display: run under xvfb-run");
    let root = temp_root();
    let run = root.join("run");
    let tmp = root.join("tmp");
    fs::create_dir_all(&tmp).unwrap();

    // One daemon for the whole test. The window finds its socket and doesn't start another.
    let config = root.join("config/padwight");
    fs::create_dir_all(&config).unwrap();
    install_config(&config, &default_config());
    let mut daemon: Daemon = spawn_daemon(&root, Some(&run));
    let socket = run.join("padwight.sock");
    let deadline = Instant::now() + TIMEOUT;
    while UnixStream::connect(&socket).is_err() {
        assert!(Instant::now() < deadline, "timed out waiting for the daemon's socket: {}", daemon_log(&root));
        thread::sleep(Duration::from_millis(20));
    }

    let gui_log = root.join("gui.log");
    let mut gui = Command::new(env!("CARGO_BIN_EXE_padwight"))
        .arg("gui")
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_RUNTIME_DIR", &run)
        .env("TMPDIR", &tmp)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&gui_log).unwrap())
        .stderr(fs::File::create(root.join("gui.err")).unwrap())
        .spawn()
        .expect("starting the settings window");

    let started = Instant::now();
    while started.elapsed() < OPEN_FOR {
        if let Some(status) = gui.try_wait().unwrap() {
            let out = fs::read_to_string(&gui_log).unwrap_or_default();
            let err = fs::read_to_string(root.join("gui.err")).unwrap_or_default();
            panic!("the window exited early ({status}):\n{out}{err}");
        }
        thread::sleep(Duration::from_millis(50));
    }

    // The window must be on the display, not only the process running: look for its title.
    let tree = Command::new("xwininfo").args(["-root", "-tree"]).output().expect("running xwininfo (x11-utils)");
    let tree = String::from_utf8_lossy(&tree.stdout);
    assert!(tree.contains("\"Padwight\""), "no window titled Padwight on the display:\n{tree}");

    gui.kill().unwrap();
    gui.wait().unwrap();
    let out = fs::read_to_string(&gui_log).unwrap_or_default();
    let err = fs::read_to_string(root.join("gui.err")).unwrap_or_default();
    assert!(!err.contains("panicked") && !out.contains("panicked"), "the window panicked:\n{out}{err}");

    // Stop the daemon the normal way, so its log is complete.
    // SAFETY: `kill` only sends a signal, to a child we started and have not yet waited for.
    let rc = unsafe { libc::kill(daemon.0.id() as libc::pid_t, libc::SIGTERM) };
    assert_eq!(rc, 0, "sending SIGTERM");
    wait_for("the daemon to exit", || daemon.0.try_wait().unwrap());
    assert_log_clean(&root);
    let _ = fs::remove_dir_all(&root);
}

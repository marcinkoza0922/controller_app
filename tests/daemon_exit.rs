//! Stops the real `padwight daemon` while it holds a key and a mouse button, against a fake
//! controller made with uhid, and checks what it leaves behind: the controller's grab is released
//! and the daemon's virtual devices are gone. For SIGTERM, which a daemon can handle, the keys must
//! also be released before it exits; SIGKILL can't be handled, so only the cleanup is checked.
//!
//! Needs /dev/uinput and read access to /dev/input, so it is opt-in:
//! `cargo test --test daemon_exit -- --ignored`. The daemon grabs any gamepad it finds, so run it
//! with no real controller attached.

mod common;

use std::{
    fs,
    os::unix::net::UnixStream,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use common::{
    Daemon, TIMEOUT, assert_log_clean, daemon_log, hold_config, serial, spawn_daemon, temp_root, wait_for,
    input::{Reader, find_node, grabbed},
    uhid::{BUTTON_EAST, BUTTON_SOUTH, HidPad},
};

use evdev::KeyCode;

#[derive(Clone, Copy, Debug)]
enum Signal {
    Kill,
    Term,
}

fn start_daemon(root: &Path) -> Daemon {
    let config = root.join("config/padwight");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("config.toml"), hold_config()).unwrap();
    spawn_daemon(root, Some(&root.join("run")))
}

fn send(daemon: &mut Daemon, signal: Signal) {
    match signal {
        Signal::Kill => daemon.0.kill().unwrap(),
        Signal::Term => {
            // SAFETY: `kill` only sends a signal, to a child we started and have not yet waited for.
            let rc = unsafe { libc::kill(daemon.0.id() as libc::pid_t, libc::SIGTERM) };
            assert_eq!(rc, 0, "sending SIGTERM");
        }
    }
}

fn wait_exit(daemon: &mut Daemon) {
    wait_for("the daemon to exit", || daemon.0.try_wait().unwrap());
}

fn scenario(signal: Signal) {
    let _serial = serial();
    let root = temp_root();

    let mut pad = HidPad::new("Test Controller");
    let pad_node = wait_for("the controller's event node", || find_node("Test Controller"));

    let mut daemon = start_daemon(&root);
    let run = root.join("run");
    let socket = run.join("padwight.sock");
    let deadline = Instant::now() + TIMEOUT;
    while UnixStream::connect(&socket).is_err() {
        if let Ok(Some(status)) = daemon.0.try_wait() {
            panic!("the daemon exited ({status}): {}", daemon_log(&root));
        }
        assert!(Instant::now() < deadline, "timed out waiting for the daemon's socket: {}", daemon_log(&root));
        thread::sleep(Duration::from_millis(20));
    }
    wait_for("the daemon to grab the fake controller", || grabbed(&pad_node).then_some(()));

    // Hold South (a key) and East (a mouse button). Resent until the daemon has opened the pad.
    let mut keys = Reader::open("Padwight Virtual Keyboard");
    let mut mouse = Reader::open("Padwight Virtual Mouse");
    let held = BUTTON_SOUTH | BUTTON_EAST;
    wait_for("KEY_A and the mouse's left button to be pressed", || {
        pad.report(held, 0, 0);
        thread::sleep(Duration::from_millis(20));
        keys.drain();
        mouse.drain();
        (keys.pressed.contains(&KeyCode::KEY_A.0) && mouse.pressed.contains(&KeyCode::BTN_LEFT.0)).then_some(())
    });

    // Stop the daemon. While it is still running it can release what it holds, so watch the keys
    // until they are up or it exits. Once it has exited its virtual devices are gone, and a reader
    // gets no release events from them, so anything still held at that point counts as stuck.
    send(&mut daemon, signal);
    let deadline = Instant::now() + TIMEOUT;
    let mut exited = false;
    while !exited && !(keys.pressed.is_empty() && mouse.pressed.is_empty()) {
        assert!(Instant::now() < deadline, "{signal:?}: the daemon neither exited nor released its keys");
        keys.drain();
        mouse.drain();
        exited = matches!(daemon.0.try_wait(), Ok(Some(_)));
        thread::sleep(Duration::from_millis(5));
    }
    let released_before_exit = keys.pressed.is_empty() && mouse.pressed.is_empty();
    wait_exit(&mut daemon);

    // SIGTERM is the one a daemon can handle (systemctl stop sends it), so it must release first.
    // SIGKILL can't be handled: nothing can be asserted about the keys then, only the cleanup below.
    if let Signal::Term = signal {
        assert!(
            released_before_exit,
            "SIGTERM: the daemon did not release its keys before exiting (KEY_A released: {}, left button released: {}). \
             It has no SIGTERM handler, and the tray's Quit exits without releasing anything either",
            !keys.pressed.contains(&KeyCode::KEY_A.0),
            !mouse.pressed.contains(&KeyCode::BTN_LEFT.0)
        );
    }
    wait_for("the virtual keyboard and mouse to go away", || (keys.drain() && mouse.drain()).then_some(()));

    // The daemon's grab on the controller went with it.
    assert!(!grabbed(&pad_node), "{signal:?}: the controller is still grabbed");

    // No virtual device is left: a leaked file descriptor would keep one registered.
    wait_for("the daemon's virtual devices to disappear", || {
        (find_node("Padwight Virtual Keyboard").is_none() && find_node("Padwight Virtual Mouse").is_none()).then_some(())
    });
    assert_log_clean(&root);

    let _ = fs::remove_dir_all(&root);
}

#[test]
#[ignore]
fn daemon_killed_with_sigkill_releases_everything() {
    scenario(Signal::Kill);
}

#[test]
#[ignore]
fn daemon_killed_with_sigterm_releases_everything() {
    scenario(Signal::Term);
}

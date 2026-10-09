//! Controllers leaving and coming back while the daemon runs. The controllers are made with uhid, so
//! the daemon sees real HID devices. The virtual keyboard and mouse stay up throughout, so the keys
//! it releases on an unplug are read from them while they are still there.
//!
//! Not covered here: a profile that needs gyro giving way when a controller without it is plugged
//! in. The daemon finds gyro from a motion sensor the kernel marks as one, and a simulated
//! controller can't report that.
//!
//! Needs /dev/uinput and /dev/uhid, and read access to /dev/input, so it is opt-in:
//! `cargo test --test hotplug -- --ignored`, or `scripts/kernel-test-docker.sh hotplug_`.

mod common;

use std::{fs, thread, time::Duration};

use common::{
    Daemon, assert_log_clean, daemon_log, hold_config, input::{Reader, find_node, grabbed}, serial, spawn_daemon, temp_root,
    uhid::{BUTTON_EAST, BUTTON_SOUTH, HidPad}, wait_for,
};
use evdev::KeyCode;

const NAME: &str = "Test Controller";
const HELD: u8 = BUTTON_SOUTH | BUTTON_EAST;

fn start(root: &std::path::Path) -> Daemon {
    let config = root.join("config/padwight");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("config.toml"), hold_config()).unwrap();
    spawn_daemon(root, Some(&root.join("run")))
}

/// Creates a controller and waits until the daemon has grabbed it.
fn connect() -> HidPad {
    connect_as(NAME)
}

/// Creates a controller called `name` and waits until the daemon has grabbed it.
fn connect_as(name: &str) -> HidPad {
    let pad = HidPad::new(name);
    wait_for("the daemon to grab the controller", || find_node(name).filter(|path| grabbed(path)));
    pad
}

/// Holds South (a key) and East (a mouse button) until the kernel shows both pressed. The state is
/// sent again on each try, since the kernel drops reports until the daemon has opened the pad.
fn hold(pad: &mut HidPad, keys: &mut Reader, mouse: &mut Reader) {
    wait_for("KEY_A and the left button to be pressed", || {
        pad.report(HELD, 0, 0);
        thread::sleep(Duration::from_millis(20));
        keys.drain();
        mouse.drain();
        (keys.pressed.contains(&KeyCode::KEY_A.0) && mouse.pressed.contains(&KeyCode::BTN_LEFT.0)).then_some(())
    });
}

/// Unplugs the controller while it holds a key and a button, and waits for the daemon to release
/// them. Returns once nothing is pressed on the virtual devices.
fn unplug_while_held(pad: HidPad, keys: &mut Reader, mouse: &mut Reader) {
    drop(pad);
    wait_for("the controller's node to go", || find_node(NAME).is_none().then_some(()));
    wait_for("the held keys to be released", || {
        keys.drain();
        mouse.drain();
        (keys.pressed.is_empty() && mouse.pressed.is_empty()).then_some(())
    });
}

#[test]
#[ignore]
fn hotplug_unplugging_releases_what_was_held() {
    let _serial = serial();
    let root = temp_root();
    let mut daemon = start(&root);
    let mut keys = Reader::open("Padwight Virtual Keyboard");
    let mut mouse = Reader::open("Padwight Virtual Mouse");
    let mut pad = connect();
    hold(&mut pad, &mut keys, &mut mouse);

    unplug_while_held(pad, &mut keys, &mut mouse);

    assert!(daemon.0.try_wait().unwrap().is_none(), "the daemon keeps running after an unplug");
    let log = daemon_log(&root);
    assert!(log.contains(&format!("device gone: {NAME}")), "the daemon noticed the unplug: {log}");
    let _ = daemon.0.kill();
    let _ = daemon.0.wait();
    assert_log_clean(&root);
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[ignore]
fn hotplug_replugging_the_same_controller_works_again() {
    let _serial = serial();
    let root = temp_root();
    let mut daemon = start(&root);
    let mut keys = Reader::open("Padwight Virtual Keyboard");
    let mut mouse = Reader::open("Padwight Virtual Mouse");
    let mut pad = connect();
    hold(&mut pad, &mut keys, &mut mouse);
    unplug_while_held(pad, &mut keys, &mut mouse);

    // Plugged back in, it is managed again, and the same profile still maps it: South holds a key.
    let mut pad = connect();
    hold(&mut pad, &mut keys, &mut mouse);

    assert!(daemon.0.try_wait().unwrap().is_none(), "the daemon keeps running across the replug");
    let log = daemon_log(&root);
    assert_eq!(log.matches(&format!("managing {NAME} (")).count(), 2, "the controller was taken twice: {log}");
    let _ = daemon.0.kill();
    let _ = daemon.0.wait();
    assert_log_clean(&root);
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[ignore]
fn hotplug_a_different_controller_takes_over_and_is_remapped() {
    let _serial = serial();
    let root = temp_root();
    let mut daemon = start(&root);
    let mut keys = Reader::open("Padwight Virtual Keyboard");
    let mut mouse = Reader::open("Padwight Virtual Mouse");
    let mut pad = connect();
    hold(&mut pad, &mut keys, &mut mouse);
    unplug_while_held(pad, &mut keys, &mut mouse);

    // A different controller in the same place: it is grabbed and its buttons are remapped.
    const OTHER: &str = "Other Controller";
    let mut pad = connect_as(OTHER);
    hold(&mut pad, &mut keys, &mut mouse);

    assert!(daemon.0.try_wait().unwrap().is_none(), "the daemon keeps running when the controller changes");
    let log = daemon_log(&root);
    assert!(log.contains(&format!("managing {OTHER} (")), "the daemon took the new controller: {log}");
    assert!(log.contains(&format!("device gone: {NAME}")), "the daemon let go of the old one: {log}");
    let _ = daemon.0.kill();
    let _ = daemon.0.wait();
    assert_log_clean(&root);
    let _ = fs::remove_dir_all(&root);
}

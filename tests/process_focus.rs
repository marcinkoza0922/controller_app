//! Profile switching by process, on a desktop with no focus tracker (D-partial). The daemon scans
//! the running processes for a game's rule; a stand-in game process named like the rule's
//! executable switches the profile, and its exit switches it back. The desktop trackers (KWin, Sway,
//! Hyprland) are not started here: they need a session, so they stay manual.
//!
//! Needs /dev/uinput and /dev/uhid, and read access to /dev/input, so it is opt-in:
//! `cargo test --test process_focus -- --ignored`, or `scripts/kernel-test-docker.sh process_`.

mod common;

use std::{
    fs,
    path::Path,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

use common::{Daemon, TIMEOUT, assert_log_clean, daemon_log, default_config, serial, spawn_daemon, temp_root, wait_for};

/// The stand-in game's file name, which the rule matches.
const GAME_EXE: &str = "padwight-fake-game";
const GAME: &str = "Fake Game";
const GAME_PROFILE: &str = "Fake Pad";

/// The default config with one game added: a rule that matches [`GAME_EXE`], and a profile copied
/// from the General Gamepad profile, so it maps the same buttons under another name.
fn game_config() -> String {
    // The default config has an empty `games` list, which the game's `[[games]]` would repeat.
    let text = default_config();
    assert!(text.contains("\ngames = []\n"), "the default config has an empty games list");
    let text = text.replace("\ngames = []\n", "\n");
    let mut general = Vec::new();
    let mut in_profile = false;
    for line in text.lines() {
        if line.starts_with("[[general.profiles]]") {
            in_profile = true;
        } else if line.starts_with('[') && !line.starts_with("[general.profiles.") && !line.starts_with("[[general.profiles.") {
            in_profile = false;
        }
        if in_profile {
            general.push(line);
        }
    }
    let profile = general.join("\n").replace("general.profiles", "games.profiles").replace("name = \"Gamepad\"", &format!("name = \"{GAME_PROFILE}\""));
    assert!(profile.contains(&format!("name = \"{GAME_PROFILE}\"")), "the default config has a Gamepad profile");
    format!(
        "{text}\n[[games]]\nname = \"{GAME}\"\n\n[[games.rules]]\nkind = \"executable\"\nvalue = \"{GAME_EXE}\"\nprofile = \"{GAME_PROFILE}\"\n\n{profile}\n"
    )
}

/// A process that runs as [`GAME_EXE`]: a copy of `sleep` under that name, so the daemon sees the
/// name in the process's executable.
fn start_game(root: &Path) -> Child {
    let sleep = ["/bin/sleep", "/usr/bin/sleep"].into_iter().find(|p| Path::new(p).exists()).expect("a sleep binary");
    let game = root.join(GAME_EXE);
    fs::copy(sleep, &game).unwrap();
    Command::new(&game).arg("60").spawn().unwrap()
}

/// Waits until the daemon's log has `line`. On a timeout the failure shows the whole log.
fn wait_for_log(root: &Path, line: &str) {
    let deadline = Instant::now() + TIMEOUT;
    while !daemon_log(root).contains(line) {
        assert!(Instant::now() < deadline, "timed out waiting for the log to say {line:?}: {}", daemon_log(root));
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore]
fn a_game_process_switches_the_profile_and_back() {
    let _serial = serial();
    let root = temp_root();
    let config = root.join("config/padwight");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("config.toml"), game_config()).unwrap();
    let mut daemon: Daemon = spawn_daemon(&root, Some(&root.join("run")));

    // With no session and no tracker, the daemon falls back to scanning processes.
    wait_for_log(&root, "focus tracking: running processes");

    let mut game = start_game(&root);
    wait_for_log(&root, &format!("running processes → profile {GAME} › {GAME_PROFILE}"));

    game.kill().unwrap();
    game.wait().unwrap();
    wait_for_log(&root, "no game running → profile Gamepad");

    // SIGTERM is the normal stop, so the log is complete once it exits.
    // SAFETY: `kill` only sends a signal, to a child we started and have not yet waited for.
    let rc = unsafe { libc::kill(daemon.0.id() as libc::pid_t, libc::SIGTERM) };
    assert_eq!(rc, 0, "sending SIGTERM");
    wait_for("the daemon to exit", || daemon.0.try_wait().unwrap());
    assert_log_clean(&root);
    let _ = fs::remove_dir_all(&root);
}

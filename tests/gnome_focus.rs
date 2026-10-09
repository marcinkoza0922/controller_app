//! GNOME focus tracking, against a stand-in for GNOME Shell (`tests/fixtures/gnome_shell_mock.js`)
//! on a private session bus. The daemon should install the extension, enable it through
//! `org.gnome.Shell.Extensions`, report GNOME Shell as its focus tracker, and accept the
//! extension's `WindowActivated` call. It doesn't prove that real GNOME Shell loads the extension.
//!
//! Needs `gjs` and a session bus, so it is opt-in:
//! `dbus-run-session -- cargo nextest run --profile agent --hide-progress-bar --cargo-quiet --run-ignored ignored-only gnome_`.

mod common;

use std::{
    path::Path,
    process::{Child, Command},
};

use common::{assert_log_clean, daemon_log, serial, spawn_daemon, temp_root, wait_for};

const UUID: &str = "padwight-focus@io.github.marcinkoza0922";
const MOCK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/gnome_shell_mock.js");
const LABEL: &str = "focus tracking: focused window (GNOME Shell)";

/// Kills the stand-in when the test ends.
struct Mock(Child);

impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn session_has_owner(name: &str) -> bool {
    let conn = zbus::blocking::Connection::session().unwrap();
    conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "NameHasOwner",
        &(name,),
    )
    .ok()
    .and_then(|reply| reply.body().deserialize::<bool>().ok())
    .unwrap_or(false)
}

fn start_mock(root: &Path) -> Mock {
    let child = Command::new("gjs")
        .arg(MOCK)
        .env("XDG_DATA_HOME", root.join("data"))
        .spawn()
        .expect("starting gjs (is it installed?)");
    wait_for("the GNOME Shell stand-in on the bus", || session_has_owner("org.gnome.Shell").then_some(()));
    Mock(child)
}

#[test]
#[ignore]
fn gnome_extension_is_installed_enabled_and_reported() {
    let _guard = serial();
    let root = temp_root();
    let _mock = start_mock(&root);

    let daemon = spawn_daemon(&root, Some(&root.join("runtime")));
    wait_for(LABEL, || daemon_log(&root).contains(LABEL).then_some(()));

    let dir = root.join("data/gnome-shell/extensions").join(UUID);
    assert!(dir.join("metadata.json").exists(), "metadata.json not installed in {}", dir.display());
    assert!(dir.join("extension.js").exists(), "extension.js not installed in {}", dir.display());

    // What the extension sends on each focus change: three strings, as in extension.js.
    let conn = zbus::blocking::Connection::session().unwrap();
    conn.call_method(
        Some("io.github.marcinkoza0922.Padwight"),
        "/Focus",
        Some("io.github.marcinkoza0922.Padwight.Focus"),
        "WindowActivated",
        &("firefox", "1234", "A window"),
    )
    .expect("the daemon's WindowActivated method");
    // And when nothing has focus, which is the extension's other call.
    conn.call_method(
        Some("io.github.marcinkoza0922.Padwight"),
        "/Focus",
        Some("io.github.marcinkoza0922.Padwight.Focus"),
        "FocusCleared",
        &(),
    )
    .expect("the daemon's FocusCleared method");

    drop(daemon);
    assert_log_clean(&root);
}

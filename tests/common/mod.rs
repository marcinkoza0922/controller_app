//! Helpers shared by the integration tests that run the real daemon binary.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Mutex, MutexGuard},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub mod input;
pub mod uhid;

pub const TIMEOUT: Duration = Duration::from_secs(10);

/// Tests in one binary run in parallel, but the daemon's virtual devices have fixed names, and
/// every daemon grabs the same controllers. Each test takes this lock for its whole run.
static SERIAL: Mutex<()> = Mutex::new(());

pub fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The daemon's default config, as it writes it on first start. Its Gamepad profile (the active one)
/// passes every button through; the test changes South and East, to hold a key and a mouse button.
/// Regenerate it if the config format changes: start the daemon with an empty XDG_CONFIG_HOME.
const DEFAULT_CONFIG: &str = include_str!("../fixtures/default-config.toml");

/// The default config with every real gamepad on this machine ignored, so a daemon started by a
/// test never grabs the user's controller. The test's own controllers are not on the list.
pub fn default_config() -> String {
    let ignored: Vec<String> = real_gamepads().iter().map(|name| format!("{name:?}")).collect();
    DEFAULT_CONFIG.replace("ignored_devices = []", &format!("ignored_devices = [{}]", ignored.join(", ")))
}

/// Names of the gamepads the daemon would take, other than the test's own and the daemon's virtual
/// devices. Found with the same rule the daemon uses.
fn real_gamepads() -> Vec<String> {
    let Ok(entries) = fs::read_dir("/dev/input") else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("event"))
        .filter_map(|entry| {
            let dev = evdev::Device::open(entry.path()).ok()?;
            let keys = dev.supported_keys()?;
            let axes = dev.supported_absolute_axes()?;
            let gamepad = keys.contains(evdev::KeyCode::BTN_SOUTH)
                && axes.contains(evdev::AbsoluteAxisCode::ABS_X)
                && axes.contains(evdev::AbsoluteAxisCode::ABS_Y);
            let name = dev.name()?.to_string();
            let ours = name.starts_with("Padwight Virtual") || name.starts_with("Test Controller");
            (gamepad && !ours).then_some(name)
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The config for a test that holds South (a key) and East (a mouse button): the default one, with
/// those two buttons changed.
pub fn hold_config() -> String {
    let text = default_config().replace("[general.profiles.buttons.South]\ngamepad = \"South\"", "[general.profiles.buttons.South]\nkeys = [\"KEY_A\"]");
    let text = text.replace("[general.profiles.buttons.East]\ngamepad = \"East\"", "[general.profiles.buttons.East]\nmouse = \"Left\"");
    assert!(text.contains("keys = [\"KEY_A\"]") && text.contains("mouse = \"Left\""), "the default config has South and East as passthrough");
    text
}

/// The daemon, killed if the test fails before it has been stopped.
pub struct Daemon(pub Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A fresh directory for one test's files; everything the daemon touches lives under it.
pub fn temp_root() -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("padwight-test-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Polls `f` until it returns a value, or panics after `TIMEOUT`.
pub fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(value) = f() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(20));
    }
}

/// Starts `padwight daemon` with its config, state and temp files under `root`. With `runtime`
/// set it gets that as `XDG_RUNTIME_DIR`; without, it has none and uses its temp-dir fallback.
/// Its output goes to `root/daemon.log`.
pub fn spawn_daemon(root: &Path, runtime: Option<&Path>) -> Daemon {
    let config = root.join("config/padwight/config.toml");
    if !config.exists() {
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, default_config()).unwrap();
    }
    let tmp = root.join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    let log = fs::File::create(root.join("daemon.log")).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_padwight"));
    command
        .arg("daemon")
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("TMPDIR", &tmp)
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    match runtime {
        Some(dir) => {
            fs::create_dir_all(dir).unwrap();
            command.env("XDG_RUNTIME_DIR", dir);
        }
        None => {
            command.env_remove("XDG_RUNTIME_DIR");
        }
    }
    Daemon(command.spawn().expect("starting the daemon"))
}

/// The daemon's log, for a failure message.
pub fn daemon_log(root: &Path) -> String {
    fs::read_to_string(root.join("daemon.log")).unwrap_or_default()
}

/// Fails if the daemon's log has a panic or a failed output write. Call it once the daemon has
/// stopped, so the log is complete (I-8). Other messages are not checked: a container has no
/// compositor or session bus, and the daemon logs that as an error.
pub fn assert_log_clean(root: &Path) {
    let log = daemon_log(root);
    assert!(!log.contains("panicked"), "the daemon panicked:\n{log}");
    assert!(!log.contains("output error"), "the daemon failed to send input:\n{log}");
}

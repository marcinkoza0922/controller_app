//! Daemon <-> client protocol: one JSON request line, one JSON response line per connection.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{Button, Config, ProfileRef};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Status,
    GetConfig,
    /// Replace profiles/ignore list. The daemon keeps its current `enabled` and active profile.
    SetConfig(Box<Config>),
    /// Re-read the config file from disk.
    Reload,
    SetEnabled(bool),
    /// Switch to a profile found by name (see `Config::find_profile`).
    SetProfile(String),
    /// Switch to exactly this profile.
    Activate(ProfileRef),
    NextProfile,
    /// Keep the connection open; the daemon streams one `Option<InputSnapshot>` JSON line per
    /// update (at most ~60/s). `null` means no controller is active.
    WatchInput,
    /// Play a short test pattern on the controller at this device path.
    TestRumble(String),
    /// Average the gyro of the controller at this path for a moment, as its drift bias.
    CalibrateGyro(String),
    /// After 3 seconds, move the mouse this many pixels to the right over a second, to see how
    /// far it turns a game's camera.
    TestTurn(i32),
    /// Open or close the on-screen keyboard.
    ToggleOverlay,
    /// Open or close the on-screen numpad.
    ToggleNumpad,
    /// Show the menu with this name (items run on the most recently used controller).
    OpenMenu(String),
    /// Keep the connection open; the daemon streams one `Option<OverlayView>` JSON line per
    /// change. `null` means nothing is shown (the resident overlay idles).
    WatchOverlay,
}

// One reply per connection, so the size difference doesn't matter.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Ok,
    Status(Status),
    Config(Box<Config>),
    Error(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub enabled: bool,
    pub active_profile: String,
    /// The active profile's game; `None` is General.
    #[serde(default)]
    pub active_game: Option<String>,
    /// Layers active right now (oldest first), on top of the active profile.
    #[serde(default)]
    pub active_layers: Vec<String>,
    pub devices: Vec<DeviceInfo>,
    #[serde(default)]
    pub focus_backend: FocusBackend,
    /// Currently focused window (when the desktop reports it).
    #[serde(default)]
    pub focused: Option<WindowInfo>,
    /// Recently focused windows, newest first, for building rules.
    #[serde(default)]
    pub recent_windows: Vec<WindowInfo>,
    /// Controller motion sensors the daemon may not open (needs the udev rule in dist/).
    #[serde(default)]
    pub motion_access_denied: Vec<String>,
    /// The on-screen keyboard is up.
    #[serde(default)]
    pub overlay_visible: bool,
    #[serde(default)]
    pub numpad_visible: bool,
}

/// How the daemon learns which game is active.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusBackend {
    /// KWin script reports focus changes (KDE Plasma, Wayland or X11).
    Kwin,
    /// Sway's IPC socket reports focus changes.
    Sway,
    /// Hyprland's event socket reports focus changes.
    Hyprland,
    /// No focus information: rules match against running processes instead.
    #[default]
    ProcessScan,
}

impl FocusBackend {
    /// The desktop whose windows are tracked, `None` when only processes are.
    pub fn desktop(self) -> Option<&'static str> {
        match self {
            Self::Kwin => Some("KWin"),
            Self::Sway => Some("Sway"),
            Self::Hyprland => Some("Hyprland"),
            Self::ProcessScan => None,
        }
    }

    /// What per-game switching follows, for people.
    pub fn label(self) -> String {
        match self.desktop() {
            Some(desktop) => format!("focused window ({desktop})"),
            None => "running processes".into(),
        }
    }
}

/// What we know about a window (or, for process scanning, a process).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub class: String,
    pub title: String,
    pub pid: u32,
    /// Executable file name; for Wine/Proton the Windows `.exe`, not the Wine loader.
    pub exe: String,
    pub steam_app_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub path: String,
    /// Grabbed and being remapped right now.
    pub managed: bool,
    pub ignored: bool,
    /// False when the triggers only report on/off (e.g. Switch controllers).
    #[serde(default = "yes")]
    pub analog_triggers: bool,
    /// Supports rumble (FF_RUMBLE), so it can be tested and receives game rumble.
    #[serde(default)]
    pub rumble: bool,
    /// A motion-sensor device is paired with it.
    #[serde(default)]
    pub gyro: bool,
}

fn yes() -> bool {
    true
}

/// Physical (pre-mapping) input state of the most recently used controller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputSnapshot {
    pub device: String,
    pub buttons: Vec<Button>,
    /// Sticks are -1.0..1.0 (Y positive = down).
    pub left_stick: (f32, f32),
    pub right_stick: (f32, f32),
    /// 0.0..1.0
    pub left_trigger: f32,
    pub right_trigger: f32,
    /// Degrees/second pitch, yaw, roll, for controllers with a gyro.
    #[serde(default)]
    pub gyro: Option<[f32; 3]>,
}

pub fn socket_path() -> PathBuf {
    match dirs::runtime_dir() {
        Some(dir) => dir.join("padwight.sock"),
        None => std::env::temp_dir().join(format!("padwight-{}.sock", unsafe { libc::getuid() })),
    }
}

/// Blocking request to the daemon. Fine to call from a GUI via `spawn_blocking`.
pub fn request(req: &Request) -> Result<Response> {
    let path = socket_path();
    let stream = UnixStream::connect(&path)
        .with_context(|| format!("daemon not reachable at {}", path.display()))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;

    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    (&stream).write_all(line.as_bytes())?;

    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply)?;
    match serde_json::from_str(&reply)? {
        Response::Error(e) => bail!(e),
        r => Ok(r),
    }
}

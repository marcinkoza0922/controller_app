//! Hyprland: follows the `activewindowv2` events on its event socket and asks the command socket
//! for the window's details.

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use anyhow::{Context, Result};
use serde_json::Value;

use super::{Notify, Reported, report};

pub struct Hyprland {
    events: UnixStream,
    dir: PathBuf,
}

/// This session's instance directory: `$HYPRLAND_INSTANCE_SIGNATURE`'s, else the newest one in
/// the runtime dir (a systemd user service doesn't necessarily inherit the compositor's
/// environment).
fn instance_dirs() -> Vec<PathBuf> {
    let Some(root) = dirs::runtime_dir().map(|dir| dir.join("hypr")) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").map(|sig| root.join(sig)).into_iter().collect();
    let mut found: Vec<_> = fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    dirs.extend(found.into_iter().map(|(_, path)| path));
    dirs
}

pub fn connect() -> Result<Hyprland> {
    instance_dirs()
        .into_iter()
        .find_map(|dir| Some(Hyprland { events: UnixStream::connect(dir.join(".socket2.sock")).ok()?, dir }))
        .context("no Hyprland socket")
}

/// One request on the command socket; the reply runs to the end of the stream.
fn query(dir: &std::path::Path, command: &str) -> Result<String> {
    let mut stream = UnixStream::connect(dir.join(".socket.sock"))?;
    stream.write_all(command.as_bytes())?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    Ok(reply)
}

impl Hyprland {
    /// Reports the focused window now and on every change, until the connection ends.
    pub fn follow(self, notify: &Notify) -> Result<()> {
        let report_active = || {
            if let Some(window) = query(&self.dir, "j/activewindow").ok().and_then(|json| active_window(&json)) {
                report(notify, window);
            }
        };
        report_active();
        for line in BufReader::new(&self.events).lines() {
            if line?.starts_with("activewindowv2>>") {
                report_active();
            }
        }
        Ok(())
    }
}

/// The reply to `j/activewindow`: `{}` when nothing has focus.
fn active_window(json: &str) -> Option<Reported> {
    let json: Value = serde_json::from_str(json).ok()?;
    let pid = u32::try_from(json["pid"].as_u64()?).ok()?;
    Some(Reported {
        class: json["class"].as_str().unwrap_or_default().into(),
        title: json["title"].as_str().unwrap_or_default().into(),
        pid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_active_window_reply() {
        let json = r#"{"address":"0x5f","mapped":true,"class":"steam_app_1245620","title":"ELDEN RING","pid":4242,"xwayland":true}"#;
        let window = active_window(json).unwrap();
        assert_eq!((window.pid, window.class.as_str(), window.title.as_str()), (4242, "steam_app_1245620", "ELDEN RING"));
    }

    #[test]
    fn no_focused_window_reports_nothing() {
        assert!(active_window("{}").is_none());
        assert!(active_window(r#"{"pid": -1, "class": "x"}"#).is_none());
        assert!(active_window("Invalid").is_none());
    }
}

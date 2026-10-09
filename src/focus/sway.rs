//! Sway (and other compositors that speak the i3 IPC protocol): subscribes to window events on
//! the compositor's IPC socket.

use std::{
    fs,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::{FocusEvent, Notify, Reported, report};

const MAGIC: &[u8; 6] = b"i3-ipc";
const SUBSCRIBE: u32 = 2;
const GET_TREE: u32 = 4;
const EVENT_WINDOW: u32 = 0x8000_0003;

pub struct Sway(UnixStream);

/// `$SWAYSOCK`, then any `sway-ipc.*.sock` in the runtime dir (a systemd user service doesn't
/// necessarily inherit the compositor's environment).
fn candidates() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::env::var_os("SWAYSOCK").map(PathBuf::from).into_iter().collect();
    if let Some(entries) = dirs::runtime_dir().and_then(|dir| fs::read_dir(dir).ok()) {
        paths.extend(entries.flatten().map(|e| e.path()).filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("sway-ipc.") && n.ends_with(".sock"))
        }));
    }
    paths
}

pub fn connect() -> Result<Sway> {
    candidates()
        .iter()
        .find_map(|path| UnixStream::connect(path).ok())
        .map(Sway)
        .context("no Sway IPC socket")
}

fn send(stream: &mut UnixStream, kind: u32, payload: &[u8]) -> Result<()> {
    let mut msg = Vec::with_capacity(14 + payload.len());
    msg.extend_from_slice(MAGIC);
    msg.extend_from_slice(&u32::try_from(payload.len())?.to_ne_bytes());
    msg.extend_from_slice(&kind.to_ne_bytes());
    msg.extend_from_slice(payload);
    Ok(stream.write_all(&msg)?)
}

fn read_frame(stream: &mut UnixStream) -> Result<(u32, Vec<u8>)> {
    let mut head = [0u8; 14];
    stream.read_exact(&mut head)?;
    if &head[..6] != MAGIC {
        bail!("not an i3 IPC reply");
    }
    let len = u32::from_ne_bytes([head[6], head[7], head[8], head[9]]);
    let kind = u32::from_ne_bytes([head[10], head[11], head[12], head[13]]);
    let mut payload = vec![0; len as usize];
    stream.read_exact(&mut payload)?;
    Ok((kind, payload))
}

impl Sway {
    /// Reports the focused window now and on every change, until the connection ends.
    pub fn follow(mut self, notify: &Notify) -> Result<()> {
        send(&mut self.0, SUBSCRIBE, br#"["window"]"#)?;
        send(&mut self.0, GET_TREE, b"")?;
        loop {
            let (kind, payload) = read_frame(&mut self.0)?;
            let json: Value = serde_json::from_slice(&payload)?;
            match kind {
                GET_TREE => match focused_window(&json).and_then(reported) {
                    Some(window) => report(notify, window),
                    None => notify(FocusEvent::Unfocused),
                },
                EVENT_WINDOW if json["change"] == "focus" => match reported(&json["container"]) {
                    Some(window) => report(notify, window),
                    None => notify(FocusEvent::Unfocused),
                },
                // Closing the focused window moves focus without a focus event, so ask again.
                EVENT_WINDOW if json["change"] == "close" => send(&mut self.0, GET_TREE, b"")?,
                _ => {}
            }
        }
    }
}

/// The focused window in a `get_tree` reply. (A focused empty workspace has no pid.)
fn focused_window(node: &Value) -> Option<&Value> {
    if node["focused"] == true && node["pid"].is_number() {
        return Some(node);
    }
    ["nodes", "floating_nodes"].iter().filter_map(|key| node[key].as_array()).flatten().find_map(focused_window)
}

/// A window's class is its `app_id`, or the X11 class for Xwayland windows.
fn reported(node: &Value) -> Option<Reported> {
    let pid = u32::try_from(node["pid"].as_u64()?).ok()?;
    let class = node["app_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .or_else(|| node["window_properties"]["class"].as_str())
        .unwrap_or_default();
    Some(Reported { class: class.into(), title: node["name"].as_str().unwrap_or_default().into(), pid })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Value {
        serde_json::json!({
            "focused": false, "nodes": [{
                "focused": false, "nodes": [{
                    "focused": true, "nodes": [], "floating_nodes": [],
                    "name": "1"
                }, {
                    "focused": false, "nodes": [{
                        "focused": false, "pid": 11, "app_id": "foot", "name": "~", "nodes": []
                    }],
                    "floating_nodes": [{
                        "focused": true, "pid": 22, "app_id": null, "name": "ELDEN RING",
                        "window_properties": {"class": "steam_app_1245620"}, "nodes": [], "floating_nodes": []
                    }]
                }]
            }]
        })
    }

    #[test]
    fn finds_the_focused_window_past_empty_workspaces_and_into_floating() {
        let tree = tree();
        let window = reported(focused_window(&tree).unwrap()).unwrap();
        assert_eq!((window.pid, window.class.as_str(), window.title.as_str()), (22, "steam_app_1245620", "ELDEN RING"));
    }

    #[test]
    fn native_windows_use_their_app_id() {
        let node = serde_json::json!({"pid": 11, "app_id": "foot", "name": "~", "window_properties": {"class": "x"}});
        assert_eq!(reported(&node).unwrap().class, "foot");
        assert!(reported(&serde_json::json!({"name": "no pid"})).is_none());
    }

    #[test]
    fn frames_round_trip_over_a_socket_pair() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        send(&mut a, GET_TREE, b"{}").unwrap();
        assert_eq!(read_frame(&mut b).unwrap(), (GET_TREE, b"{}".to_vec()));
    }

    #[test]
    fn a_focus_change_to_nothing_reports_unfocused() {
        use std::sync::{Arc, Mutex};

        use crate::focus::FocusEvent;

        let (client, mut server) = UnixStream::pair().unwrap();
        let frame = |kind: u32, json: &Value| {
            let body = json.to_string();
            let mut msg = [MAGIC.as_slice(), &u32::try_from(body.len()).unwrap().to_ne_bytes(), &kind.to_ne_bytes()].concat();
            msg.extend_from_slice(body.as_bytes());
            msg
        };
        // An empty workspace (no pid) is what focus moves to when the last window closes.
        let empty = serde_json::json!({"change": "focus", "container": {"name": "2", "nodes": []}});
        server.write_all(&frame(EVENT_WINDOW, &empty)).unwrap();
        server.shutdown(std::net::Shutdown::Write).unwrap();

        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let notify: Notify = Arc::new(move |ev| {
            sink.lock().unwrap().push(matches!(ev, FocusEvent::Unfocused));
        });
        assert!(Sway(client).follow(&notify).is_err(), "ends when the socket closes");
        assert_eq!(*seen.lock().unwrap(), [true]);
    }

    #[test]
    fn follow_reports_the_tree_and_focus_events() {
        use std::sync::{Arc, Mutex};

        use crate::focus::FocusEvent;

        let (client, mut server) = UnixStream::pair().unwrap();
        let frame = |kind: u32, json: &Value| {
            let body = json.to_string();
            let mut msg = [MAGIC.as_slice(), &u32::try_from(body.len()).unwrap().to_ne_bytes(), &kind.to_ne_bytes()].concat();
            msg.extend_from_slice(body.as_bytes());
            msg
        };
        server.write_all(&frame(GET_TREE, &tree())).unwrap();
        let focus = serde_json::json!({"change": "focus", "container": {"pid": 5, "app_id": "foot", "name": "~"}});
        server.write_all(&frame(EVENT_WINDOW, &focus)).unwrap();
        let title = serde_json::json!({"change": "title", "container": {"pid": 6, "app_id": "other", "name": "x"}});
        server.write_all(&frame(EVENT_WINDOW, &title)).unwrap();
        server.shutdown(std::net::Shutdown::Write).unwrap();

        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let notify: Notify = Arc::new(move |ev| {
            if let FocusEvent::Focused(info) = ev {
                sink.lock().unwrap().push(info.class);
            }
        });
        assert!(Sway(client).follow(&notify).is_err(), "ends when the socket closes");
        assert_eq!(*seen.lock().unwrap(), ["steam_app_1245620", "foot"]);
    }
}

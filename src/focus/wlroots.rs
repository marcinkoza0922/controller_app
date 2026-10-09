//! wlroots compositors (labwc, Wayfire, river, and others that offer `wlr-foreign-toplevel-management`):
//! follows the protocol's window list and reports the activated window. The protocol doesn't say
//! which process owns a window, so only class rules match here, not executable rules.

use std::{
    collections::HashMap,
    fs,
    hash::Hash,
    os::unix::net::UnixStream,
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    backend::ObjectId,
    event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::wl_registry,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

use super::{Notify, Reported, report};
use crate::focus::FocusEvent;

/// `activated` in the protocol's state enum.
const ACTIVATED: u32 = 2;

/// What the protocol's window events add up to: which window has focus, if any.
#[derive(Debug, PartialEq)]
pub enum Change {
    Focused { app_id: String, title: String },
    Unfocused,
}

#[derive(Default)]
struct Toplevel {
    app_id: String,
    title: String,
    /// From the `state` event, applied at `done` as the protocol asks.
    activated_pending: bool,
    activated: bool,
}

/// Turns the window events into focus changes. Keys are the windows' protocol identities.
pub struct Windows<K> {
    toplevels: HashMap<K, Toplevel>,
    focused: Option<K>,
    /// The title and app id last reported for the focused window, so a title change reports again.
    reported: Option<(String, String)>,
}

impl<K> Default for Windows<K> {
    fn default() -> Self {
        Self { toplevels: HashMap::new(), focused: None, reported: None }
    }
}

impl<K: Eq + Hash + Clone> Windows<K> {
    pub fn new_window(&mut self, id: K) {
        self.toplevels.insert(id, Toplevel::default());
    }

    pub fn app_id(&mut self, id: &K, app_id: String) {
        if let Some(t) = self.toplevels.get_mut(id) {
            t.app_id = app_id;
        }
    }

    pub fn title(&mut self, id: &K, title: String) {
        if let Some(t) = self.toplevels.get_mut(id) {
            t.title = title;
        }
    }

    pub fn state(&mut self, id: &K, activated: bool) {
        if let Some(t) = self.toplevels.get_mut(id) {
            t.activated_pending = activated;
        }
    }

    /// The window's events are complete. Returns a change when focus moved, or the focused
    /// window's title or app id changed.
    pub fn done(&mut self, id: &K) -> Option<Change> {
        let t = self.toplevels.get_mut(id)?;
        t.activated = t.activated_pending;
        if t.activated {
            let info = (t.app_id.clone(), t.title.clone());
            let moved = self.focused.as_ref() != Some(id);
            if !moved && self.reported.as_ref() == Some(&info) {
                return None;
            }
            self.focused = Some(id.clone());
            self.reported = Some(info.clone());
            return Some(Change::Focused { app_id: info.0, title: info.1 });
        }
        if self.focused.as_ref() == Some(id) {
            self.focused = None;
            self.reported = None;
            return Some(Change::Unfocused);
        }
        None
    }

    pub fn closed(&mut self, id: &K) -> Option<Change> {
        self.toplevels.remove(id);
        if self.focused.as_ref() == Some(id) {
            self.focused = None;
            self.reported = None;
            return Some(Change::Unfocused);
        }
        None
    }
}

/// The state the Wayland event loop dispatches into.
struct State {
    windows: Windows<ObjectId>,
    changes: Vec<Change>,
}

/// A connection to the compositor's foreign toplevel manager.
pub struct Wlroots {
    queue: wayland_client::EventQueue<State>,
    state: State,
}

/// The compositor's socket: `$WAYLAND_DISPLAY`, else the first `wayland-N` in the runtime dir (a
/// systemd user service doesn't necessarily inherit the compositor's environment).
fn socket_candidates() -> Vec<PathBuf> {
    let dir = dirs::runtime_dir().unwrap_or_default();
    let mut paths: Vec<PathBuf> = std::env::var_os("WAYLAND_DISPLAY").map(|name| dir.join(name)).into_iter().collect();
    if let Ok(entries) = fs::read_dir(&dir) {
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("wayland-") && !n.ends_with(".lock"))
            })
            .collect();
        found.sort();
        paths.extend(found);
    }
    paths
}

/// Connects and checks that the compositor offers the protocol.
pub fn connect() -> Result<Wlroots> {
    let stream = socket_candidates()
        .iter()
        .find_map(|path| UnixStream::connect(path).ok())
        .context("no Wayland socket")?;
    let conn = Connection::from_socket(stream)?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    let manager: std::result::Result<ZwlrForeignToplevelManagerV1, _> = globals.bind(&qh, 1..=3, ());
    if manager.is_err() {
        bail!("the compositor doesn't offer wlr-foreign-toplevel-management");
    }
    let mut state = State { windows: Windows::default(), changes: Vec::new() };
    queue.roundtrip(&mut state)?;
    Ok(Wlroots { queue, state })
}

impl Wlroots {
    /// Reports the focused window now and on every change, until the connection ends.
    pub fn follow(mut self, notify: &Notify) -> Result<()> {
        loop {
            self.queue.blocking_dispatch(&mut self.state)?;
            for change in self.state.changes.drain(..) {
                match change {
                    Change::Focused { app_id, title } => report(notify, Reported { class: app_id, title, pid: 0 }),
                    Change::Unfocused => notify(FocusEvent::Unfocused),
                }
            }
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            state.windows.new_window(toplevel.id());
        }
    }

    event_created_child!(State, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = handle.id();
        let change = match event {
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                state.windows.title(&id, title);
                None
            }
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                state.windows.app_id(&id, app_id);
                None
            }
            zwlr_foreign_toplevel_handle_v1::Event::State { state: flags } => {
                let activated = flags.as_chunks::<4>().0.iter().any(|b| u32::from_ne_bytes(*b) == ACTIVATED);
                state.windows.state(&id, activated);
                None
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => state.windows.done(&id),
            zwlr_foreign_toplevel_handle_v1::Event::Closed => state.windows.closed(&id),
            _ => None,
        };
        state.changes.extend(change);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn focused(app: &str, title: &str) -> Option<Change> {
        Some(Change::Focused { app_id: app.into(), title: title.into() })
    }

    #[test]
    fn activating_a_window_reports_it_once() {
        let mut w: Windows<u32> = Windows::default();
        w.new_window(1);
        w.app_id(&1, "foot".into());
        w.title(&1, "~".into());
        w.state(&1, true);
        assert_eq!(w.done(&1), focused("foot", "~"));
        assert_eq!(w.done(&1), None, "the same window and title don't report again");
    }

    #[test]
    fn moving_focus_reports_the_new_window_and_drops_the_old() {
        let mut w: Windows<u32> = Windows::default();
        for (id, app) in [(1, "foot"), (2, "kitty")] {
            w.new_window(id);
            w.app_id(&id, app.into());
        }
        w.state(&1, true);
        w.done(&1);
        w.state(&1, false);
        w.state(&2, true);
        assert_eq!(w.done(&1), Some(Change::Unfocused));
        assert_eq!(w.done(&2), focused("kitty", ""));
    }

    #[test]
    fn a_title_change_on_the_focused_window_reports_again() {
        let mut w: Windows<u32> = Windows::default();
        w.new_window(1);
        w.app_id(&1, "foot".into());
        w.state(&1, true);
        w.done(&1);
        w.title(&1, "vim".into());
        assert_eq!(w.done(&1), focused("foot", "vim"));
    }

    #[test]
    fn closing_the_focused_window_reports_unfocused() {
        let mut w: Windows<u32> = Windows::default();
        w.new_window(1);
        w.state(&1, true);
        w.done(&1);
        assert_eq!(w.closed(&1), Some(Change::Unfocused));
        assert_eq!(w.closed(&1), None, "a closed window reports nothing more");
    }

    #[test]
    fn closing_an_unfocused_window_reports_nothing() {
        let mut w: Windows<u32> = Windows::default();
        w.new_window(1);
        w.new_window(2);
        w.state(&2, true);
        w.done(&2);
        assert_eq!(w.closed(&1), None);
    }
}

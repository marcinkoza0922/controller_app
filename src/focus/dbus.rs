//! The D-Bus service that desktop-side helpers (the KWin script)
//! report focus changes to, and a few helpers for calling the desktop's own services.

use anyhow::{Context, Result};
use zbus::blocking::Connection;

use super::{FocusEvent, Notify, identify};

pub const BUS_NAME: &str = "io.github.marcinkoza0922.Padwight";
pub const OBJECT_PATH: &str = "/Focus";
pub const INTERFACE: &str = "io.github.marcinkoza0922.Padwight.Focus";

struct FocusService {
    notify: Notify,
}

#[zbus::interface(name = "io.github.marcinkoza0922.Padwight.Focus")]
impl FocusService {
    /// The pid is a string because the KWin script's callDBus would send JS numbers as doubles.
    fn window_activated(&self, class: String, pid: &str, title: String) {
        let pid = pid.parse().unwrap_or(0);
        (self.notify)(FocusEvent::Focused(identify(class, title, pid)));
    }
}

pub fn serve(notify: Notify) -> Result<Connection> {
    let conn = zbus::blocking::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, FocusService { notify })?
        .build()
        .context("connecting to the session bus")?;
    Ok(conn)
}

/// Calls a method of the desktop's own services. `target` is destination, object path and interface.
pub fn call<B>(conn: &Connection, target: (&str, &str, &str), method: &str, body: &B) -> Result<zbus::message::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let (dest, path, iface) = target;
    Ok(conn.call_method(Some(dest), path, Some(iface), method, body)?)
}

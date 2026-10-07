//! GNOME Shell: it offers other programs no way to ask for the focused window, so the daemon
//! installs a small Shell extension (`dist/gnome-extension/`) that reports focus changes to our
//! D-Bus service.

use std::{collections::HashMap, fs};

use anyhow::Result;
use zbus::{blocking::Connection, zvariant::OwnedValue};

use super::dbus::{call, name_has_owner};
use crate::monitor::log;

const SHELL: &str = "org.gnome.Shell";
const UUID: &str = "controller-app-focus@marcinkoza0922.github.io";
const METADATA: &str = include_str!("../../dist/gnome-extension/metadata.json");
const EXTENSION: &str = include_str!("../../dist/gnome-extension/extension.js");
/// `ExtensionState.ACTIVE` in GNOME Shell.
const ACTIVE: f64 = 1.0;

pub enum Gnome {
    /// This isn't a GNOME session.
    Absent,
    Active,
    /// The extension isn't running yet; the text says what the user can do.
    Inactive(&'static str),
}

/// Makes sure the extension is installed and switched on, and says whether it is running. (An
/// extension.js that changed after the Shell loaded it only takes effect in the next session.)
pub fn ensure(conn: &Connection) -> Gnome {
    if !name_has_owner(conn, SHELL) {
        return Gnome::Absent;
    }
    if let Err(e) = install() {
        log!("couldn't install the GNOME Shell extension: {e:#}");
    }
    let known = state(conn).is_some();
    if known {
        let _ = enable(conn);
    }
    match state(conn) {
        Some(s) if s == ACTIVE => Gnome::Active,
        Some(_) => Gnome::Inactive("the GNOME Shell extension isn't running; switch on \"controller_app focus\" in the Extensions app"),
        None => Gnome::Inactive("installed the GNOME Shell extension for window tracking; log out and back in to start it"),
    }
}

/// Writes the extension into the user's extensions directory when missing or out of date.
fn install() -> Result<()> {
    let dir = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("no data directory"))?.join("gnome-shell/extensions").join(UUID);
    fs::create_dir_all(&dir)?;
    for (name, contents) in [("metadata.json", METADATA), ("extension.js", EXTENSION)] {
        let path = dir.join(name);
        if fs::read_to_string(&path).ok().as_deref() != Some(contents) {
            fs::write(&path, contents)?;
        }
    }
    Ok(())
}

/// Our extension's state as the Shell reports it, `None` while the Shell doesn't know it.
fn state(conn: &Connection) -> Option<f64> {
    let reply = call(conn, (SHELL, "/org/gnome/Shell", "org.gnome.Shell.Extensions"), "ListExtensions", &()).ok()?;
    let list: HashMap<String, HashMap<String, OwnedValue>> = reply.body().deserialize().ok()?;
    list.get(UUID)?.get("state")?.downcast_ref::<f64>().ok()
}

fn enable(conn: &Connection) -> Result<()> {
    call(conn, (SHELL, "/org/gnome/Shell", "org.gnome.Shell.Extensions"), "EnableExtension", &(UUID,))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus::dbus::{BUS_NAME, INTERFACE, OBJECT_PATH};

    #[test]
    fn the_extension_talks_to_our_service() {
        for name in [BUS_NAME, INTERFACE] {
            assert!(EXTENSION.contains(&format!("'{name}'")), "{name}");
        }
        assert!(EXTENSION.contains(&format!("'{OBJECT_PATH}'")));
        assert!(METADATA.contains(&format!("\"{UUID}\"")));
        serde_json::from_str::<serde_json::Value>(METADATA).unwrap();
    }
}

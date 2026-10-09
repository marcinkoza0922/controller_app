//! GNOME (Mutter): a small GNOME Shell extension reports every focus change to our D-Bus
//! service. We install it into the user's extensions directory and enable it over D-Bus.

use std::{collections::HashMap, fs, path::{Path, PathBuf}};

use anyhow::{Context, Result, bail};
use zbus::{blocking::Connection, zvariant::OwnedValue};

use super::dbus::call;
use crate::monitor::log;

const UUID: &str = "padwight-focus@io.github.marcinkoza0922";
const SHELL: &str = "org.gnome.Shell";
const EXTENSIONS: (&str, &str, &str) = (SHELL, "/org/gnome/Shell", "org.gnome.Shell.Extensions");
/// `state` of an extension that is loaded and running.
const ENABLED: f64 = 1.0;

/// The extension's files, compiled in so the app can install them anywhere.
const FILES: [(&str, &str); 2] = [
    ("metadata.json", include_str!("../../gnome-extension/padwight-focus@io.github.marcinkoza0922/metadata.json")),
    ("extension.js", include_str!("../../gnome-extension/padwight-focus@io.github.marcinkoza0922/extension.js")),
];

/// Whether GNOME Shell is on the session bus.
pub fn is_running(conn: &Connection) -> bool {
    call(conn, ("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus"), "NameHasOwner", &(SHELL,))
        .and_then(|reply| Ok(reply.body().deserialize::<bool>()?))
        .unwrap_or(false)
}

/// Installs the extension if needed and makes sure GNOME Shell has it running. Errors when it
/// isn't running yet, such as right after a first install, and the caller tries again later.
/// A running extension keeps its old code until the next log in, so an update is only reported.
pub fn ensure_extension(conn: &Connection) -> Result<()> {
    let updated = install()?;
    if is_enabled(conn)? {
        if updated {
            log!("the focus extension was updated: log out and back in to load it");
        }
        return Ok(());
    }
    let _: bool = call(conn, EXTENSIONS, "EnableExtension", &(UUID,))?.body().deserialize()?;
    if is_enabled(conn)? {
        Ok(())
    } else {
        bail!("GNOME Shell hasn't loaded {UUID} yet (a log out and back in may be needed)")
    }
}

fn is_enabled(conn: &Connection) -> Result<bool> {
    // An unknown extension has no info, which reads as not enabled.
    let info: HashMap<String, OwnedValue> = call(conn, EXTENSIONS, "GetExtensionInfo", &(UUID,))?.body().deserialize()?;
    Ok(info.get("state").and_then(|state| f64::try_from(state).ok()) == Some(ENABLED))
}

/// Writes the extension's files, leaving alone any that already match. Returns whether any were
/// written.
fn install() -> Result<bool> {
    write_files(&extension_dir()?)
}

fn write_files(dir: &Path) -> Result<bool> {
    fs::create_dir_all(dir)?;
    let mut written = false;
    for (name, content) in FILES {
        let path = dir.join(name);
        if fs::read_to_string(&path).ok().as_deref() != Some(content) {
            fs::write(&path, content)?;
            written = true;
        }
    }
    Ok(written)
}

fn extension_dir() -> Result<PathBuf> {
    // Inside a Flatpak, XDG_DATA_HOME is the app's own directory. GNOME reads the host's, which
    // the manifest grants at the same path.
    let data = if std::env::var_os("FLATPAK_ID").is_some() {
        dirs::home_dir().map(|home| home.join(".local/share"))
    } else {
        dirs::data_dir()
    };
    Ok(data.context("no data directory")?.join("gnome-shell/extensions").join(UUID))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_writes_only_what_changed() {
        let dir = std::env::temp_dir().join(format!("padwight-gnome-install-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert!(write_files(&dir).unwrap(), "a new install writes the files");
        assert!(!write_files(&dir).unwrap(), "a current install writes nothing");
        let (name, content) = FILES[1];
        fs::write(dir.join(name), "an older extension").unwrap();
        assert!(write_files(&dir).unwrap(), "an out-of-date file is rewritten");
        assert_eq!(fs::read_to_string(dir.join(name)).unwrap(), content);
        fs::remove_dir_all(&dir).unwrap();
    }
}

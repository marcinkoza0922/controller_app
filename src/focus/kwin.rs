//! KDE Plasma (Wayland or X11): a KWin script reports every focus change to our D-Bus service.

use std::{fs, path::PathBuf};

use anyhow::Result;
use zbus::blocking::Connection;

use super::dbus::{BUS_NAME, INTERFACE, OBJECT_PATH, call};

const SCRIPT_NAME: &str = "padwight_focus";
const KWIN: &str = "org.kde.KWin";

/// KWin script: reports every focus change to our D-Bus service. Everything is sent as a
/// string because callDBus would send JS numbers as doubles.
fn script() -> String {
    format!(
        r#"function report(w) {{
    if (!w) return;
    callDBus("{BUS_NAME}", "{OBJECT_PATH}", "{INTERFACE}", "WindowActivated",
             String(w.resourceClass || ""), String(w.pid || 0), String(w.caption || ""));
}}
workspace.windowActivated.connect(report);
report(workspace.activeWindow);
"#
    )
}

fn kwin_call<B>(conn: &Connection, path: &str, iface: &str, method: &str, body: &B) -> Result<zbus::message::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    call(conn, (KWIN, path, iface), method, body)
}

/// Loads and starts our KWin script unless it is already loaded.
pub fn ensure_script(conn: &Connection) -> Result<()> {
    let loaded: bool = kwin_call(conn, "/Scripting", "org.kde.kwin.Scripting", "isScriptLoaded", &(SCRIPT_NAME,))?
        .body()
        .deserialize()?;
    if loaded {
        return Ok(());
    }
    let path = script_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&path, script())?;
    let id: i32 = kwin_call(
        conn,
        "/Scripting",
        "org.kde.kwin.Scripting",
        "loadScript",
        &(path.to_string_lossy().as_ref(), SCRIPT_NAME),
    )?
    .body()
    .deserialize()?;
    kwin_call(conn, &format!("/Scripting/Script{id}"), "org.kde.kwin.Script", "run", &())?;
    Ok(())
}

/// Removes our script from KWin. The daemon leaves it loaded on exit (its calls then go
/// nowhere) and replaces it on the next start.
pub fn unload_script(conn: &Connection) -> Result<()> {
    kwin_call(conn, "/Scripting", "org.kde.kwin.Scripting", "unloadScript", &(SCRIPT_NAME,))?;
    Ok(())
}

fn script_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("padwight")
        .join("kwin_focus.js")
}

#[cfg(test)]
mod tests {
    use std::{sync::{Arc, mpsc}, time::Duration};

    use super::*;
    use crate::focus::FocusEvent;

    /// Live check against KWin on this machine: our script loads and immediately reports the
    /// active window over D-Bus. Opt-in: `cargo test -- --ignored kwin`.
    #[test]
    #[ignore]
    fn kwin_reports_active_window() {
        let (tx, rx) = mpsc::channel();
        let conn = crate::focus::dbus::serve(Arc::new(move |ev| {
            let _ = tx.send(ev);
        }))
        .unwrap();
        // Start from a clean slate in case a daemon left the script loaded.
        let _ = unload_script(&conn);
        ensure_script(&conn).unwrap();
        let event = rx.recv_timeout(Duration::from_secs(5)).expect("no focus report from KWin");
        unload_script(&conn).unwrap();
        let FocusEvent::Focused(info) = event else { panic!("expected a focus event") };
        assert!(info.pid > 0, "{info:?}");
        assert!(!info.exe.is_empty(), "{info:?}");
    }
}

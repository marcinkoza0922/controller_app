//! Tray icon (StatusNotifierItem) for a daemon that nobody's service manager is watching. Under
//! systemd the unit is the interface, so the tray stays off there.

use ksni::{
    Icon, MenuItem, ToolTip, Tray,
    blocking::{Handle, TrayMethods},
    menu::{CheckmarkItem, StandardItem},
};

use crate::icon;

/// A tray menu choice, for the daemon to carry out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    ToggleEnabled,
    OpenSettings,
    Quit,
}

/// What the tray displays, kept current by the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub enabled: bool,
    pub profile: String,
}

/// Sizes the host may pick from; hosts scale whichever they need.
const SIZES: [u32; 2] = [24, 48];

/// Whether the daemon should show a tray icon: not when systemd supervises it.
pub fn wanted() -> bool {
    !supervised_by_systemd(cgroup_leaf().as_deref(), std::env::var_os("INVOCATION_ID").is_some())
}

/// Decides from the process's cgroup, the unit systemd puts a service in. `INVOCATION_ID` alone
/// is not enough: a terminal started from a service inherits it too.
fn supervised_by_systemd(leaf: Option<&str>, invocation_id: bool) -> bool {
    match leaf {
        Some(leaf) if leaf.ends_with(".service") => true,
        Some(leaf) if leaf.ends_with(".scope") => false,
        // Inside a sandbox the cgroup path can be hidden; fall back to systemd's variable.
        _ => invocation_id,
    }
}

/// The last component of this process's cgroup path (cgroup v2), if it has one.
fn cgroup_leaf() -> Option<String> {
    let cgroups = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let path = cgroups.lines().find_map(|line| line.strip_prefix("0::"))?;
    path.rsplit('/').next().filter(|leaf| !leaf.is_empty()).map(str::to_owned)
}

/// A running tray icon. Dropping it does not remove it; the daemon's exit does.
pub struct TrayIcon {
    handle: Handle<Padwight>,
    shown: Shown,
}

impl TrayIcon {
    /// Puts the icon on the tray, reporting menu choices through `on_command`. `None` when the
    /// session has no tray to join; the daemon works without one.
    pub fn start(shown: Shown, on_command: impl Fn(Command) + Send + 'static) -> Option<Self> {
        let tray = Padwight { shown: shown.clone(), on_command: Box::new(on_command) };
        // A Flatpak can't own the StatusNotifierItem bus name, so it must not ask for it.
        let handle = tray.disable_dbus_name(std::env::var_os("FLATPAK_ID").is_some()).spawn().ok()?;
        Some(Self { handle, shown })
    }

    /// Updates the icon's tooltip and menu if `shown` differs from what it shows.
    pub fn show(&mut self, shown: &Shown) {
        if &self.shown == shown {
            return;
        }
        self.shown = shown.clone();
        let shown = shown.clone();
        self.handle.update(move |tray| tray.shown = shown);
    }
}

struct Padwight {
    shown: Shown,
    on_command: Box<dyn Fn(Command) + Send>,
}

impl Tray for Padwight {
    fn id(&self) -> String {
        "padwight".into()
    }

    fn title(&self) -> String {
        "Padwight".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        SIZES.iter().map(|&size| argb_icon(size)).collect()
    }

    fn tool_tip(&self) -> ToolTip {
        let state = if self.shown.enabled { "Remapping on" } else { "Remapping off" };
        let description = if self.shown.profile.is_empty() {
            state.to_string()
        } else {
            format!("{state}: {}", self.shown.profile)
        };
        ToolTip { title: "Padwight".into(), description, ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        (self.on_command)(Command::OpenSettings);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            CheckmarkItem {
                label: "Remapping".into(),
                checked: self.shown.enabled,
                activate: Box::new(|tray: &mut Self| (tray.on_command)(Command::ToggleEnabled)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open settings".into(),
                activate: Box::new(|tray: &mut Self| (tray.on_command)(Command::OpenSettings)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| (tray.on_command)(Command::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// The icon as the ARGB32, network byte order, that StatusNotifierItem asks for.
fn argb_icon(size: u32) -> Icon {
    let data = icon::rgba(size)
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|px| [px[3], px[0], px[1], px[2]])
        .collect();
    Icon { width: size as i32, height: size as i32, data }
}

#[cfg(test)]
mod tests {
    use super::supervised_by_systemd;

    #[test]
    fn service_cgroup_is_supervised() {
        assert!(supervised_by_systemd(Some("padwight.service"), false));
    }

    #[test]
    fn terminal_scope_is_not_supervised_even_with_invocation_id() {
        // A kitty started from Plasma inherits INVOCATION_ID but sits in its own scope.
        assert!(!supervised_by_systemd(Some("kitty-56853-0.scope"), true));
    }

    #[test]
    fn hidden_cgroup_falls_back_to_invocation_id() {
        assert!(supervised_by_systemd(None, true));
        assert!(!supervised_by_systemd(None, false));
    }
}

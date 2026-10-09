//! Which game is active: a tracker for the running desktop reports the focused window, windows
//! and processes are identified from /proc (see [`identify`]), and per-game rules pick the
//! profile. Desktops without a tracker fall back to matching running processes.

mod dbus;
mod gnome;
mod hyprland;
mod identify;
mod kwin;
mod sway;
mod wlroots;

use std::{sync::Arc, thread, time::Duration};

use zbus::blocking::Connection;

use crate::{
    config::{Config, ProfileRef, Rule, RuleKind},
    ipc::{FocusBackend, WindowInfo},
    monitor::log,
};
pub use identify::{base_name, identify, is_own_window, running_processes};

/// How often we check that the desktop's tracker is still there (a compositor restart, a late
/// login, an extension switched on).
const WATCHDOG: Duration = Duration::from_secs(10);

/// Events the focus tracker sends to the daemon.
pub enum FocusEvent {
    Focused(WindowInfo),
    /// Nothing has focus, such as the desktop or an empty workspace.
    Unfocused,
    Backend(FocusBackend),
}

type Notify = Arc<dyn Fn(FocusEvent) + Send + Sync>;

/// What a compositor tells us about the focused window, before we look at its process.
struct Reported {
    class: String,
    title: String,
    pid: u32,
}

fn report(notify: &Notify, window: Reported) {
    notify(FocusEvent::Focused(identify(window.class, window.title, window.pid)));
}

/// Tells the daemon about changes of backend, and logs them once.
struct Reporter {
    notify: Notify,
    backend: Option<FocusBackend>,
}

impl Reporter {
    fn backend(&mut self, now: FocusBackend) {
        if self.backend != Some(now) {
            log!("focus tracking: {}", now.label());
            (self.notify)(FocusEvent::Backend(now));
            self.backend = Some(now);
        }
    }
}

/// Starts focus tracking: reports which tracker is working through `Backend`, and keeps
/// retrying so it recovers when the desktop's shell appears or restarts.
pub fn spawn(notify: impl Fn(FocusEvent) + Send + Sync + 'static) {
    let notify: Notify = Arc::new(notify);
    thread::spawn(move || {
        let conn = match dbus::serve(notify.clone()) {
            Ok(conn) => Some(conn),
            Err(e) => {
                log!("focus tracking: no session bus ({e:#})");
                None
            }
        };
        // A script left by an earlier run would not report the current window until focus
        // changes; reloading it reports immediately.
        if let Some(conn) = &conn {
            let _ = kwin::unload_script(conn);
        }
        let mut reporter = Reporter { notify: notify.clone(), backend: None };
        loop {
            track(&mut reporter, conn.as_ref());
            thread::sleep(WATCHDOG);
        }
    });
}

/// One round of tracking. Sway and Hyprland push events, so this returns only when that
/// connection ends; the rest are checked once.
fn track(reporter: &mut Reporter, conn: Option<&Connection>) {
    let notify = reporter.notify.clone();
    if let Ok(sway) = sway::connect() {
        reporter.backend(FocusBackend::Sway);
        if let Err(e) = sway.follow(&notify) {
            log!("lost Sway: {e:#}");
        }
    } else if let Ok(hypr) = hyprland::connect() {
        reporter.backend(FocusBackend::Hyprland);
        if let Err(e) = hypr.follow(&notify) {
            log!("lost Hyprland: {e:#}");
        }
    } else if let Some(backend) = conn.and_then(shell_backend) {
        reporter.backend(backend);
    } else if let Ok(wl) = wlroots::connect() {
        reporter.backend(FocusBackend::Wlroots);
        if let Err(e) = wl.follow(&notify) {
            log!("lost the Wayland compositor: {e:#}");
        }
    } else {
        reporter.backend(FocusBackend::ProcessScan);
    }
}

/// The tracker for a desktop reached over D-Bus: KWin, or GNOME Shell with our extension. `None`
/// when the desktop isn't one of those.
fn shell_backend(conn: &Connection) -> Option<FocusBackend> {
    if kwin::ensure_script(conn).is_ok() {
        Some(FocusBackend::Kwin)
    } else if gnome::is_running(conn) && gnome::ensure_extension(conn).is_ok() {
        Some(FocusBackend::GnomeShell)
    } else {
        None
    }
}

fn rule_matches(kind: RuleKind, value: &str, info: &WindowInfo) -> bool {
    let value = value.trim();
    !value.is_empty()
        && match kind {
            RuleKind::Executable => info.exe.eq_ignore_ascii_case(value),
            RuleKind::SteamAppId => info.steam_app_id.as_deref() == Some(value),
            RuleKind::WindowClass => info.class.eq_ignore_ascii_case(value),
        }
}

/// Every game's switched-on rules, in order (games in order, then each game's rules).
pub fn rules(config: &Config) -> impl Iterator<Item = (&str, &Rule)> {
    config.games.iter().flat_map(|g| g.rules.iter().filter(|r| r.enabled).map(move |r| (g.name.as_str(), r)))
}

/// The first rule that matches a focused window, with its game's name.
pub fn matching_rule<'a>(config: &'a Config, info: &WindowInfo) -> Option<(&'a str, &'a Rule)> {
    rules(config).find(|(_, r)| rule_matches(r.kind, &r.value, info))
}

/// Profile for a focused window: the first matching rule's, `None` if it isn't a game's.
pub fn profile_for(config: &Config, info: &WindowInfo) -> Option<ProfileRef> {
    matching_rule(config, info).map(|(game, r)| ProfileRef::new(Some(game), &r.profile))
}

/// The first rule (in rule order) that matches any running process, with that process.
pub fn matching_process<'a>(config: &'a Config, processes: &'a [WindowInfo]) -> Option<(&'a str, &'a Rule, &'a WindowInfo)> {
    rules(config).find_map(|(game, r)| processes.iter().find(|p| rule_matches(r.kind, &r.value, p)).map(|p| (game, r, p)))
}

/// Profile from running processes, for desktops without focus information.
pub fn profile_for_processes(config: &Config, processes: &[WindowInfo]) -> Option<ProfileRef> {
    matching_process(config, processes).map(|(game, r, _)| ProfileRef::new(Some(game), &r.profile))
}

/// A rule as people read it, for the reason a profile was switched: `Executable "doom.exe"`.
pub fn rule_text(rule: &Rule) -> String {
    format!("{} “{}”", rule.kind, rule.value)
}

/// Whether rules can pick `game`, so it has focus only while one matches.
pub fn has_rules(config: &Config, game: &str) -> bool {
    rules(config).any(|(g, _)| g == game)
}

/// The game a rule matches this window to, and the window's process: a launch is the first
/// focus of a game with a process it hasn't had. (The default profile isn't a game launch.)
pub fn game_launch(config: &Config, info: &WindowInfo) -> Option<(String, u32)> {
    rules(config).find(|(_, r)| rule_matches(r.kind, &r.value, info)).map(|(game, _)| (game.to_string(), info.pid))
}

/// Like [`game_launch`], for the first rule that matches a running process.
pub fn game_launch_in(config: &Config, processes: &[WindowInfo]) -> Option<(String, u32)> {
    rules(config).find_map(|(game, r)| {
        processes.iter().find(|p| rule_matches(r.kind, &r.value, p)).map(|p| (game.to_string(), p.pid))
    })
}

/// Whether a rule matches any of these windows or processes.
/// Rule for a window using its most specific identifier: Steam App ID, then a Windows `.exe`
/// name, then the window class, then the native executable name.
pub fn rule_for_window(w: &WindowInfo, profile: String) -> Rule {
    let (kind, value) = if let Some(id) = &w.steam_app_id {
        (RuleKind::SteamAppId, id.clone())
    } else if w.exe.to_ascii_lowercase().ends_with(".exe") {
        (RuleKind::Executable, w.exe.clone())
    } else if !w.class.is_empty() {
        (RuleKind::WindowClass, w.class.clone())
    } else {
        (RuleKind::Executable, w.exe.clone())
    };
    Rule::new(kind, value, profile)
}

pub fn rule_matches_any(rule: &Rule, seen: &[WindowInfo]) -> bool {
    seen.iter().any(|w| rule_matches(rule.kind, &rule.value, w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Rule;


    /// A config with one game per (game, rules) entry, each rule naming profile "P".
    fn config(games: &[(&str, &[(RuleKind, &str)])]) -> Config {
        let mut config = Config::default();
        for (name, rules) in games {
            let mut game = crate::config::Game::new(name, vec![crate::config::Profile::passthrough("P")]);
            game.rules = rules.iter().map(|(kind, value)| Rule::new(*kind, *value, "P")).collect();
            config.games.push(game);
        }
        config
    }

    fn at(game: &str) -> Option<ProfileRef> {
        Some(ProfileRef::new(Some(game), "P"))
    }

    #[test]
    fn first_matching_rule_wins() {
        let mut config = config(&[
            ("Souls", &[(RuleKind::SteamAppId, "1245620")]),
            ("Other", &[(RuleKind::Executable, "ELDENRING.EXE")]),
            ("Factorio", &[(RuleKind::WindowClass, "factorio")]),
        ]);
        let elden = WindowInfo {
            class: "steam_app_1245620".into(),
            exe: "eldenring.exe".into(),
            steam_app_id: Some("1245620".into()),
            ..Default::default()
        };
        assert_eq!(profile_for(&config, &elden), at("Souls"));
        let factorio = WindowInfo { class: "Factorio".into(), exe: "factorio".into(), ..Default::default() };
        assert_eq!(profile_for(&config, &factorio), at("Factorio"));

        let browser = WindowInfo { class: "firefox".into(), exe: "firefox".into(), ..Default::default() };
        assert_eq!(profile_for(&config, &browser), None, "the default is the daemon's call");
        assert!(has_rules(&config, "Souls") && !has_rules(&config, "General"));

        config.games[0].rules[0].enabled = false;
        assert_eq!(profile_for(&config, &elden), at("Other"), "switched-off rules are skipped");
    }

    #[test]
    fn empty_rule_values_never_match() {
        let config = config(&[("X", &[(RuleKind::WindowClass, " ")])]);
        assert_eq!(profile_for(&config, &WindowInfo::default()), None);
    }

    #[test]
    fn process_scan_uses_rule_order() {
        let config = config(&[("A", &[(RuleKind::Executable, "game.exe")]), ("B", &[(RuleKind::Executable, "bash")])]);
        let procs = |names: &[&str]| -> Vec<WindowInfo> {
            names.iter().map(|n| WindowInfo { exe: n.to_string(), ..Default::default() }).collect()
        };
        assert_eq!(profile_for_processes(&config, &procs(&["bash", "game.exe"])), at("A"));
        assert_eq!(profile_for_processes(&config, &procs(&["bash"])), at("B"));
        assert_eq!(profile_for_processes(&config, &procs(&["zsh"])), None);
    }

    #[test]
    fn launches_come_from_rules_with_the_matching_process() {
        let mut config = config(&[("Doom", &[(RuleKind::Executable, "doom.exe")])]);
        config.auto_switch.default_profile = Some(ProfileRef::new(None, "Desktop"));
        let doom = WindowInfo { exe: "doom.exe".into(), pid: 42, ..Default::default() };
        assert_eq!(game_launch(&config, &doom), Some(("Doom".into(), 42)));
        let shell = WindowInfo { exe: "bash".into(), pid: 7, ..Default::default() };
        assert_eq!(game_launch(&config, &shell), None, "the default profile isn't a launch");
        assert_eq!(game_launch_in(&config, &[shell, doom]), Some(("Doom".into(), 42)));
    }
}

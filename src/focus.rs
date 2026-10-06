//! Which game is active: focus tracking through a KWin script, identification of windows and
//! processes from /proc (including Wine/Proton games), and per-game rule matching.

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use zbus::blocking::Connection;

use crate::{
    config::{Config, ProfileRef, Rule, RuleKind},
    ipc::{FocusBackend, WindowInfo},
    monitor::log,
};

pub const BUS_NAME: &str = "io.github.marcinkoza0922.ControllerApp";
const OBJECT_PATH: &str = "/Focus";
const INTERFACE: &str = "io.github.marcinkoza0922.ControllerApp.Focus";
const SCRIPT_NAME: &str = "controller_app_focus";
/// How often we check that KWin still has our script (KWin restarts, late login).
const WATCHDOG: Duration = Duration::from_secs(10);

/// Events the focus tracker sends to the daemon.
pub enum FocusEvent {
    Focused(WindowInfo),
    Backend(FocusBackend),
}

/// KWin script: reports every focus change to our D-Bus service. Everything is sent as a
/// string because callDBus would send JS numbers as doubles.
fn kwin_script() -> String {
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

type Notify = Arc<dyn Fn(FocusEvent) + Send + Sync>;

struct FocusService {
    notify: Notify,
}

#[zbus::interface(name = "io.github.marcinkoza0922.ControllerApp.Focus")]
impl FocusService {
    fn window_activated(&self, class: String, pid: String, title: String) {
        let pid = pid.parse().unwrap_or(0);
        (self.notify)(FocusEvent::Focused(identify(class, title, pid)));
    }
}

/// Starts focus tracking. Reports `Backend(Kwin)` while our KWin script is loaded, otherwise
/// `Backend(ProcessScan)`, and keeps retrying so it recovers when KWin appears.
pub fn spawn(notify: impl Fn(FocusEvent) + Send + Sync + 'static) {
    let notify: Notify = Arc::new(notify);
    thread::spawn(move || {
        let conn = match serve(notify.clone()) {
            Ok(conn) => conn,
            Err(e) => {
                log!("focus tracking unavailable ({e:#}); matching running processes instead");
                notify(FocusEvent::Backend(FocusBackend::ProcessScan));
                return;
            }
        };
        // A script left by an earlier run would not report the current window until focus
        // changes; reloading it reports immediately.
        let _ = unload_kwin_script(&conn);
        let mut backend = None;
        loop {
            let now = match ensure_kwin_script(&conn) {
                Ok(()) => FocusBackend::Kwin,
                Err(_) => FocusBackend::ProcessScan,
            };
            if backend != Some(now) {
                match now {
                    FocusBackend::Kwin => log!("focus tracking: KWin"),
                    FocusBackend::ProcessScan => log!("focus tracking: KWin not available, matching running processes"),
                }
                notify(FocusEvent::Backend(now));
                backend = Some(now);
            }
            thread::sleep(WATCHDOG);
        }
    });
}

fn serve(notify: Notify) -> Result<Connection> {
    let conn = zbus::blocking::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, FocusService { notify })?
        .build()
        .context("connecting to the session bus")?;
    Ok(conn)
}

fn kwin_call<B>(conn: &Connection, path: &str, iface: &str, method: &str, body: &B) -> Result<zbus::message::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    Ok(conn.call_method(Some("org.kde.KWin"), path, Some(iface), method, body)?)
}

/// Loads and starts our KWin script unless it is already loaded.
fn ensure_kwin_script(conn: &Connection) -> Result<()> {
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
    fs::write(&path, kwin_script())?;
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
fn unload_kwin_script(conn: &Connection) -> Result<()> {
    kwin_call(conn, "/Scripting", "org.kde.kwin.Scripting", "unloadScript", &(SCRIPT_NAME,))?;
    Ok(())
}

fn script_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("controller_app")
        .join("kwin_focus.js")
}

/// Builds a [`WindowInfo`] for a window owned by `pid`, reading its process from /proc.
pub fn identify(class: String, title: String, pid: u32) -> WindowInfo {
    let mut info = process_info(pid).unwrap_or_default();
    // Proton names its windows `steam_app_<id>` even when the environment is unreadable.
    if info.steam_app_id.is_none() {
        info.steam_app_id = class.strip_prefix("steam_app_").filter(|id| is_app_id(id)).map(str::to_string);
    }
    WindowInfo { class, title, pid, ..info }
}

/// Executable and Steam App ID of a process, or `None` if it is gone or not ours.
fn process_info(pid: u32) -> Option<WindowInfo> {
    if pid == 0 {
        return None;
    }
    let dir = PathBuf::from(format!("/proc/{pid}"));
    let cmdline = fs::read(dir.join("cmdline")).ok()?;
    let exe_path = fs::read_link(dir.join("exe")).ok();
    let environ = fs::read(dir.join("environ")).unwrap_or_default();
    Some(WindowInfo {
        pid,
        exe: exe_name(&cmdline, exe_path.as_deref()),
        steam_app_id: steam_app_id(&environ),
        ..WindowInfo::default()
    })
}

/// File name of what a process is running. Wine/Proton processes run a loader binary but put
/// the Windows program (`C:\Games\Foo\foo.exe`) in their command line, so prefer that.
fn exe_name(cmdline: &[u8], exe_path: Option<&Path>) -> String {
    let args: Vec<String> = cmdline
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    let windows_exe = args.iter().find(|a| a.to_ascii_lowercase().ends_with(".exe"));
    if let Some(exe) = windows_exe {
        return base_name(exe);
    }
    exe_path
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().trim_end_matches(" (deleted)").to_string())
        .or_else(|| args.first().map(|a| base_name(a)))
        .unwrap_or_default()
}

/// Last component of a Unix or Windows path.
fn base_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// Steam sets these for games it launches (and Proton for Windows games).
fn steam_app_id(environ: &[u8]) -> Option<String> {
    let vars: Vec<(&[u8], &[u8])> = environ
        .split(|b| *b == 0)
        .filter_map(|kv| {
            let eq = kv.iter().position(|b| *b == b'=')?;
            Some((&kv[..eq], &kv[eq + 1..]))
        })
        .collect();
    ["SteamAppId", "SteamGameId", "STEAM_COMPAT_APP_ID"].iter().find_map(|key| {
        vars.iter()
            .find(|(k, _)| *k == key.as_bytes())
            .map(|(_, v)| String::from_utf8_lossy(v).into_owned())
            .filter(|id| is_app_id(id))
    })
}

/// Real app IDs are non-zero numbers (Steam uses 0 for "no game").
fn is_app_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) && !id.trim_start_matches('0').is_empty()
}

/// True if this window belongs to controller_app itself (the settings GUI).
pub fn is_own_window(info: &WindowInfo) -> bool {
    let Ok(own) = std::env::current_exe() else { return false };
    fs::read_link(format!("/proc/{}/exe", info.pid)).is_ok_and(|exe| exe == own)
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

/// Profile for a focused window: the first matching rule's, else the default (if any).
pub fn profile_for(config: &Config, info: &WindowInfo) -> Option<ProfileRef> {
    rules(config)
        .find(|(_, r)| rule_matches(r.kind, &r.value, info))
        .map(|(game, r)| ProfileRef::new(Some(game), &r.profile))
        .or_else(|| config.auto_switch.default_profile.clone())
}

/// Profile from running processes, for desktops without focus information: the first rule
/// (in rule order) that matches any running process, else the default.
pub fn profile_for_processes(config: &Config, processes: &[WindowInfo]) -> Option<ProfileRef> {
    rules(config)
        .find(|(_, r)| processes.iter().any(|p| rule_matches(r.kind, &r.value, p)))
        .map(|(game, r)| ProfileRef::new(Some(game), &r.profile))
        .or_else(|| config.auto_switch.default_profile.clone())
}

/// Whether a rule matches any of these windows or processes.
pub fn rule_matches_any(rule: &Rule, seen: &[WindowInfo]) -> bool {
    seen.iter().any(|w| rule_matches(rule.kind, &rule.value, w))
}

/// This user's running processes.
pub fn running_processes() -> Vec<WindowInfo> {
    let uid = unsafe { libc::getuid() };
    let Ok(entries) = fs::read_dir("/proc") else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            (e.metadata().ok()?.uid() == uid).then_some(pid)
        })
        .filter_map(process_info)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::config::Rule;

    fn nul_joined(parts: &[&str]) -> Vec<u8> {
        parts.iter().flat_map(|p| p.bytes().chain([0])).collect()
    }

    #[test]
    fn wine_processes_report_the_windows_exe() {
        let cmd = nul_joined(&["C:\\Program Files\\ELDEN RING\\Game\\eldenring.exe"]);
        assert_eq!(exe_name(&cmd, Some(Path::new("/usr/bin/wine64-preloader"))), "eldenring.exe");
        // Some launches keep the loader as argv[0] and pass the program as an argument.
        let cmd = nul_joined(&["/opt/proton/files/bin/wine64", "Z:\\home\\me\\Games\\Foo.EXE"]);
        assert_eq!(exe_name(&cmd, None), "Foo.EXE");
    }

    #[test]
    fn native_processes_report_their_binary() {
        let cmd = nul_joined(&["./factorio", "--fullscreen"]);
        assert_eq!(exe_name(&cmd, Some(Path::new("/home/me/factorio/bin/x64/factorio"))), "factorio");
        assert_eq!(exe_name(&cmd, None), "factorio");
    }

    #[test]
    fn steam_app_id_from_environment_skips_zero() {
        let env = nul_joined(&["HOME=/home/me", "SteamAppId=0", "STEAM_COMPAT_APP_ID=1245620"]);
        assert_eq!(steam_app_id(&env).as_deref(), Some("1245620"));
        assert_eq!(steam_app_id(&nul_joined(&["SteamAppId=0"])), None);
    }

    #[test]
    fn proton_window_class_gives_app_id() {
        let info = identify("steam_app_1245620".into(), "ELDEN RING".into(), 0);
        assert_eq!(info.steam_app_id.as_deref(), Some("1245620"));
        assert_eq!(identify("steam_app_0".into(), String::new(), 0).steam_app_id, None);
    }

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
    fn first_matching_rule_wins_else_default() {
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
        assert_eq!(profile_for(&config, &browser), None);
        config.auto_switch.default_profile = Some(ProfileRef::new(None, "Desktop"));
        assert_eq!(profile_for(&config, &browser), Some(ProfileRef::new(None, "Desktop")));

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
        let mut config = config(&[("A", &[(RuleKind::Executable, "game.exe")]), ("B", &[(RuleKind::Executable, "bash")])]);
        config.auto_switch.default_profile = Some(ProfileRef::new(None, "D"));
        let procs = |names: &[&str]| -> Vec<WindowInfo> {
            names.iter().map(|n| WindowInfo { exe: n.to_string(), ..Default::default() }).collect()
        };
        assert_eq!(profile_for_processes(&config, &procs(&["bash", "game.exe"])), at("A"));
        assert_eq!(profile_for_processes(&config, &procs(&["bash"])), at("B"));
        assert_eq!(profile_for_processes(&config, &procs(&["zsh"])), Some(ProfileRef::new(None, "D")));
    }

    #[test]
    fn identifies_this_test_process() {
        let me = std::process::id();
        let info = identify("test".into(), String::new(), me);
        assert!(!info.exe.is_empty());
        assert!(is_own_window(&info));
        assert!(running_processes().iter().any(|p| p.pid == me));
    }

    /// Live check against KWin on this machine: our script loads and immediately reports the
    /// active window over D-Bus. Opt-in: `cargo test -- --ignored kwin`.
    #[test]
    #[ignore]
    fn kwin_reports_active_window() {
        let (tx, rx) = mpsc::channel();
        let conn = serve(Arc::new(move |ev| {
            let _ = tx.send(ev);
        }))
        .unwrap();
        // Start from a clean slate in case a daemon left the script loaded.
        let _ = unload_kwin_script(&conn);
        ensure_kwin_script(&conn).unwrap();
        let event = rx.recv_timeout(Duration::from_secs(5)).expect("no focus report from KWin");
        unload_kwin_script(&conn).unwrap();
        let FocusEvent::Focused(info) = event else { panic!("expected a focus event") };
        assert!(info.pid > 0, "{info:?}");
        assert!(!info.exe.is_empty(), "{info:?}");
    }
}

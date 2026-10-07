//! Identifies windows and processes from /proc, including Wine/Proton games.

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use crate::ipc::WindowInfo;

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
pub fn base_name(path: &str) -> String {
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
    use super::*;

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

    #[test]
    fn identifies_this_test_process() {
        let me = std::process::id();
        let info = identify("test".into(), String::new(), me);
        assert!(!info.exe.is_empty());
        assert!(is_own_window(&info));
        assert!(running_processes().iter().any(|p| p.pid == me));
    }
}

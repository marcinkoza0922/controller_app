//! Force-quitting the focused game: ends its process and everything it started.

use std::{collections::HashMap, fs, thread, time::Duration};

/// How long a game gets to exit after SIGTERM before it is killed.
const GRACE: Duration = Duration::from_secs(3);

/// Programs that must never be force quit: the desktop itself, so a stray hold with no
/// window in front (or the shell focused) can't take the session down.
const PROTECTED: &[&str] = &[
    "plasmashell", "kwin_wayland", "kwin_wayland_wrapper", "kwin_x11", "krunner", "xorg", "xwayland", "gnome-shell",
    "mutter", "sway", "hyprland", "weston", "labwc", "river", "niri", "cosmic-comp", "wayfire", "systemd", "sddm",
    "gdm", "lightdm", "login", "pipewire", "wireplumber", "controller_app",
];

/// Whether a window of this executable is off limits to force quit.
pub fn protected(exe: &str) -> bool {
    let exe = exe.trim().to_ascii_lowercase();
    exe.is_empty() || PROTECTED.contains(&exe.as_str())
}

/// `pid` and all its descendants (children first, so a launcher can't respawn them), given
/// each process's parent.
fn tree(pid: u32, parents: &HashMap<u32, u32>) -> Vec<u32> {
    let mut found = vec![pid];
    let mut i = 0;
    while i < found.len() {
        let parent = found[i];
        let mut children: Vec<u32> = parents.iter().filter(|(_, p)| **p == parent).map(|(c, _)| *c).collect();
        children.sort_unstable();
        children.retain(|c| !found.contains(c));
        found.extend(children);
        i += 1;
    }
    found.reverse();
    found
}

/// Every process's parent, from /proc.
fn parents() -> HashMap<u32, u32> {
    let Ok(entries) = fs::read_dir("/proc") else { return HashMap::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            let stat = fs::read_to_string(e.path().join("stat")).ok()?;
            // "pid (comm) S ppid ...": the name can hold spaces and parentheses, so count
            // from its closing one.
            let rest = &stat[stat.rfind(')')? + 1..];
            let ppid: u32 = rest.split_whitespace().nth(1)?.parse().ok()?;
            Some((pid, ppid))
        })
        .collect()
}

fn signal(pids: &[u32], sig: libc::c_int) {
    for pid in pids {
        // SAFETY: `kill` only sends a signal.
        unsafe { libc::kill(*pid as libc::pid_t, sig) };
    }
}

fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Asks the game's process and its descendants to exit, and kills what is still there after a
/// few seconds. Returns at once; the kill follows on a thread. Does nothing for pid 0, 1 or
/// this program.
pub fn force_quit(pid: u32) {
    if pid <= 1 || pid == std::process::id() {
        return;
    }
    let pids = tree(pid, &parents());
    signal(&pids, libc::SIGTERM);
    thread::spawn(move || {
        thread::sleep(GRACE);
        let left: Vec<u32> = pids.into_iter().filter(|p| alive(*p)).collect();
        signal(&left, libc::SIGKILL);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_lists_descendants_children_first() {
        // 10 → 11 → 13, 10 → 12; 20 is unrelated.
        let parents = HashMap::from([(11, 10), (12, 10), (13, 11), (20, 1)]);
        assert_eq!(tree(10, &parents), [13, 12, 11, 10]);
        assert_eq!(tree(20, &parents), [20]);
        assert_eq!(tree(99, &parents), [99]);
    }

    #[test]
    fn force_quit_ends_a_process_and_its_children() {
        // A shell that starts a child and waits for it.
        let mut shell = std::process::Command::new("sh").args(["-c", "sleep 60 & wait"]).spawn().unwrap();
        thread::sleep(Duration::from_millis(200));
        force_quit(shell.id());
        let status = shell.wait().unwrap();
        assert!(!status.success());
    }

    #[test]
    fn the_desktop_is_protected() {
        for exe in ["plasmashell", "KWin_Wayland", "Hyprland", "", "controller_app"] {
            assert!(protected(exe), "{exe}");
        }
        assert!(!protected("eldenring.exe") && !protected("firefox"));
    }

    #[test]
    fn this_process_is_found_under_its_parent() {
        let me = std::process::id();
        let parent = parents().get(&me).copied();
        assert!(parent.is_some_and(|p| p > 0));
        assert!(alive(me));
    }
}

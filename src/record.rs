//! Screen recording with gpu-screen-recorder, saved to `~/Videos/Recordings/<game>/`.

use std::{
    ffi::OsString,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};

use crate::capture::{media_path, on_path, timestamp};

const PROGRAM: &str = "gpu-screen-recorder";

/// How long a fresh recorder has to stay up before it counts as started.
const SETTLE: Duration = Duration::from_millis(1200);

/// What to capture: the screen directly (needs no prompt), or through the desktop portal
/// (works anywhere on Wayland, asking once and remembering the choice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Screen,
    Portal,
}

fn args(source: Source, path: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-w", if source == Source::Screen { "screen" } else { "portal" }].map(Into::into).into();
    if source == Source::Portal {
        args.extend(["-restore-portal-session", "yes"].map(OsString::from));
    }
    // Desktop audio, 60 fps, high quality, finished with SIGINT.
    args.extend(["-f", "60", "-q", "high", "-a", "default_output", "-c", "mp4", "-o"].map(OsString::from));
    args.push(path.as_os_str().to_owned());
    args
}

fn spawn(source: Source, path: &Path) -> Result<Child> {
    let mut command = Command::new(PROGRAM);
    command.args(args(source, path)).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // SAFETY: only calls `prctl`, which is async-signal-safe. The recorder finishes its file
    // (SIGINT) if the daemon goes away.
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGINT);
            Ok(())
        });
    }
    command.spawn().with_context(|| format!("running {PROGRAM}"))
}

/// Starts recording for `game` and returns the recorder and its file. Waits a moment to see
/// that it stays up, so run it off the daemon's main thread. Tries the screen first and the
/// portal if that fails.
pub fn start(game: &str) -> Result<(Child, PathBuf)> {
    if !on_path(PROGRAM) {
        bail!("{PROGRAM} is not installed");
    }
    let videos = crate::capture::media_base(dirs::video_dir(), "Videos")?;
    let path = media_path(&videos, "Recordings", game, &timestamp(), "mp4");
    std::fs::create_dir_all(path.parent().context("no folder")?).with_context(|| format!("creating {}", path.display()))?;
    for source in [Source::Screen, Source::Portal] {
        let mut child = spawn(source, &path)?;
        thread::sleep(SETTLE);
        if child.try_wait()?.is_none() {
            return Ok((child, path));
        }
    }
    bail!("{PROGRAM} couldn't start. Is it set up for screen capture?")
}

/// Asks the recorder to finish its file and exit.
pub fn stop(pid: u32) {
    // SAFETY: `kill` only sends a signal.
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(args: &[OsString], a: &str) -> bool {
        args.iter().any(|x| x == a)
    }

    #[test]
    fn the_screen_needs_no_portal_options() {
        let a = args(Source::Screen, Path::new("/v/x.mp4"));
        assert!(has(&a, "screen") && !has(&a, "-restore-portal-session"));
        assert_eq!(a.last().unwrap(), "/v/x.mp4");
    }

    #[test]
    fn the_portal_remembers_its_choice() {
        let a = args(Source::Portal, Path::new("/v/x.mp4"));
        assert!(has(&a, "portal") && has(&a, "-restore-portal-session"));
    }
}

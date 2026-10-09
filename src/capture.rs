//! Screenshots: picks the screenshot tool for the desktop and saves to
//! `~/Pictures/Screenshots/<game>/`. Also the file naming recordings share.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

/// A command-line screenshot tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    /// KDE's. Also works on other desktops, through the screenshot portal.
    Spectacle,
    /// wlroots compositors (sway, Hyprland).
    Grim,
    GnomeScreenshot,
}

impl Tool {
    fn program(self) -> &'static str {
        match self {
            Tool::Spectacle => "spectacle",
            Tool::Grim => "grim",
            Tool::GnomeScreenshot => "gnome-screenshot",
        }
    }

    /// Arguments that save the whole screen to `path`, without any window or notification.
    fn args(self, path: &Path) -> Vec<std::ffi::OsString> {
        let path = path.as_os_str().to_owned();
        match self {
            Tool::Spectacle => ["-b", "-n", "-f", "-o"].iter().map(Into::into).chain([path]).collect(),
            Tool::Grim => vec![path],
            Tool::GnomeScreenshot => vec!["-f".into(), path],
        }
    }
}

/// The tool to use on `desktop` (`XDG_CURRENT_DESKTOP`), given which programs are installed:
/// the desktop's own first, then any other.
fn pick_tool(desktop: &str, installed: impl Fn(&str) -> bool) -> Option<Tool> {
    let desktop = desktop.to_ascii_lowercase();
    let preferred = if desktop.contains("kde") {
        Tool::Spectacle
    } else if desktop.contains("gnome") {
        Tool::GnomeScreenshot
    } else {
        Tool::Grim
    };
    [preferred, Tool::Spectacle, Tool::Grim, Tool::GnomeScreenshot]
        .into_iter()
        .find(|t| installed(t.program()))
}

pub fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// Where captures go instead of the user's Pictures and Videos, when set (debug mode).
static REDIRECT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Sends every screenshot and recording from now on to `dir`, in the same layout as the usual
/// folders. The daemon does this in debug mode, so test captures don't land among real ones.
pub fn redirect_to(dir: PathBuf) {
    let _ = REDIRECT.set(dir);
}

/// The folder captures of one kind go under: the redirect, else the user's own `dir` (their
/// Pictures or Videos), else `fallback` in their home.
pub fn media_base(dir: Option<PathBuf>, fallback: &str) -> Result<PathBuf> {
    if let Some(dir) = REDIRECT.get() {
        return Ok(dir.clone());
    }
    dir.or_else(|| dirs::home_dir().map(|h| h.join(fallback))).with_context(|| format!("no {fallback} folder"))
}

/// A game's name as a folder name: no path separators or leading dots.
fn folder_name(game: &str) -> String {
    let name: String = game.chars().map(|c| if c == '/' || c == '\\' || c.is_control() { '_' } else { c }).collect();
    let name = name.trim().trim_start_matches('.');
    if name.is_empty() { "Screenshots".into() } else { name.into() }
}

/// `<base>/<kind>/<game>/<stamp>.<ext>`, e.g. `~/Pictures/Screenshots/Doom/<time>.png`.
pub fn media_path(base: &Path, kind: &str, game: &str, stamp: &str, ext: &str) -> PathBuf {
    base.join(kind).join(folder_name(game)).join(format!("{stamp}.{ext}"))
}

/// The local time as `2026-10-07_19-58-03`.
pub fn timestamp() -> String {
    // SAFETY: `localtime_r` and `strftime` only write into the buffers passed to them.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        let mut buf = [0 as libc::c_char; 32];
        let len = libc::strftime(buf.as_mut_ptr(), buf.len(), c"%Y-%m-%d_%H-%M-%S".as_ptr(), &tm);
        buf[..len].iter().map(|&c| c as u8 as char).collect()
    }
}

/// Saves a screenshot for `game` and returns where it went. Waits for the tool, so run it off
/// the daemon's main thread.
pub fn screenshot(game: &str) -> Result<PathBuf> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let tool = pick_tool(&desktop, on_path).context("no screenshot tool found (install spectacle, grim or gnome-screenshot)")?;
    let pictures = media_base(dirs::picture_dir(), "Pictures")?;
    let path = media_path(&pictures, "Screenshots", game, &timestamp(), "png");
    std::fs::create_dir_all(path.parent().context("no folder")?).with_context(|| format!("creating {}", path.display()))?;
    let status = Command::new(tool.program()).args(tool.args(&path)).status().with_context(|| format!("running {}", tool.program()))?;
    if !status.success() || !path.exists() {
        bail!("{} did not save a screenshot ({status})", tool.program());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktops_own_tool_comes_first() {
        let all = |_: &str| true;
        assert_eq!(pick_tool("KDE", all), Some(Tool::Spectacle));
        assert_eq!(pick_tool("GNOME", all), Some(Tool::GnomeScreenshot));
        assert_eq!(pick_tool("sway", all), Some(Tool::Grim));
        assert_eq!(pick_tool("KDE", |p| p == "grim"), Some(Tool::Grim), "falls back to whatever is installed");
        assert_eq!(pick_tool("KDE", |_| false), None);
    }

    #[test]
    fn files_go_in_a_folder_named_after_the_game() {
        let pictures = Path::new("/home/me/Pictures");
        assert_eq!(
            media_path(pictures, "Screenshots", "Max Payne 2", "2026-10-07_19-58-03", "png"),
            Path::new("/home/me/Pictures/Screenshots/Max Payne 2/2026-10-07_19-58-03.png")
        );
        assert_eq!(folder_name("../etc/passwd"), "_etc_passwd");
        assert_eq!(folder_name("  "), "Screenshots");
    }

    #[test]
    fn timestamps_look_like_dates() {
        let t = timestamp();
        assert_eq!(t.len(), 19, "{t}");
        assert!(t.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '_'), "{t}");
    }

    #[test]
    fn spectacle_runs_in_the_background() {
        let args = Tool::Spectacle.args(Path::new("/x.png"));
        assert_eq!(args.last().unwrap(), "/x.png");
        assert!(args.iter().any(|a| a == "-b"));
    }
}

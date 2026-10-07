mod config;
mod daemon;
mod engine;
mod focus;
mod font;
mod gui;
mod info;
mod inputlog;
mod input;
mod ipc;
mod keyboard;
mod launchers;
mod library;
mod menu;
mod monitor;
mod output;
mod overlay;
mod pack;
mod remap;
mod pad_svg;
mod rumble;
mod style;

use anyhow::{Result, bail};

use ipc::{Request, Response};

const USAGE: &str = "\
usage: controller_app [command]

commands:
  gui              open the settings window (default)
  daemon           run the background remapping service
  status           show daemon status and detected controllers
  enable|disable   turn remapping on or off
  profile <name>   switch to a profile (in the current game, General, or any game)
  next-profile     switch to the next profile
  reload           re-read the config file
  overlay-toggle   open or close the on-screen keyboard overlay
  numpad-toggle    open or close the on-screen numpad overlay
  menu <name>      show an on-screen menu";

#[expect(clippy::print_stdout, reason = "CLI output")]
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] | ["gui"] => gui::run().map_err(Into::into),
        ["daemon"] => daemon::run(),
        ["status"] => status(),
        ["enable"] => send(&Request::SetEnabled(true)),
        ["disable"] => send(&Request::SetEnabled(false)),
        ["profile", name] => send(&Request::SetProfile(name.to_string())),
        ["next-profile"] => send(&Request::NextProfile),
        ["reload"] => send(&Request::Reload),
        ["overlay"] => overlay::run(),
        ["overlay-toggle"] => send(&Request::ToggleOverlay),
        ["numpad-toggle"] => send(&Request::ToggleNumpad),
        ["menu", name] => send(&Request::OpenMenu(name.to_string())),
        ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}

fn send(req: &Request) -> Result<()> {
    ipc::request(req)?;
    Ok(())
}

#[expect(clippy::print_stdout, reason = "CLI output")]
fn status() -> Result<()> {
    let Response::Status(s) = ipc::request(&Request::Status)? else {
        bail!("unexpected reply from daemon");
    };
    println!("enabled: {}", s.enabled);
    match &s.active_game {
        Some(game) => println!("profile: {} ({game})", s.active_profile),
        None => println!("profile: {}", s.active_profile),
    }
    if let Some(at) = &s.active_keyboard {
        println!("keyboard profile: {at}");
    }
    let tracking = match s.focus_backend {
        ipc::FocusBackend::Kwin => "focused window (KWin)",
        ipc::FocusBackend::ProcessScan => "running processes",
    };
    println!("per-game switching follows: {tracking}");
    if let Some(w) = &s.focused {
        let steam = w.steam_app_id.as_deref().map(|id| format!(", Steam {id}")).unwrap_or_default();
        println!("focused: {} (class {}{steam})", w.exe, w.class);
    }
    for d in s.devices {
        let state = if d.managed { "remapping" } else if d.ignored { "ignored" } else { "idle" };
        let triggers = if d.analog_triggers { "" } else { ", digital triggers" };
        let rumble = if d.rumble { "" } else { ", no rumble" };
        println!("  {} [{}] {state}{triggers}{rumble}", d.name, d.path);
    }
    Ok(())
}

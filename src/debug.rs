//! Debug mode, reached only from the command line (`padwight debug`). It runs a virtual
//! controller that can be any supported model: clicked on in a window, or driven by commands
//! from another terminal or a test script. Nothing here touches real controllers or the daemon.

use std::{
    io::Read,
    time::Duration,
};

use anyhow::{Context, Result, bail};

mod bridge;
mod pad;
mod protocol;
mod server;
mod window;

use pad::VirtualPad;
use protocol::{Command, DebugResponse};
use server::{Client, Session};
use crate::{
    info::{Glyphs, PadModel},
    ipc,
    monitor::log,
    pad_svg,
};

const USAGE: &str = "\
usage: padwight debug [--headless] [--attach [--live]] [--model <name>]
       padwight debug [--wait <ms>] <command>...

With no command, starts a debug session: a virtual controller that stands in for any
supported model, in a window of its own that shows every model's drawing, live. Click the
virtual controller to press its buttons and drag its sticks and triggers. With --headless
there is no window, only the command socket, for scripts and tests.

With --attach, the daemon runs the virtual controller as a controller of its own, so the
active profile's mappings, layers, menus and overlays react to it, and `output` shows what the
mappings did. The daemon must have been started with `padwight daemon --debug`. The output goes
nowhere unless --live is given, which also sends it to the real virtual pad, keyboard and
mouse: keys really are typed. What a mapping does besides output (screenshots, recording, force
quit, the on-screen keyboard) happens either way, except that a `--debug` daemon saves
screenshots and recordings in a private folder in the temp directory (it logs where), not in
Pictures and Videos. Disabling remapping removes the controller.

A running session takes commands from other terminals. --wait retries connecting for that
long, for a session that is still starting. The socket is $XDG_RUNTIME_DIR/padwight-debug.sock,
or $PADWIGHT_DEBUG_SOCKET (in a directory of yours), so sessions can run side by side.

Without a session:
  models                         list the model names
  svg [model]                    print a model's drawing at rest, as SVG
  detach-all                     remove controllers a session left in the daemon

With a session:
";

fn usage() -> String {
    format!("{USAGE}{}", protocol::COMMANDS)
}

#[expect(clippy::print_stdout, reason = "CLI output")]
pub fn run(args: &[&str]) -> Result<()> {
    // --wait only matters to commands sent to a session, but any command may carry it.
    let (wait, args) = match args {
        ["--wait", ms, rest @ ..] => (Duration::from_millis(ms.parse().context("--wait takes milliseconds")?), rest),
        _ => (Duration::ZERO, args),
    };
    match args {
        ["help" | "-h" | "--help"] => println!("{}", usage()),
        ["models"] => {
            println!("generic");
            PadModel::ALL.iter().for_each(|m| println!("{}", m.slug()));
        }
        ["svg"] => println!("{}", rest_svg(None)),
        ["svg", name] => println!("{}", rest_svg(parse_model(name)?)),
        ["detach-all"] => ipc::request(&ipc::Request::DebugDetach("all".into())).map(drop)?,
        [] => start(&[])?,
        [first, ..] if first.starts_with("--") => start(args)?,
        _ => command(wait, args)?,
    }
    Ok(())
}

fn parse_model(name: &str) -> Result<Option<PadModel>> {
    PadModel::parse(name).with_context(|| format!("unknown model {name} (see `padwight debug models`)"))
}

fn rest_svg(model: Option<PadModel>) -> String {
    let glyphs = Glyphs { family: model.map_or_else(Default::default, PadModel::family), nintendo_layout: false };
    pad_svg::render(None, model, glyphs, &[])
}

/// Runs a session until it is closed or told to quit.
fn start(args: &[&str]) -> Result<()> {
    let (mut headless, mut attach, mut live) = (false, false, false);
    let mut model = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match *arg {
            "--headless" => headless = true,
            "--attach" => attach = true,
            "--live" => live = true,
            "--model" => model = parse_model(it.next().context("--model needs a name")?)?,
            _ => bail!("{}", usage()),
        }
    }
    if live && !attach {
        bail!("--live needs --attach");
    }
    let path = server::socket_path();
    let session = Session::new(VirtualPad::of(model));
    if attach {
        let device = session.attach(live)?;
        log!("attached to the daemon as {device}{}", if live { " (live output)" } else { "" });
    }
    server::serve(&session, &path)?;
    log!("debug session listening on {}", path.display());
    let result = if headless {
        while !session.quitting() {
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    } else {
        window::run(session.clone(), path.display().to_string())
    };
    session.detach();
    let _ = std::fs::remove_file(&path);
    result.map_err(Into::into)
}

/// Sends commands to a running session.
fn command(wait: Duration, words: &[&str]) -> Result<()> {
    match words {
        ["run"] => script(wait, &mut std::io::stdin().lock()),
        ["run", file] => script(wait, &mut std::fs::File::open(file).with_context(|| format!("cannot read {file}"))?),
        _ => {
            let Command::Request(req) = protocol::parse(words)? else { bail!("sleep only makes sense in a script") };
            let mut client = Client::connect(&server::socket_path(), wait)?;
            show(client.send(&req)?)
        }
    }
}

/// Runs the commands of a script, one per line. The whole script is read first, so a typo
/// stops it before anything is sent.
fn script(wait: Duration, source: &mut impl Read) -> Result<()> {
    let mut text = String::new();
    source.read_to_string(&mut text)?;
    let mut steps = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default();
        let words: Vec<&str> = line.split_whitespace().collect();
        if !words.is_empty() {
            steps.push((n + 1, protocol::parse(&words).with_context(|| format!("line {}", n + 1))?));
        }
    }
    let mut client = Client::connect(&server::socket_path(), wait)?;
    for (n, step) in steps {
        match step {
            Command::Sleep(ms) => std::thread::sleep(Duration::from_millis(ms)),
            Command::Request(req) => show(client.send(&req)?).with_context(|| format!("line {n}"))?,
        }
    }
    Ok(())
}

/// Prints a reply: state as one line of JSON, text as it is. An error becomes the exit status.
#[expect(clippy::print_stdout, reason = "CLI output")]
fn show(reply: DebugResponse) -> Result<()> {
    match reply {
        DebugResponse::Ok => {}
        DebugResponse::State(s) => println!("{}", serde_json::to_string(&s)?),
        DebugResponse::Text(t) => println!("{t}"),
        DebugResponse::Error(e) => bail!(e),
    }
    Ok(())
}

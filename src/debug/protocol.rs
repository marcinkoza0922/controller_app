//! The debug session's wire protocol (one JSON line each way, like `ipc`) and the command words
//! that become requests, shared by the one-shot command line and `run` scripts.

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::{
    config::{Button, Stick, Trigger},
    info::{PadFamily, PadModel},
    ipc::InputSnapshot,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DebugRequest {
    /// Succeeds when the session is up.
    Ping,
    /// The virtual controller's input as a snapshot.
    State,
    /// The virtual controller's drawing, as SVG.
    Render,
    /// Let go of everything and center the sticks.
    Reset,
    /// End the session.
    Quit,
    /// Become another model; `None` is the generic controller.
    Model(Option<PadModel>),
    Family(PadFamily),
    Press(Vec<Button>),
    Release(Vec<Button>),
    /// Press, hold for `ms`, release.
    Tap { buttons: Vec<Button>, ms: u64 },
    Stick { stick: Stick, x: f32, y: f32 },
    Trigger { trigger: Trigger, value: f32 },
    /// Motion rates in degrees per second. While attached to the daemon the pad turns at them
    /// for `ms` (100 when 0), then stops; otherwise they are just set.
    Gyro { rates: [f32; 3], ms: u64 },
    /// What the daemon's mappings have output for this controller, one event per line; needs
    /// `--attach`. `clear` forgets what was returned.
    Output { clear: bool },
    /// Fails unless the virtual controller is as described.
    Expect(Expectation),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expectation {
    Model(Option<PadModel>),
    Pressed(Vec<Button>),
    Released(Vec<Button>),
    /// Nothing held and everything centered.
    Idle,
    Stick { stick: Stick, x: f32, y: f32 },
    Trigger { trigger: Trigger, value: f32 },
    /// The mappings have output this line (see `output`) since it was last cleared.
    Output(String),
    /// They have output nothing since then.
    NoOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DebugResponse {
    Ok,
    State(InputSnapshot),
    Text(String),
    Error(String),
}

/// One line of a command or script.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Request(DebugRequest),
    /// Pause a script, in milliseconds.
    Sleep(u64),
}

pub const COMMANDS: &str = "\
  ping                           succeeds when a session is running
  state                          print the virtual controller's input as JSON
  render                         print its drawing as SVG
  reset                          release everything and center the sticks
  quit                           end the session
  model <name|generic>           become another controller model (see `debug models`)
  family <xbox|playstation|nintendo>   draw another family's button letters
  press <button>...              hold buttons down
  release <button>...            let buttons go
  tap <button>... [ms]           press, wait (default 50 ms), release
  stick <left|right> <x> <y>     push a stick, each axis -1..1 (y positive is down)
  trigger <left|right> <0..1>    pull a trigger
  gyro <pitch> <yaw> <roll> [ms] set the motion rates, in degrees per second; attached, the
                                 pad turns that way for ms (default 100), then stops
  output [clear]                 attached: print what the daemon's mappings have output
                                 (key KEY_A down, pad South up, mouse-move 3 -2, ...)
  expect model <name>            exit with an error unless the pad is that model
  expect pressed <button>...     ... unless these buttons are down
  expect released <button>...    ... unless these buttons are up
  expect idle                    ... unless nothing is held and the sticks are centered
  expect stick <left|right> <x> <y>   ... unless the stick is there (within 0.01)
  expect trigger <left|right> <0..1>  ... unless the trigger is pulled that far
  expect output <line>           attached: ... unless the mappings have output that line
  expect no-output               attached: ... if they have output anything
  run [file]                     run commands, one per line, from a file or stdin;
                                 blank lines and # comments are skipped; `sleep <ms>` waits

buttons: south east north west (or a b y x), lb rb, select start guide, ls rs (stick clicks),
         up down left right (the D-pad), and paddles: lpaddle rpaddle lpaddle2 rpaddle2";

/// Parses one command line's words.
pub fn parse(words: &[&str]) -> Result<Command> {
    let request = match words {
        ["ping"] => DebugRequest::Ping,
        ["state"] => DebugRequest::State,
        ["render"] => DebugRequest::Render,
        ["reset"] => DebugRequest::Reset,
        ["quit"] => DebugRequest::Quit,
        ["sleep", ms] => return Ok(Command::Sleep(ms.parse().map_err(|_| anyhow!("not a number of milliseconds: {ms}"))?)),
        ["model", name] => DebugRequest::Model(model(name)?),
        ["family", name] => DebugRequest::Family(family(name)?),
        ["press", names @ ..] => DebugRequest::Press(buttons(names)?),
        ["release", names @ ..] => DebugRequest::Release(buttons(names)?),
        ["tap", names @ ..] => {
            // A trailing number is how long to hold.
            let (names, ms) = match names.split_last() {
                Some((last, rest)) if last.parse::<u64>().is_ok() => (rest, last.parse().unwrap_or(50)),
                _ => (names, 50),
            };
            DebugRequest::Tap { buttons: buttons(names)?, ms }
        }
        ["stick", side, x, y] => DebugRequest::Stick { stick: stick(side)?, x: number(x)?, y: number(y)? },
        ["trigger", side, value] => DebugRequest::Trigger { trigger: trigger(side)?, value: number(value)? },
        ["gyro", p, y, r] => DebugRequest::Gyro { rates: [number(p)?, number(y)?, number(r)?], ms: 0 },
        ["gyro", p, y, r, ms] => DebugRequest::Gyro {
            rates: [number(p)?, number(y)?, number(r)?],
            ms: ms.parse().map_err(|_| anyhow!("not a number of milliseconds: {ms}"))?,
        },
        ["output"] => DebugRequest::Output { clear: false },
        ["output", "clear"] => DebugRequest::Output { clear: true },
        ["expect", rest @ ..] => DebugRequest::Expect(expectation(rest)?),
        [] => bail!("no command"),
        [other, ..] => bail!("unknown debug command: {other}"),
    };
    Ok(Command::Request(request))
}

fn expectation(words: &[&str]) -> Result<Expectation> {
    Ok(match words {
        ["model", name] => Expectation::Model(model(name)?),
        ["pressed", names @ ..] => Expectation::Pressed(buttons(names)?),
        ["released", names @ ..] => Expectation::Released(buttons(names)?),
        ["idle"] => Expectation::Idle,
        ["output", line @ ..] if !line.is_empty() => Expectation::Output(line.join(" ")),
        ["no-output"] => Expectation::NoOutput,
        ["stick", side, x, y] => Expectation::Stick { stick: stick(side)?, x: number(x)?, y: number(y)? },
        ["trigger", side, value] => Expectation::Trigger { trigger: trigger(side)?, value: number(value)? },
        _ => bail!("expect what? model, pressed, released, idle, stick, trigger, output or no-output"),
    })
}

fn number(s: &str) -> Result<f32> {
    s.parse().map_err(|_| anyhow!("not a number: {s}"))
}

fn model(name: &str) -> Result<Option<PadModel>> {
    PadModel::parse(name).ok_or_else(|| {
        let all: Vec<&str> = PadModel::ALL.iter().map(|m| m.slug()).collect();
        anyhow!("unknown model {name}; try generic, {}", all.join(", "))
    })
}

fn family(name: &str) -> Result<PadFamily> {
    match name.to_lowercase().as_str() {
        "xbox" => Ok(PadFamily::Xbox),
        "playstation" | "ps" => Ok(PadFamily::PlayStation),
        "nintendo" => Ok(PadFamily::Nintendo),
        _ => bail!("unknown family {name}; try xbox, playstation or nintendo"),
    }
}

fn stick(side: &str) -> Result<Stick> {
    match side.to_lowercase().as_str() {
        "left" | "l" => Ok(Stick::Left),
        "right" | "r" => Ok(Stick::Right),
        _ => bail!("which stick? left or right, not {side}"),
    }
}

fn trigger(side: &str) -> Result<Trigger> {
    match side.to_lowercase().as_str() {
        "left" | "l" | "lt" | "l2" => Ok(Trigger::Left),
        "right" | "r" | "rt" | "r2" => Ok(Trigger::Right),
        _ => bail!("which trigger? left or right, not {side}"),
    }
}

fn buttons(names: &[&str]) -> Result<Vec<Button>> {
    if names.is_empty() {
        bail!("which button?");
    }
    names.iter().map(|n| button(n)).collect()
}

/// A physical button by any of its common names.
pub fn button(name: &str) -> Result<Button> {
    let norm = name.to_lowercase().replace(['-', '_'], "");
    Ok(match norm.as_str() {
        "south" | "a" | "cross" => Button::South,
        "east" | "b" | "circle" => Button::East,
        "north" | "y" | "triangle" => Button::North,
        "west" | "x" | "square" => Button::West,
        "lb" | "l1" | "leftbumper" => Button::LeftBumper,
        "rb" | "r1" | "rightbumper" => Button::RightBumper,
        "select" | "back" | "share" | "minus" => Button::Select,
        "start" | "menu" | "options" | "plus" => Button::Start,
        "guide" | "home" | "ps" => Button::Guide,
        "ls" | "l3" | "leftstick" => Button::LeftStick,
        "rs" | "r3" | "rightstick" => Button::RightStick,
        "up" | "dpadup" => Button::DpadUp,
        "down" | "dpaddown" => Button::DpadDown,
        "left" | "dpadleft" => Button::DpadLeft,
        "right" | "dpadright" => Button::DpadRight,
        "lpaddle" | "leftpaddle" => Button::LeftPaddle,
        "rpaddle" | "rightpaddle" => Button::RightPaddle,
        "lpaddle2" | "leftpaddle2" => Button::LeftPaddle2,
        "rpaddle2" | "rightpaddle2" => Button::RightPaddle2,
        _ => bail!("unknown button {name}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(line: &str) -> DebugRequest {
        let words: Vec<&str> = line.split_whitespace().collect();
        match parse(&words).unwrap() {
            Command::Request(r) => r,
            Command::Sleep(_) => panic!("not a request"),
        }
    }

    #[test]
    fn commands_parse_into_requests() {
        assert_eq!(req("press a LB"), DebugRequest::Press(vec![Button::South, Button::LeftBumper]));
        assert_eq!(req("tap start 200"), DebugRequest::Tap { buttons: vec![Button::Start], ms: 200 });
        assert_eq!(req("tap dpad-up"), DebugRequest::Tap { buttons: vec![Button::DpadUp], ms: 50 });
        assert_eq!(req("stick left 0.5 -1"), DebugRequest::Stick { stick: Stick::Left, x: 0.5, y: -1.0 });
        assert_eq!(req("trigger rt 0.25"), DebugRequest::Trigger { trigger: Trigger::Right, value: 0.25 });
        assert_eq!(req("model DualSense-Edge"), DebugRequest::Model(Some(PadModel::DualSenseEdge)));
        assert_eq!(req("model generic"), DebugRequest::Model(None));
        assert_eq!(req("expect idle"), DebugRequest::Expect(Expectation::Idle));
        assert_eq!(req("expect output key KEY_A down"), DebugRequest::Expect(Expectation::Output("key KEY_A down".into())));
        assert_eq!(req("expect no-output"), DebugRequest::Expect(Expectation::NoOutput));
        assert_eq!(req("output clear"), DebugRequest::Output { clear: true });
        assert_eq!(req("gyro 1 2 3 250"), DebugRequest::Gyro { rates: [1.0, 2.0, 3.0], ms: 250 });
        assert_eq!(parse(&["sleep", "20"]).unwrap(), Command::Sleep(20));
    }

    #[test]
    fn mistakes_are_explained() {
        let err = |line: &str| parse(&line.split_whitespace().collect::<Vec<_>>()).unwrap_err().to_string();
        assert!(err("press").contains("which button"));
        assert!(err("press banana").contains("unknown button"));
        assert!(err("model nope").contains("xbox360"));
        assert!(err("stick middle 0 0").contains("left or right"));
        assert!(err("jump").contains("unknown debug command"));
    }
}

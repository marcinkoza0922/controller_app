//! Media playback controls: an overlay at the top of the screen that drives whatever MPRIS
//! player is running (Spotify, YouTube Music in a browser, a local player). A worker thread
//! talks to the session bus so a stuck player can't stall the daemon; `MediaSession` turns
//! controller input into commands for it, and the overlay process draws `MediaView`.

use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, RecvTimeoutError, Sender},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use zbus::{blocking::Connection, zvariant::{OwnedValue, Value}};

use crate::{
    config::{Button, OverlayStyle, ScreenPosition},
    input::InputEvent,
    monitor::log,
};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// How often the worker re-reads the player (the progress bar moves with it).
const POLL: Duration = Duration::from_millis(500);
/// The overlay closes by itself after this long without input, so it can't hold the
/// controller forever.
const IDLE_CLOSE: Duration = Duration::from_secs(10);
const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_EVERY: Duration = Duration::from_millis(120);
const VOLUME_STEP: f64 = 0.05;
const SEEK_STEP_US: i64 = 10_000_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayState {
    Playing,
    Paused,
    #[default]
    Stopped,
}

/// What the worker last read from the selected player.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaState {
    /// The player's display name; `None` when no player is running.
    pub player: Option<String>,
    pub title: String,
    pub artist: String,
    pub state: PlayState,
    pub position_us: i64,
    pub length_us: i64,
    pub volume: Option<f64>,
    /// How many players are running, to offer switching between them.
    pub players: usize,
}

/// What the overlay draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaView {
    pub player: Option<String>,
    pub title: String,
    pub artist: String,
    pub state: PlayState,
    pub position_ms: i64,
    pub length_ms: i64,
    /// 0..1.
    pub volume: Option<f32>,
    pub players: usize,
    pub hint: String,
    pub style: OverlayStyle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    PlayPause,
    Next,
    Previous,
    /// Microseconds, negative to go back.
    Seek(i64),
    /// Added to the volume (0..1 scale).
    Volume(f64),
    CyclePlayer,
}

/// Turns controller input into player commands while the media overlay is up.
pub struct MediaSession {
    commands: Sender<Command>,
    state: MediaState,
    guide_held: bool,
    last_input: Instant,
    /// A held button that repeats, and when it next fires.
    repeat: Option<(Button, Instant)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaOutcome {
    Close,
}

fn repeating(b: Button) -> Option<Command> {
    Some(match b {
        Button::DpadUp => Command::Volume(VOLUME_STEP),
        Button::DpadDown => Command::Volume(-VOLUME_STEP),
        Button::DpadLeft => Command::Seek(-SEEK_STEP_US),
        Button::DpadRight => Command::Seek(SEEK_STEP_US),
        _ => return None,
    })
}

impl MediaSession {
    pub fn new(commands: Sender<Command>, guide_held: bool, now: Instant) -> Self {
        MediaSession { commands, state: MediaState::default(), guide_held, last_input: now, repeat: None }
    }

    pub fn update(&mut self, state: MediaState) {
        self.state = state;
    }

    pub fn view(&self) -> MediaView {
        let s = &self.state;
        MediaView {
            player: s.player.clone(),
            title: s.title.clone(),
            artist: s.artist.clone(),
            state: s.state,
            position_ms: s.position_us / 1000,
            length_ms: s.length_us / 1000,
            volume: s.volume.map(|v| v as f32),
            players: s.players,
            hint: "D-pad ◀ ▶ seek   ▲ ▼ volume   LB RB track   A play / pause   Y player   B close".into(),
            style: OverlayStyle { position: ScreenPosition::TopCenter, ..OverlayStyle::default() },
        }
    }

    pub fn handle(&mut self, ev: InputEvent, now: Instant) -> Option<MediaOutcome> {
        let InputEvent::Button(b, pressed) = ev else { return None };
        if b == Button::Guide {
            self.guide_held = pressed;
            return None;
        }
        if !pressed {
            if self.repeat.is_some_and(|(held, _)| held == b) {
                self.repeat = None;
            }
            return None;
        }
        self.last_input = now;
        let command = match b {
            // Guide + LB opened it, so Guide + LB again puts it away.
            Button::LeftBumper if self.guide_held => return Some(MediaOutcome::Close),
            Button::East => return Some(MediaOutcome::Close),
            Button::South => Command::PlayPause,
            Button::RightBumper => Command::Next,
            Button::LeftBumper => Command::Previous,
            Button::North => Command::CyclePlayer,
            other => {
                let command = repeating(other)?;
                self.repeat = Some((other, now + REPEAT_DELAY));
                command
            }
        };
        let _ = self.commands.send(command);
        None
    }

    /// Fires a held button's repeat.
    pub fn tick(&mut self, now: Instant) {
        let Some((b, due)) = self.repeat else { return };
        if due > now {
            return;
        }
        if let Some(command) = repeating(b) {
            let _ = self.commands.send(command);
        }
        self.last_input = now;
        self.repeat = Some((b, now + REPEAT_EVERY));
    }

    pub fn expired(&self, now: Instant) -> bool {
        self.last_input + IDLE_CLOSE <= now
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        let idle = self.last_input + IDLE_CLOSE;
        Some(self.repeat.map_or(idle, |(_, due)| due.min(idle)))
    }
}

/// Reads and drives the players until `commands` is dropped. `publish` gets each fresh state
/// and says whether anyone is still listening.
pub fn worker(commands: &Receiver<Command>, publish: impl Fn(MediaState) -> bool) {
    let conn = match Connection::session() {
        Ok(c) => c,
        Err(e) => {
            log!("media: cannot reach the session bus: {e}");
            return;
        }
    };
    let mut chosen: Option<String> = None;
    loop {
        let names = players(&conn).unwrap_or_default();
        match commands.recv_timeout(POLL) {
            Ok(command) => {
                if command == Command::CyclePlayer {
                    chosen = next_player(&names, chosen.as_deref());
                } else if let Some(name) = chosen.as_deref().or(names.first().map(String::as_str))
                    && let Err(e) = send(&conn, name, command)
                {
                    log!("media: {name}: {e:#}");
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let names = players(&conn).unwrap_or_default();
        let state = read_all(&conn, &names, &mut chosen);
        if !publish(state) {
            return;
        }
    }
}

fn next_player(names: &[String], current: Option<&str>) -> Option<String> {
    let at = current.and_then(|c| names.iter().position(|n| n == c));
    names.get(at.map_or(0, |i| (i + 1) % names.len().max(1))).cloned()
}

fn players(conn: &Connection) -> Result<Vec<String>> {
    let reply = conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "ListNames", &())?;
    let mut names: Vec<String> = reply.body().deserialize::<Vec<String>>()?.into_iter().filter(|n| n.starts_with(PREFIX)).collect();
    names.sort();
    Ok(names)
}

fn all_properties(conn: &Connection, name: &str, iface: &str) -> Result<HashMap<String, OwnedValue>> {
    let reply = conn.call_method(Some(name), PATH, Some(PROPERTIES), "GetAll", &(iface,))?;
    Ok(reply.body().deserialize()?)
}

fn send(conn: &Connection, name: &str, command: Command) -> Result<()> {
    let call = |method: &str| conn.call_method(Some(name), PATH, Some(PLAYER), method, &()).map(drop);
    match command {
        Command::PlayPause => call("PlayPause")?,
        Command::Next => call("Next")?,
        Command::Previous => call("Previous")?,
        Command::Seek(us) => {
            conn.call_method(Some(name), PATH, Some(PLAYER), "Seek", &(us,))?;
        }
        Command::Volume(delta) => {
            let props = all_properties(conn, name, PLAYER)?;
            let current = props.get("Volume").and_then(|v| v.downcast_ref::<f64>().ok()).context("this player has no volume")?;
            let volume = (current + delta).clamp(0.0, 1.0);
            conn.call_method(Some(name), PATH, Some(PROPERTIES), "Set", &(PLAYER, "Volume", Value::from(volume)))?;
        }
        Command::CyclePlayer => {}
    }
    Ok(())
}

/// Picks the player to show (keeping `chosen` unless another one is the only one playing) and
/// reads it.
fn read_all(conn: &Connection, names: &[String], chosen: &mut Option<String>) -> MediaState {
    let mut playing = None;
    let mut read: HashMap<&str, HashMap<String, OwnedValue>> = HashMap::new();
    for name in names {
        if let Ok(props) = all_properties(conn, name, PLAYER) {
            if playing.is_none() && status(&props) == PlayState::Playing {
                playing = Some(name);
            }
            read.insert(name, props);
        }
    }
    let keep = chosen.as_ref().filter(|c| read.contains_key(c.as_str()));
    let keep_is_playing = keep.is_some_and(|c| status(&read[c.as_str()]) == PlayState::Playing);
    *chosen = if keep_is_playing { keep.cloned() } else { playing.cloned().or_else(|| keep.cloned()).or_else(|| names.first().cloned()) };
    let Some(name) = chosen.as_deref().filter(|c| read.contains_key(c)) else {
        return MediaState { players: names.len(), ..MediaState::default() };
    };
    let props = &read[name];
    let metadata = props.get("Metadata").and_then(|m| HashMap::<String, OwnedValue>::try_from(m.try_clone().ok()?).ok()).unwrap_or_default();
    let text = |key: &str| metadata.get(key).and_then(|v| v.downcast_ref::<String>().ok()).unwrap_or_default();
    let artist = metadata
        .get("xesam:artist")
        .and_then(|v| Vec::<String>::try_from(v.try_clone().ok()?).ok())
        .map(|a| a.join(", "))
        .unwrap_or_default();
    let identity = all_properties(conn, name, ROOT).ok().and_then(|r| r.get("Identity").and_then(|v| v.downcast_ref::<String>().ok()));
    MediaState {
        player: Some(identity.unwrap_or_else(|| name.trim_start_matches(PREFIX).to_string())),
        title: text("xesam:title"),
        artist,
        state: status(props),
        position_us: props.get("Position").and_then(integer).unwrap_or(0),
        length_us: metadata.get("mpris:length").and_then(integer).unwrap_or(0),
        volume: props.get("Volume").and_then(|v| v.downcast_ref::<f64>().ok()),
        players: names.len(),
    }
}

fn status(props: &HashMap<String, OwnedValue>) -> PlayState {
    match props.get("PlaybackStatus").and_then(|v| v.downcast_ref::<String>().ok()).as_deref() {
        Some("Playing") => PlayState::Playing,
        Some("Paused") => PlayState::Paused,
        _ => PlayState::Stopped,
    }
}

/// Players disagree on the integer type of positions and lengths.
fn integer(v: &OwnedValue) -> Option<i64> {
    v.downcast_ref::<i64>()
        .ok()
        .or_else(|| v.downcast_ref::<u64>().ok().and_then(|n| i64::try_from(n).ok()))
        .or_else(|| v.downcast_ref::<i32>().ok().map(i64::from))
        .or_else(|| v.downcast_ref::<u32>().ok().map(i64::from))
}

/// `1:05` or `1:02:03`.
pub fn clock(ms: i64) -> String {
    let secs = (ms / 1000).max(0);
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn session(guide: bool) -> (MediaSession, mpsc::Receiver<Command>, Instant) {
        let (tx, rx) = mpsc::channel();
        let now = Instant::now();
        (MediaSession::new(tx, guide, now), rx, now)
    }

    fn press(s: &mut MediaSession, b: Button, now: Instant) -> Option<MediaOutcome> {
        s.handle(InputEvent::Button(b, true), now)
    }

    #[test]
    fn buttons_send_commands() {
        let (mut s, rx, now) = session(false);
        for (b, c) in [
            (Button::South, Command::PlayPause),
            (Button::RightBumper, Command::Next),
            (Button::LeftBumper, Command::Previous),
            (Button::DpadUp, Command::Volume(VOLUME_STEP)),
            (Button::DpadRight, Command::Seek(SEEK_STEP_US)),
            (Button::North, Command::CyclePlayer),
        ] {
            assert_eq!(press(&mut s, b, now), None);
            assert_eq!(rx.try_recv(), Ok(c));
        }
        assert_eq!(press(&mut s, Button::East, now), Some(MediaOutcome::Close));
    }

    #[test]
    fn guide_and_left_bumper_close_but_left_bumper_alone_goes_back_a_track() {
        let (mut s, rx, now) = session(true);
        assert_eq!(press(&mut s, Button::LeftBumper, now), Some(MediaOutcome::Close));
        s.handle(InputEvent::Button(Button::Guide, false), now);
        assert_eq!(press(&mut s, Button::LeftBumper, now), None);
        assert_eq!(rx.try_recv(), Ok(Command::Previous));
    }

    #[test]
    fn held_buttons_repeat_until_let_go() {
        let (mut s, rx, now) = session(false);
        press(&mut s, Button::DpadDown, now);
        assert_eq!(rx.try_recv(), Ok(Command::Volume(-VOLUME_STEP)));
        s.tick(now + Duration::from_millis(100));
        assert!(rx.try_recv().is_err(), "not before the delay");
        s.tick(now + REPEAT_DELAY);
        assert_eq!(rx.try_recv(), Ok(Command::Volume(-VOLUME_STEP)));
        s.handle(InputEvent::Button(Button::DpadDown, false), now);
        s.tick(now + REPEAT_DELAY * 4);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn closes_when_idle() {
        let (mut s, _rx, now) = session(false);
        assert!(!s.expired(now + IDLE_CLOSE - Duration::from_millis(1)));
        assert!(s.expired(now + IDLE_CLOSE));
        press(&mut s, Button::South, now + Duration::from_secs(5));
        assert!(!s.expired(now + IDLE_CLOSE));
    }

    #[test]
    fn cycling_wraps_around_the_players() {
        let names = vec!["a".to_string(), "b".to_string()];
        assert_eq!(next_player(&names, Some("a")), Some("b".into()));
        assert_eq!(next_player(&names, Some("b")), Some("a".into()));
        assert_eq!(next_player(&names, None), Some("a".into()));
        assert_eq!(next_player(&[], None), None);
    }

    #[test]
    fn clock_formats() {
        assert_eq!(clock(65_000), "1:05");
        assert_eq!(clock(3_723_000), "1:02:03");
        assert_eq!(clock(-5), "0:00");
    }
}

//! The debug session's socket: applies requests to the virtual controller, and the client side
//! the command line uses to reach it.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use super::{
    bridge::Link,
    pad::VirtualPad,
    protocol::{DebugRequest, DebugResponse, Expectation},
};
use crate::{config::Button, info::Glyphs, ipc, pad_svg};

/// Where the session listens. `PADWIGHT_DEBUG_SOCKET` overrides it, so tests can run sessions side by side.
pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("PADWIGHT_DEBUG_SOCKET") {
        return path.into();
    }
    ipc::socket_path().with_file_name("padwight-debug.sock")
}

/// The virtual controller, shared between the socket's threads and the window.
#[derive(Clone, Default)]
pub struct Session {
    pad: Arc<Mutex<VirtualPad>>,
    quit: Arc<AtomicBool>,
    pub(super) link: Arc<Link>,
}

impl Session {
    pub fn new(pad: VirtualPad) -> Self {
        Self { pad: Arc::new(Mutex::new(pad)), quit: Arc::default(), link: Arc::default() }
    }

    pub fn pad(&self) -> MutexGuard<'_, VirtualPad> {
        self.pad.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether a `quit` request (or the window) asked the session to end.
    pub fn quitting(&self) -> bool {
        self.quit.load(Ordering::Relaxed)
    }

    pub fn quit(&self) {
        self.quit.store(true, Ordering::Relaxed);
    }

    /// Applies one request and says how it went.
    pub fn handle(&self, req: DebugRequest) -> DebugResponse {
        self.try_handle(req).unwrap_or_else(|e| DebugResponse::Error(format!("{e:#}")))
    }

    fn try_handle(&self, req: DebugRequest) -> Result<DebugResponse> {
        match req {
            DebugRequest::Ping => {}
            DebugRequest::State => return Ok(DebugResponse::State(self.pad().snapshot())),
            DebugRequest::Render => return Ok(DebugResponse::Text(self.render())),
            DebugRequest::Quit => self.quit(),
            DebugRequest::Expect(e) => return Ok(self.expect(&e)),
            DebugRequest::Output { clear } => return Ok(DebugResponse::Text(self.output(clear)?.join("\n"))),
            DebugRequest::Identify(identity) => self.identify(identity)?,
            DebugRequest::Tap { buttons, ms } => {
                buttons.iter().for_each(|&b| self.pad().press(b));
                self.flush()?;
                thread::sleep(Duration::from_millis(ms));
                buttons.iter().for_each(|&b| self.pad().release(b));
                self.flush()?;
            }
            DebugRequest::Gyro { rates, ms } if self.attached().is_some() => self.turn(rates, if ms == 0 { 100 } else { ms })?,
            other => {
                self.apply(other);
                self.flush()?;
            }
        }
        Ok(DebugResponse::Ok)
    }

    fn apply(&self, req: DebugRequest) {
        let mut pad = self.pad();
        match req {
            DebugRequest::Reset => pad.reset(),
            DebugRequest::Model(model) => pad.set_model(model),
            DebugRequest::Family(family) => pad.family = family,
            DebugRequest::Press(buttons) => buttons.into_iter().for_each(|b| pad.press(b)),
            DebugRequest::Release(buttons) => buttons.into_iter().for_each(|b| pad.release(b)),
            DebugRequest::Stick { stick, x, y } => pad.set_stick(stick, (x, y)),
            DebugRequest::Trigger { trigger, value } => pad.set_trigger(trigger, value),
            DebugRequest::Gyro { rates, .. } => pad.set_gyro(Some(rates)),
            _ => {}
        }
    }

    fn render(&self) -> String {
        let s = self.pad().snapshot();
        let glyphs = Glyphs { family: s.family.unwrap_or_default(), nintendo_layout: false };
        pad_svg::render(Some(&s), s.model, glyphs, &[])
    }

    fn expect(&self, e: &Expectation) -> DebugResponse {
        if let Expectation::Output(_) | Expectation::NoOutput = e {
            return self.expect_output(e);
        }
        let pad = self.pad();
        let held = |buttons: &[Button], want: bool| {
            let wrong: Vec<String> = buttons.iter().filter(|&&b| pad.is_pressed(b) != want).map(|b| format!("{b:?}")).collect();
            if wrong.is_empty() { None } else { Some(format!("{} {}", wrong.join(", "), if want { "not pressed" } else { "pressed" })) }
        };
        let near = |a: f32, b: f32| (a - b).abs() <= 0.01;
        let failure = match e {
            Expectation::Model(m) if pad.model != *m => Some(format!("model is {:?}", pad.model)),
            Expectation::Pressed(b) => held(b, true),
            Expectation::Released(b) => held(b, false),
            Expectation::Idle => {
                let s = pad.snapshot();
                let centered = s.left_stick == (0.0, 0.0) && s.right_stick == (0.0, 0.0) && s.left_trigger == 0.0 && s.right_trigger == 0.0;
                (!s.buttons.is_empty() || !centered).then(|| format!("not idle: {s:?}"))
            }
            Expectation::Stick { stick, x, y } => {
                let (sx, sy) = pad.stick(*stick);
                (!near(sx, *x) || !near(sy, *y)).then(|| format!("{stick} is at ({sx}, {sy})"))
            }
            Expectation::Trigger { trigger, value } => {
                let v = pad.trigger(*trigger);
                (!near(v, *value)).then(|| format!("{trigger} is at {v}"))
            }
            Expectation::Model(_) | Expectation::Output(_) | Expectation::NoOutput => None,
        };
        failure.map_or(DebugResponse::Ok, |why| DebugResponse::Error(format!("expectation failed: {why}")))
    }
}

impl Session {
    fn expect_output(&self, e: &Expectation) -> DebugResponse {
        let lines = match self.output(false) {
            Ok(lines) => lines,
            Err(err) => return DebugResponse::Error(format!("{err:#}")),
        };
        let failure = match e {
            Expectation::Output(line) if !lines.contains(line) => Some(format!("no output line {line:?}; the output is {lines:?}")),
            Expectation::NoOutput if !lines.is_empty() => Some(format!("output was {lines:?}")),
            _ => None,
        };
        failure.map_or(DebugResponse::Ok, |why| DebugResponse::Error(format!("expectation failed: {why}")))
    }
}

/// Starts listening at `path`; every connection is served on its own thread, any number of
/// requests per connection.
pub fn serve(session: &Session, path: &Path) -> Result<()> {
    let listener = crate::daemon::bind_socket_at(path)?;
    let session = session.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let session = session.clone();
            thread::spawn(move || {
                let _ = serve_connection(&session, &stream);
            });
        }
    });
    Ok(())
}

fn serve_connection(session: &Session, stream: &UnixStream) -> Result<()> {
    for line in BufReader::new(stream).lines() {
        let reply = match serde_json::from_str(&line?) {
            Ok(req) => session.handle(req),
            Err(e) => DebugResponse::Error(format!("bad request: {e}")),
        };
        let mut out = serde_json::to_string(&reply)?;
        out.push('\n');
        (&*stream).write_all(out.as_bytes())?;
    }
    Ok(())
}

/// A connection to a running session.
pub struct Client {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Client {
    /// Connects, retrying for up to `wait` while the session is still starting.
    pub fn connect(path: &Path, wait: Duration) -> Result<Self> {
        let start = Instant::now();
        let stream = loop {
            match UnixStream::connect(path) {
                Ok(stream) => break stream,
                Err(_) if start.elapsed() < wait => thread::sleep(Duration::from_millis(25)),
                Err(e) => return Err(e).with_context(|| format!("no debug session at {} (start one with `padwight debug`)", path.display())),
            }
        };
        // A tap or a slow script step can take a while, but a hung session shouldn't hang the caller.
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        Ok(Self { reader: BufReader::new(stream.try_clone()?), stream })
    }

    pub fn send(&mut self, req: &DebugRequest) -> Result<DebugResponse> {
        let mut line = serde_json::to_string(req)?;
        line.push('\n');
        self.stream.write_all(line.as_bytes())?;
        let mut reply = String::new();
        if self.reader.read_line(&mut reply)? == 0 {
            bail!("the debug session closed the connection");
        }
        Ok(serde_json::from_str(&reply)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{Stick, Trigger},
        info::PadModel,
    };

    fn temp_socket(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("padwight-debug-test-{}-{name}", std::process::id())).join("debug.sock")
    }

    #[test]
    fn requests_change_the_pad_and_expectations_check_it() {
        let session = Session::default();
        let ok = |r| assert_eq!(session.handle(r), DebugResponse::Ok);
        ok(DebugRequest::Model(Some(PadModel::XboxSeries)));
        ok(DebugRequest::Press(vec![Button::South, Button::DpadUp]));
        ok(DebugRequest::Stick { stick: Stick::Right, x: 0.5, y: -0.25 });
        ok(DebugRequest::Trigger { trigger: Trigger::Left, value: 1.0 });
        ok(DebugRequest::Expect(Expectation::Pressed(vec![Button::South])));
        ok(DebugRequest::Expect(Expectation::Stick { stick: Stick::Right, x: 0.5, y: -0.25 }));
        ok(DebugRequest::Expect(Expectation::Model(Some(PadModel::XboxSeries))));
        let bad = session.handle(DebugRequest::Expect(Expectation::Released(vec![Button::South])));
        assert!(matches!(bad, DebugResponse::Error(e) if e.contains("South")));
        ok(DebugRequest::Release(vec![Button::South]));
        ok(DebugRequest::Reset);
        ok(DebugRequest::Expect(Expectation::Idle));
        let DebugResponse::Text(svg) = session.handle(DebugRequest::Render) else { panic!("render") };
        assert!(svg.starts_with("<svg"));
        let DebugResponse::State(s) = session.handle(DebugRequest::State) else { panic!("state") };
        assert_eq!(s.model, Some(PadModel::XboxSeries), "reset keeps the model");
    }

    #[test]
    fn tap_releases_what_it_pressed() {
        let session = Session::default();
        session.handle(DebugRequest::Tap { buttons: vec![Button::Start], ms: 1 });
        assert!(!session.pad().is_pressed(Button::Start));
    }

    #[test]
    fn a_client_drives_a_served_session() {
        let path = temp_socket("client");
        let session = Session::default();
        serve(&session, &path).unwrap();
        let mut client = Client::connect(&path, Duration::from_secs(2)).unwrap();
        assert_eq!(client.send(&DebugRequest::Press(vec![Button::East])).unwrap(), DebugResponse::Ok);
        assert!(session.pad().is_pressed(Button::East));
        let DebugResponse::State(s) = client.send(&DebugRequest::State).unwrap() else { panic!("state") };
        assert_eq!(s.buttons, [Button::East]);
        assert_eq!(client.send(&DebugRequest::Quit).unwrap(), DebugResponse::Ok);
        assert!(session.quitting());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_second_session_cannot_take_the_socket() {
        let path = temp_socket("twice");
        serve(&Session::default(), &path).unwrap();
        assert!(serve(&Session::default(), &path).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}

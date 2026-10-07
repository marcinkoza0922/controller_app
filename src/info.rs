//! Info overlays: grids of text with `{tokens}` that turn into controller button glyphs
//! (drawn for the kind of controller in use) and live values such as the time or CPU load.
//! The daemon resolves an [`InfoOverlay`] into an [`InfoView`] for the overlay window.

use std::{
    collections::{HashMap, HashSet},
    fs,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::config::{Button, CurrentInput, InfoOverlay, OverlayStyle, Stick, Trigger};

/// Whose button names and symbols to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PadFamily {
    #[default]
    Xbox,
    PlayStation,
    Nintendo,
}

impl PadFamily {
    pub const ALL: [PadFamily; 3] = [PadFamily::Xbox, PadFamily::PlayStation, PadFamily::Nintendo];

    /// Recognizes the family from the USB/Bluetooth vendor ID, then the device name; `None`
    /// when neither gives it away, so the caller can fall back to the user's choice.
    pub fn detect(vendor: u16, name: &str) -> Option<Self> {
        match vendor {
            0x045e => return Some(PadFamily::Xbox),
            0x054c => return Some(PadFamily::PlayStation),
            0x057e => return Some(PadFamily::Nintendo),
            // Steam Deck and Steam Controller label their face buttons like Xbox pads.
            0x28de => return Some(PadFamily::Xbox),
            _ => {}
        }
        let name = name.to_lowercase();
        let has = |names: &[&str]| names.iter().any(|n| name.contains(n));
        if has(&["dualsense", "dualshock", "playstation", "sony"]) {
            Some(PadFamily::PlayStation)
        } else if has(&["nintendo", "pro controller", "joy-con"]) {
            Some(PadFamily::Nintendo)
        } else if has(&["xbox", "x-box", "microsoft"]) {
            Some(PadFamily::Xbox)
        } else {
            None
        }
    }
}

impl std::fmt::Display for PadFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PadFamily::Xbox => "Xbox",
            PadFamily::PlayStation => "PlayStation",
            PadFamily::Nintendo => "Nintendo",
        })
    }
}

/// A live value an info overlay can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    Time,
    Time12,
    Date,
    Profile,
    App,
    Title,
    Pid,
    Cpu,
    Ram,
    Gpu,
    Controller,
    Layer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Button(Button),
    Trigger(Trigger),
    /// The stick itself (for "move"), not its click.
    Stick(Stick),
    Dpad,
    Stat(Stat),
    /// The inputs just pressed, as the overlay's input settings say, except for what the
    /// token itself sets: `{current_input_device_N}` for one controller, then optionally
    /// `:gap_ms:stay_ms`.
    CurrentInput { device: Option<u8>, gap_ms: Option<u64>, stay_ms: Option<u64> },
}

/// Every token, with a description, for the editor's "Insert…" list.
pub const TOKENS: &[(&str, &str)] = &[
    ("south", "Bottom face button (A / ✕ / B)"),
    ("east", "Right face button (B / ○ / A)"),
    ("west", "Left face button (X / □ / Y)"),
    ("north", "Top face button (Y / △ / X)"),
    ("lb", "Left bumper"),
    ("rb", "Right bumper"),
    ("lt", "Left trigger"),
    ("rt", "Right trigger"),
    ("select", "Select / View / Share / −"),
    ("start", "Start / Menu / Options / +"),
    ("guide", "Guide / PS / Home"),
    ("ls", "Left stick"),
    ("rs", "Right stick"),
    ("l3", "Left stick click"),
    ("r3", "Right stick click"),
    ("dpad", "D-pad"),
    ("up", "D-pad up"),
    ("down", "D-pad down"),
    ("left", "D-pad left"),
    ("right", "D-pad right"),
    ("time", "Time (24-hour)"),
    ("time12", "Time (12-hour)"),
    ("date", "Date"),
    ("profile", "Active profile"),
    ("layer", "Active layers"),
    ("app", "Focused program's executable"),
    ("title", "Focused window's title"),
    ("pid", "Focused program's process ID"),
    ("cpu", "CPU usage"),
    ("ram", "Memory in use"),
    ("gpu", "GPU usage (AMD)"),
    ("controller", "Controller name"),
    ("current_input", "Buttons just pressed (set what it follows under Input, below the cells)"),
];

/// A token from what's between the braces: a name, and for `current_input` optional
/// `:`-separated timings.
fn token(inner: &str) -> Option<Token> {
    let mut parts = inner.split(':');
    let name = parts.next()?.trim().to_lowercase();
    let args: Vec<&str> = parts.map(str::trim).collect();
    if let Some(rest) = name.strip_prefix("current_input") {
        let device = match rest {
            "" => None,
            n => Some(n.strip_prefix("_device_")?.parse().ok()?),
        };
        // An argument that isn't a number leaves the whole token as text.
        let num = |i: usize| match args.get(i) {
            None => Some(None),
            Some(a) => a.parse::<u64>().ok().map(Some),
        };
        if args.len() > 2 {
            return None;
        }
        let (gap_ms, stay_ms) = (num(0)?, num(1)?);
        return Some(Token::CurrentInput { device, gap_ms, stay_ms });
    }
    if !args.is_empty() {
        return None;
    }
    Some(match name.as_str() {
        "south" => Token::Button(Button::South),
        "east" => Token::Button(Button::East),
        "west" => Token::Button(Button::West),
        "north" => Token::Button(Button::North),
        "lb" => Token::Button(Button::LeftBumper),
        "rb" => Token::Button(Button::RightBumper),
        "lt" => Token::Trigger(Trigger::Left),
        "rt" => Token::Trigger(Trigger::Right),
        "select" => Token::Button(Button::Select),
        "start" => Token::Button(Button::Start),
        "guide" => Token::Button(Button::Guide),
        "ls" => Token::Stick(Stick::Left),
        "rs" => Token::Stick(Stick::Right),
        "l3" => Token::Button(Button::LeftStick),
        "r3" => Token::Button(Button::RightStick),
        "dpad" => Token::Dpad,
        "up" => Token::Button(Button::DpadUp),
        "down" => Token::Button(Button::DpadDown),
        "left" => Token::Button(Button::DpadLeft),
        "right" => Token::Button(Button::DpadRight),
        "time" => Token::Stat(Stat::Time),
        "time12" => Token::Stat(Stat::Time12),
        "date" => Token::Stat(Stat::Date),
        "profile" => Token::Stat(Stat::Profile),
        "layer" => Token::Stat(Stat::Layer),
        "app" => Token::Stat(Stat::App),
        "title" => Token::Stat(Stat::Title),
        "pid" => Token::Stat(Stat::Pid),
        "cpu" => Token::Stat(Stat::Cpu),
        "ram" => Token::Stat(Stat::Ram),
        "gpu" => Token::Stat(Stat::Gpu),
        "controller" => Token::Stat(Stat::Controller),
        _ => return None,
    })
}

/// A piece of a cell: plain text, or a button glyph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Segment {
    Text(String),
    /// `fill` is the button's own color (e.g. Xbox A green); `None` draws it neutral.
    Glyph { label: String, fill: Option<[u8; 3]>, round: bool },
    /// A cross-shaped D-pad with its lit arms, `[up, down, left, right]` (two for a diagonal).
    Dpad([bool; 4]),
}

/// What the overlay window draws for one info overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoView {
    pub style: OverlayStyle,
    pub rows: Vec<Vec<Vec<Segment>>>,
    /// 1 normally, falling to 0 as it fades out (see [`fade`]).
    #[serde(default = "opaque")]
    pub opacity: f32,
}

fn opaque() -> f32 {
    1.0
}

/// How long an info overlay takes to fade out at the end of its time.
pub const FADE_OUT: std::time::Duration = std::time::Duration::from_millis(600);

/// Info overlays shown for a while rather than steadily: at the game's start, or lingering
/// after the action holding them lets go.
#[derive(Debug, Default)]
pub struct Timers {
    /// Shown for the game's start, and when each goes.
    start: HashMap<String, Instant>,
    /// Let go and lingering, and when each goes.
    lingering: HashMap<String, Instant>,
    /// What actions held up when last updated, to notice them let go.
    held: HashSet<String>,
}

impl Timers {
    /// The game started: its overlays set to show then do, for their time.
    pub fn game_started(&mut self, overlays: &[InfoOverlay], now: Instant) {
        self.start = overlays
            .iter()
            .filter_map(|o| o.on_start.map(|s| (o.name.clone(), now + Duration::from_secs_f32(s.max(0.0)))))
            .collect();
    }

    /// Another game: nothing timed carries over.
    pub fn clear(&mut self) {
        self.start.clear();
        self.lingering.clear();
    }

    /// Notes what actions hold up now. Overlays let go since last time stay their linger time.
    pub fn update(&mut self, held: HashSet<String>, overlays: &[InfoOverlay], now: Instant) {
        self.update_with(held, |name| overlays.iter().find(|o| o.name == name).and_then(|o| o.linger), now);
    }

    /// Like [`Timers::update`], for overlays whose lingering `linger` looks up by name.
    pub fn update_with(&mut self, held: HashSet<String>, linger: impl Fn(&str) -> Option<f32>, now: Instant) {
        for name in self.held.difference(&held) {
            if let Some(s) = linger(name) {
                self.lingering.insert(name.clone(), now + Duration::from_secs_f32(s.max(0.0)));
            }
        }
        self.lingering.retain(|name, end| !held.contains(name) && *end > now);
        self.start.retain(|_, end| *end > now);
        self.held = held;
    }

    /// Held up by an action right now.
    pub fn held(&self, name: &str) -> bool {
        self.held.contains(name)
    }

    /// Shown for a while right now.
    pub fn timed(&self, name: &str) -> bool {
        self.start.contains_key(name) || self.lingering.contains_key(name)
    }

    /// How visible a timed overlay is (0 once its time is up).
    pub fn opacity(&self, name: &str, now: Instant) -> f32 {
        [self.start.get(name), self.lingering.get(name)]
            .into_iter()
            .flatten()
            .filter_map(|end| fade(now, *end))
            .fold(0.0, f32::max)
    }

    /// When the overlay next needs redrawing for a timed overlay: as one starts to fade, then
    /// every frame while it does.
    pub fn next_redraw(&self, now: Instant, frame: Duration) -> Option<Instant> {
        self.start
            .values()
            .chain(self.lingering.values())
            .map(|end| redraw_at(*end, now, frame))
            .min()
    }
}

/// When something due to go at `end` next needs redrawing: as it starts to fade, then every
/// frame while it does.
pub fn redraw_at(end: Instant, now: Instant, frame: Duration) -> Instant {
    let fading = end.checked_sub(FADE_OUT).unwrap_or(now);
    if now >= fading { now + frame } else { fading }
}

/// How long a toast stays up, including its fade.
pub const TOAST_TIME: Duration = Duration::from_millis(2500);

/// A short notice (such as the profile just switched to), drawn like an info overlay at the
/// top of the screen and fading out after [`TOAST_TIME`].
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    lines: Vec<String>,
    end: Instant,
}

impl Toast {
    pub fn new(lines: Vec<String>, now: Instant) -> Self {
        Toast { lines, end: now + TOAST_TIME }
    }

    /// What to draw now, or `None` once it's gone.
    pub fn view(&self, now: Instant) -> Option<InfoView> {
        let opacity = fade(now, self.end)?;
        let rows = self.lines.iter().map(|l| vec![vec![Segment::Text(l.clone())]]).collect();
        Some(InfoView { style: OverlayStyle::toast(), rows, opacity })
    }

    pub fn next_redraw(&self, now: Instant, frame: Duration) -> Instant {
        redraw_at(self.end, now, frame)
    }
}

/// How visible an overlay due to go at `end` is at `now`: 1 until its last moments, then
/// fading to 0; `None` once it's gone.
pub fn fade(now: std::time::Instant, end: std::time::Instant) -> Option<f32> {
    let left = end.checked_duration_since(now).filter(|d| !d.is_zero())?;
    Some((left.as_secs_f32() / FADE_OUT.as_secs_f32()).min(1.0))
}

/// Splits a cell into text and tokens. Unknown `{words}` stay as they are.
pub fn parse(cell: &str) -> Vec<Result<String, Token>> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut rest = cell;
    while let Some(open) = rest.find('{') {
        let Some(len) = rest[open..].find('}') else { break };
        let inner = &rest[open + 1..open + len];
        text.push_str(&rest[..open]);
        match token(inner) {
            Some(t) => {
                if !text.is_empty() {
                    parts.push(Ok(std::mem::take(&mut text)));
                }
                parts.push(Err(t));
            }
            None => text.push_str(&rest[open..=open + len]),
        }
        rest = &rest[open + len + 1..];
    }
    text.push_str(rest);
    if !text.is_empty() {
        parts.push(Ok(text));
    }
    parts
}

/// What each `{current_input}` cell of an overlay follows: the overlay's input settings,
/// with whatever its token sets itself.
pub fn input_tokens(overlay: &InfoOverlay) -> Vec<CurrentInput> {
    overlay
        .rows
        .iter()
        .flatten()
        .flat_map(|cell| parse(cell))
        .filter_map(|p| match p {
            Err(Token::CurrentInput { device, gap_ms, stay_ms }) => Some(token_settings(&overlay.current_input, device, gap_ms, stay_ms)),
            _ => None,
        })
        .collect()
}

fn token_settings(base: &CurrentInput, device: Option<u8>, gap_ms: Option<u64>, stay_ms: Option<u64>) -> CurrentInput {
    let mut s = base.clone();
    s.tracking.device = device.or(s.tracking.device);
    s.tracking.gap_ms = gap_ms.unwrap_or(s.tracking.gap_ms);
    s.stay_ms = stay_ms.unwrap_or(s.stay_ms);
    s
}

/// Whether an overlay shows anything that changes over time (so it needs refreshing).
pub fn is_live(overlay: &InfoOverlay) -> bool {
    overlay.rows.iter().flatten().any(|cell| parse(cell).iter().any(|p| matches!(p, Err(Token::Stat(_)))))
}

pub fn glyph(label: &str, fill: Option<[u8; 3]>, round: bool) -> Segment {
    Segment::Glyph { label: label.to_string(), fill, round }
}

/// A button's glyph as the given family labels it. Face buttons are round and, for Xbox
/// and PlayStation, in their usual colors.
pub fn button_glyph(b: Button, family: PadFamily) -> Segment {
    use PadFamily::*;
    const GREEN: [u8; 3] = [0x3c, 0xa0, 0x3c];
    const RED: [u8; 3] = [0xc8, 0x3c, 0x3c];
    const BLUE: [u8; 3] = [0x2f, 0x6c, 0xc8];
    const YELLOW: [u8; 3] = [0xc8, 0xa0, 0x1e];
    const PS_BLUE: [u8; 3] = [0x5a, 0x8c, 0xdc];
    const PS_RED: [u8; 3] = [0xdc, 0x5a, 0x5a];
    const PS_PINK: [u8; 3] = [0xc8, 0x6e, 0xc8];
    const PS_GREEN: [u8; 3] = [0x32, 0xb4, 0x96];
    let face = |xbox: (&str, [u8; 3]), ps: (&str, [u8; 3]), nintendo: &str| match family {
        Xbox => glyph(xbox.0, Some(xbox.1), true),
        PlayStation => glyph(ps.0, Some(ps.1), true),
        Nintendo => glyph(nintendo, None, true),
    };
    let named = |xbox: &str, ps: &str, nintendo: &str| {
        glyph(
            match family {
                Xbox => xbox,
                PlayStation => ps,
                Nintendo => nintendo,
            },
            None,
            false,
        )
    };
    match b {
        Button::South => face(("A", GREEN), ("✕", PS_BLUE), "B"),
        Button::East => face(("B", RED), ("○", PS_RED), "A"),
        Button::West => face(("X", BLUE), ("□", PS_PINK), "Y"),
        Button::North => face(("Y", YELLOW), ("△", PS_GREEN), "X"),
        Button::LeftBumper => named("LB", "L1", "L"),
        Button::RightBumper => named("RB", "R1", "R"),
        Button::Select => named("View", "Share", "−"),
        Button::Start => named("Menu", "Options", "+"),
        Button::Guide => named("Guide", "PS", "Home"),
        Button::LeftStick => named("LS", "L3", "LS"),
        Button::RightStick => named("RS", "R3", "RS"),
        Button::DpadUp => Segment::Dpad([true, false, false, false]),
        Button::DpadDown => Segment::Dpad([false, true, false, false]),
        Button::DpadLeft => Segment::Dpad([false, false, true, false]),
        Button::DpadRight => Segment::Dpad([false, false, false, true]),
        other => glyph(crate::menu::button_badge(other), None, false),
    }
}

pub fn trigger_glyph(t: Trigger, family: PadFamily) -> Segment {
    let label = match (t, family) {
        (Trigger::Left, PadFamily::Xbox) => "LT",
        (Trigger::Right, PadFamily::Xbox) => "RT",
        (Trigger::Left, PadFamily::PlayStation) => "L2",
        (Trigger::Right, PadFamily::PlayStation) => "R2",
        (Trigger::Left, PadFamily::Nintendo) => "ZL",
        (Trigger::Right, PadFamily::Nintendo) => "ZR",
    };
    glyph(label, None, false)
}

/// Everything live values are read from.
#[derive(Debug, Clone, Default)]
pub struct Live {
    pub profile: String,
    pub app: String,
    pub title: String,
    pub pid: u32,
    pub controller: String,
    /// Active layers, oldest first.
    pub layers: Vec<String>,
    pub family: PadFamily,
    pub system: SystemStats,
    /// Recent input, for `{current_input}`.
    pub inputs: crate::inputlog::Inputs,
}

impl Live {
    /// Made-up values for the settings preview.
    pub fn sample(family: PadFamily) -> Self {
        Live {
            profile: "My game".into(),
            app: "game.exe".into(),
            title: "My Game".into(),
            pid: 4242,
            controller: format!("{family} controller"),
            layers: vec!["Hotkeys".into()],
            family,
            system: SystemStats { cpu: Some(23.0), ram: Some((7.4, 31.2)), gpu: Some(61.0) },
            inputs: crate::inputlog::Inputs::sample(),
        }
    }
}

fn stat(s: Stat, live: &Live) -> String {
    let or_dash = |v: &str| if v.is_empty() { "—".to_string() } else { v.to_string() };
    match s {
        Stat::Time => local_time("%H:%M"),
        Stat::Time12 => local_time("%-I:%M %p"),
        Stat::Date => local_time("%a %-d %b %Y"),
        Stat::Profile => or_dash(&live.profile),
        Stat::App => or_dash(&live.app),
        Stat::Title => or_dash(&live.title),
        Stat::Pid if live.pid == 0 => "—".into(),
        Stat::Pid => live.pid.to_string(),
        Stat::Cpu => live.system.cpu.map_or("n/a".into(), |c| format!("{c:.0}%")),
        Stat::Ram => live.system.ram.map_or("n/a".into(), |(used, total)| format!("{used:.1}/{total:.1} GB")),
        Stat::Gpu => live.system.gpu.map_or("n/a".into(), |g| format!("{g:.0}%")),
        Stat::Controller => or_dash(&live.controller),
        Stat::Layer => or_dash(&live.layers.join(" + ")),
    }
}

/// Turns an overlay's cells into text and glyphs, with live values filled in.
pub fn resolve(overlay: &InfoOverlay, live: &Live) -> InfoView {
    let rows = overlay
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    let mut segments: Vec<Segment> = Vec::new();
                    for part in parse(cell) {
                        let segment = match part {
                            Err(Token::CurrentInput { device, gap_ms, stay_ms }) => {
                                let settings = token_settings(&overlay.current_input, device, gap_ms, stay_ms);
                                segments.extend(live.inputs.current(&settings, live.family));
                                continue;
                            }
                            Ok(text) => Segment::Text(text),
                            Err(Token::Button(b)) => button_glyph(b, live.family),
                            Err(Token::Trigger(t)) => trigger_glyph(t, live.family),
                            Err(Token::Stick(Stick::Left)) => glyph("LS", None, true),
                            Err(Token::Stick(Stick::Right)) => glyph("RS", None, true),
                            Err(Token::Dpad) => glyph("✚", None, false),
                            Err(Token::Stat(s)) => Segment::Text(stat(s, live)),
                        };
                        // Live values join the text around them.
                        match (segments.last_mut(), segment) {
                            (Some(Segment::Text(prev)), Segment::Text(t)) => prev.push_str(&t),
                            (_, segment) => segments.push(segment),
                        }
                    }
                    segments
                })
                .collect()
        })
        .collect();
    InfoView { style: overlay.style.clone(), rows, opacity: 1.0 }
}

/// Formats the local time with a `strftime` pattern.
fn local_time(pattern: &str) -> String {
    let Ok(pattern) = std::ffi::CString::new(pattern) else { return String::new() };
    // SAFETY: localtime_r and strftime write into the buffers given; tm is plain data.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return String::new();
        }
        let mut buf = [0u8; 64];
        let len = libc::strftime(buf.as_mut_ptr().cast(), buf.len(), pattern.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..len]).into_owned()
    }
}

/// System load, as of the last sample.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemStats {
    /// Percent busy across all cores.
    pub cpu: Option<f32>,
    /// Used and total, in GB.
    pub ram: Option<(f32, f32)>,
    /// Percent busy, where the driver reports it (amdgpu).
    pub gpu: Option<f32>,
}

/// Samples CPU, memory and GPU load from /proc and /sys. CPU load is the change since the
/// previous sample.
#[derive(Default)]
pub struct Sampler {
    last_cpu: Option<(u64, u64)>,
    pub stats: SystemStats,
    pub sampled_at: Option<Instant>,
}

impl Sampler {
    pub fn sample(&mut self) {
        let cpu = cpu_times();
        self.stats.cpu = match (self.last_cpu, cpu) {
            (Some((busy0, total0)), Some((busy1, total1))) if total1 > total0 => {
                Some((busy1 - busy0) as f32 * 100.0 / (total1 - total0) as f32)
            }
            _ => self.stats.cpu,
        };
        self.last_cpu = cpu;
        self.stats.ram = memory();
        self.stats.gpu = gpu_busy();
        self.sampled_at = Some(Instant::now());
    }
}

/// Busy and total jiffies from the first line of /proc/stat.
fn cpu_times() -> Option<(u64, u64)> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let fields: Vec<u64> = stat.lines().next()?.split_whitespace().skip(1).filter_map(|f| f.parse().ok()).collect();
    let total: u64 = fields.iter().take(8).sum();
    // idle + iowait
    let idle = fields.get(3)? + fields.get(4).copied().unwrap_or(0);
    Some((total - idle, total))
}

fn memory() -> Option<(f32, f32)> {
    let info = fs::read_to_string("/proc/meminfo").ok()?;
    let field = |name: &str| -> Option<f32> {
        let line = info.lines().find(|l| l.starts_with(name))?;
        line.split_whitespace().nth(1)?.parse::<f32>().ok()
    };
    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    let gb = 1024.0 * 1024.0;
    Some(((total - available) / gb, total / gb))
}

fn gpu_busy() -> Option<f32> {
    let cards = fs::read_dir("/sys/class/drm").ok()?;
    cards
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with("card") && !n.contains('-')))
        .find_map(|e| fs::read_to_string(e.path().join("device/gpu_busy_percent")).ok())
        .and_then(|v| v.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_input_tokens_take_a_controller_and_timings() {
        let tok = |cell: &str| parse(cell).into_iter().find_map(Result::err);
        let input = |device, gap_ms, stay_ms| Some(Token::CurrentInput { device, gap_ms, stay_ms });
        assert_eq!(tok("{current_input}"), input(None, None, None));
        assert_eq!(tok("{current_input:400}"), input(None, Some(400), None));
        assert_eq!(tok("{current_input_device_1:400:2000}"), input(Some(1), Some(400), Some(2000)));
        for literal in ["{current_input:x}", "{current_input_device_x}", "{current_input:1:2:3}", "{time:5}"] {
            assert_eq!(parse(literal), vec![Ok(literal.to_string())], "{literal}");
        }
    }

    #[test]
    fn a_token_overrides_the_overlays_input_settings() {
        let mut o = overlay(&[&["{current_input}", "{current_input_device_2:400}"]]);
        o.current_input.tracking.device = Some(1);
        o.current_input.stay_ms = 3000;
        let cells = input_tokens(&o);
        assert_eq!((cells[0].tracking.device, cells[0].tracking.gap_ms, cells[0].stay_ms), (Some(1), 250, 3000));
        assert_eq!((cells[1].tracking.device, cells[1].tracking.gap_ms, cells[1].stay_ms), (Some(2), 400, 3000));
    }

    #[test]
    fn current_input_shows_the_sample_motion() {
        let o = overlay(&[&["{current_input}"]]);
        let cell = &resolve(&o, &Live::sample(PadFamily::Xbox)).rows[0][0];
        let labels: Vec<&str> = cell
            .iter()
            .filter_map(|s| match s {
                Segment::Glyph { label, .. } => Some(label.as_str()),
                Segment::Dpad(_) => Some("dpad"),
                Segment::Text(_) => None,
            })
            .collect();
        assert_eq!(labels, ["dpad", "dpad", "dpad", "B"]);
        assert_eq!(cell[1], Segment::Dpad([false, true, false, true]), "a diagonal lights two arms");
    }

    fn overlay(rows: &[&[&str]]) -> InfoOverlay {
        InfoOverlay {
            name: "Keys".into(),
            always: true, on_start: None, linger: None, current_input: Default::default(),
            style: OverlayStyle::info(),
            rows: rows.iter().map(|r| r.iter().map(std::string::ToString::to_string).collect()).collect(),
        }
    }

    #[test]
    fn tokens_become_glyphs_for_the_controller_in_use() {
        let o = overlay(&[&["{north} Reload", "{LT}+{south}"]]);
        let xbox = resolve(&o, &Live::sample(PadFamily::Xbox));
        assert_eq!(xbox.rows[0][0][0], Segment::Glyph { label: "Y".into(), fill: Some([0xc8, 0xa0, 0x1e]), round: true });
        assert_eq!(xbox.rows[0][0][1], Segment::Text(" Reload".into()));
        let ps = resolve(&o, &Live::sample(PadFamily::PlayStation));
        assert!(matches!(&ps.rows[0][0][0], Segment::Glyph { label, .. } if label == "△"));
        assert!(matches!(&ps.rows[0][1][0], Segment::Glyph { label, .. } if label == "L2"), "tokens are case-insensitive");
        let switch = resolve(&o, &Live::sample(PadFamily::Nintendo));
        assert!(matches!(&switch.rows[0][1][2], Segment::Glyph { label, .. } if label == "B"), "Nintendo's bottom button is B");
    }

    #[test]
    fn live_values_and_unknown_braces() {
        let o = overlay(&[&["CPU {cpu} · {app}", "{nope} {", "pid {pid}"]]);
        assert!(is_live(&o));
        let v = resolve(&o, &Live::sample(PadFamily::Xbox));
        assert_eq!(v.rows[0][0], vec![Segment::Text("CPU 23% · game.exe".into())]);
        assert_eq!(v.rows[0][1], vec![Segment::Text("{nope} {".into())]);
        assert_eq!(v.rows[0][2], vec![Segment::Text("pid 4242".into())]);
        assert!(!is_live(&overlay(&[&["{south} Jump"]])));
        assert_eq!(local_time("%Y").len(), 4);

        let layer = overlay(&[&["Layer: {layer}"]]);
        let mut live = Live::sample(PadFamily::Xbox);
        live.layers = vec!["Hotkeys".into(), "Build".into()];
        assert_eq!(resolve(&layer, &live).rows[0][0], vec![Segment::Text("Layer: Hotkeys + Build".into())]);
        live.layers.clear();
        assert_eq!(resolve(&layer, &live).rows[0][0], vec![Segment::Text("Layer: —".into())]);
    }

    #[test]
    fn overlays_fade_out_at_the_end_of_their_time() {
        let now = std::time::Instant::now();
        let s = std::time::Duration::from_secs_f32;
        assert_eq!(fade(now, now + s(5.0)), Some(1.0));
        let half = fade(now, now + FADE_OUT / 2).unwrap();
        assert!((half - 0.5).abs() < 0.01, "{half}");
        assert_eq!(fade(now, now), None);
        assert_eq!(fade(now + s(1.0), now), None);
    }

    #[test]
    fn toasts_show_their_lines_then_fade_away() {
        let now = Instant::now();
        let toast = Toast::new(vec!["Doom".into(), "Play".into()], now);
        let view = toast.view(now).unwrap();
        assert_eq!(view.rows, [[[Segment::Text("Doom".into())]], [[Segment::Text("Play".into())]]]);
        assert_eq!(view.opacity, 1.0);
        assert_eq!(toast.next_redraw(now, Duration::from_millis(33)), now + TOAST_TIME - FADE_OUT);
        assert!(toast.view(now + TOAST_TIME - FADE_OUT / 2).unwrap().opacity < 1.0);
        assert_eq!(toast.view(now + TOAST_TIME), None);
    }

    #[test]
    fn timed_overlays_show_at_start_and_linger_after_release() {
        let now = Instant::now();
        let s = Duration::from_secs_f32;
        let mut start = overlay(&[&["Loaded"]]);
        start.name = "Loaded".into();
        start.on_start = Some(3.0);
        let mut sheet = overlay(&[&["{south} Jump"]]);
        sheet.name = "Sheet".into();
        sheet.linger = Some(2.0);
        let overlays = [start, sheet];
        let mut t = Timers::default();

        t.game_started(&overlays, now);
        assert!(t.timed("Loaded") && !t.timed("Sheet"));
        assert_eq!(t.opacity("Loaded", now + s(1.0)), 1.0);
        assert!(t.next_redraw(now, s(0.03)).is_some_and(|w| w > now + s(2.0)), "wakes as it starts to fade");

        // Held, then let go: it lingers, fading at the end, and holding it again ends that.
        t.update(HashSet::from(["Sheet".to_string()]), &overlays, now);
        assert!(t.held("Sheet") && !t.timed("Sheet"));
        t.update(HashSet::new(), &overlays, now + s(1.0));
        assert!(t.timed("Sheet") && !t.held("Sheet"));
        assert!(t.opacity("Sheet", now + s(3.0) - FADE_OUT / 2) < 0.6);
        t.update(HashSet::from(["Sheet".to_string()]), &overlays, now + s(1.5));
        assert!(!t.timed("Sheet"));

        // Time's up for the start one.
        t.update(HashSet::new(), &overlays, now + s(4.0));
        assert!(!t.timed("Loaded"));
        assert_eq!(t.opacity("Loaded", now + s(4.0)), 0.0);
        t.clear();
        assert!(!t.timed("Sheet"));
    }

    #[test]
    fn families_from_vendor_and_name() {
        assert_eq!(PadFamily::detect(0x054c, "Wireless Controller"), Some(PadFamily::PlayStation));
        assert_eq!(PadFamily::detect(0x057e, "Pro Controller"), Some(PadFamily::Nintendo));
        assert_eq!(PadFamily::detect(0x045e, "Xbox Wireless Controller"), Some(PadFamily::Xbox));
        assert_eq!(PadFamily::detect(0x0000, "Nintendo Switch Pro Controller"), Some(PadFamily::Nintendo));
        assert_eq!(PadFamily::detect(0x0000, "Generic X-Box pad"), Some(PadFamily::Xbox));
        assert_eq!(PadFamily::detect(0x28de, "Steam Deck"), Some(PadFamily::Xbox));
        assert_eq!(PadFamily::detect(0x2dc8, "8BitDo Ultimate 2C"), None, "unknown pads use the fallback setting");
    }

    #[test]
    fn system_sampling_reads_proc() {
        let mut s = Sampler::default();
        s.sample();
        s.sample();
        assert!(s.stats.ram.is_some_and(|(used, total)| used > 0.0 && total >= used));
        assert!(s.stats.cpu.is_none_or(|c| (0.0..=100.0).contains(&c)));
    }
}

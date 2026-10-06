//! Info overlays: grids of text with `{tokens}` that turn into controller button glyphs
//! (drawn for the kind of controller in use) and live values such as the time or CPU load.
//! The daemon resolves an [`InfoOverlay`] into an [`InfoView`] for the overlay window.

use std::{fs, time::Instant};

use serde::{Deserialize, Serialize};

use crate::config::{Button, InfoOverlay, OverlayStyle, Stick, Trigger};

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
];

fn token(name: &str) -> Option<Token> {
    Some(match name.to_lowercase().as_str() {
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
}

/// What the overlay window draws for one info overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoView {
    pub style: OverlayStyle,
    pub rows: Vec<Vec<Vec<Segment>>>,
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
        match token(inner.trim()) {
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

/// Whether an overlay shows anything that changes over time (so it needs refreshing).
pub fn is_live(overlay: &InfoOverlay) -> bool {
    overlay.rows.iter().flatten().any(|cell| parse(cell).iter().any(|p| matches!(p, Err(Token::Stat(_)))))
}

fn glyph(label: &str, fill: Option<[u8; 3]>, round: bool) -> Segment {
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
        Button::DpadUp => glyph("↑", None, false),
        Button::DpadDown => glyph("↓", None, false),
        Button::DpadLeft => glyph("←", None, false),
        Button::DpadRight => glyph("→", None, false),
        other => glyph(crate::menu::button_badge(other), None, false),
    }
}

fn trigger_glyph(t: Trigger, family: PadFamily) -> Segment {
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
    InfoView { style: overlay.style.clone(), rows }
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

    fn overlay(rows: &[&[&str]]) -> InfoOverlay {
        InfoOverlay {
            name: "Keys".into(),
            always: true,
            style: OverlayStyle::info(),
            rows: rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect(),
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

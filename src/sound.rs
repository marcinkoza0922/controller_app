//! Overlay sounds: short cues, synthesized in code, played as the cursor moves in an overlay and
//! when something is picked there. A setup sets them for each kind of overlay, and any kind can
//! be silenced. Playback runs on its own thread so a slow audio device never holds up input.

use std::{
    f32::consts::{PI, TAU},
    fmt,
    sync::mpsc::{self, Sender},
    thread,
};

use serde::{Deserialize, Serialize};

/// Sample rate of every cue.
const RATE: u32 = 44_100;
/// Peak level of a cue before its volume is applied.
const PEAK: f32 = 0.9;

/// What a cue sounds like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundKind {
    /// A dry tick.
    #[default]
    Click,
    /// A small, bright wooden tick, like a metronome.
    Tick,
    /// A soft bubble that drops in pitch.
    Pop,
    /// A short square-ish bleep that rises. Retro and electronic.
    Blip,
    /// A bright two-note bell.
    Chime,
    /// A high metal ping with a rattle at the start.
    Clink,
    /// A low, short thump.
    Thud,
    /// A knuckle on wood: a dull knock with a little rap.
    Knock,
    /// A few crisp bursts of noise.
    Crunch,
    /// A breath of air that brightens as it passes.
    Swish,
    /// A key going down on a mechanical keyboard: a sharp click with a little thock under it.
    Keys,
    /// A typebar striking the paper: a dull clack with a metal ring.
    Typewriter,
    /// A single piano key struck: a warm, bright note that dies away.
    Piano,
    /// A clean, flat electronic beep, like a device acknowledging something.
    Beep,
}

impl SoundKind {
    pub const ALL: [SoundKind; 14] = [
        SoundKind::Click,
        SoundKind::Tick,
        SoundKind::Pop,
        SoundKind::Blip,
        SoundKind::Chime,
        SoundKind::Clink,
        SoundKind::Thud,
        SoundKind::Knock,
        SoundKind::Crunch,
        SoundKind::Swish,
        SoundKind::Keys,
        SoundKind::Typewriter,
        SoundKind::Piano,
        SoundKind::Beep,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SoundKind::Click => "Click",
            SoundKind::Tick => "Tick",
            SoundKind::Pop => "Pop",
            SoundKind::Blip => "Blip",
            SoundKind::Chime => "Chime",
            SoundKind::Clink => "Clink",
            SoundKind::Thud => "Thud",
            SoundKind::Knock => "Knock",
            SoundKind::Crunch => "Crunch",
            SoundKind::Swish => "Swish",
            SoundKind::Keys => "Keys",
            SoundKind::Typewriter => "Typewriter",
            SoundKind::Piano => "Piano",
            SoundKind::Beep => "Beep",
        }
    }

    /// How long the cue lasts, in seconds, at normal length.
    fn seconds(self) -> f32 {
        match self {
            SoundKind::Click => 0.03,
            SoundKind::Tick => 0.025,
            SoundKind::Pop => 0.09,
            SoundKind::Blip => 0.08,
            SoundKind::Chime => 0.45,
            SoundKind::Clink => 0.14,
            SoundKind::Thud => 0.22,
            SoundKind::Knock => 0.12,
            SoundKind::Crunch => 0.14,
            SoundKind::Swish => 0.2,
            SoundKind::Keys => 0.05,
            SoundKind::Typewriter => 0.12,
            SoundKind::Piano => 0.5,
            SoundKind::Beep => 0.12,
        }
    }
}

impl fmt::Display for SoundKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One cue: its kind, how loud it is, how high, and how long. Volume, pitch and length are
/// factors on the default, so 1.0 is what the kind sounds like on its own.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SoundSpec {
    #[serde(default)]
    pub kind: SoundKind,
    /// 0..=1.
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// 0.5..=2 (an octave either way).
    #[serde(default = "default_pitch")]
    pub pitch: f32,
    /// 0.5..=2 (half as long, or twice as long).
    #[serde(default = "default_length")]
    pub length: f32,
}

fn default_volume() -> f32 {
    0.25
}

fn default_pitch() -> f32 {
    1.0
}

fn default_length() -> f32 {
    1.0
}

impl SoundSpec {
    pub fn new(kind: SoundKind, volume: f32, pitch: f32) -> Self {
        SoundSpec { kind, volume, pitch, length: default_length() }
    }

    /// The cue's samples, mono at [`RATE`], with volume applied.
    pub fn samples(&self) -> Vec<f32> {
        let pitch = self.pitch.clamp(0.5, 2.0);
        let length = self.length.clamp(0.5, 2.0);
        let volume = self.volume.clamp(0.0, 1.0);
        let mut out = synthesize(self.kind, pitch, length);
        let peak = out.iter().fold(0.0_f32, |p, s| p.max(s.abs())).max(f32::EPSILON);
        let gain = PEAK / peak * volume;
        for s in &mut out {
            *s *= gain;
        }
        out
    }
}

/// The sounds of one kind of overlay: whether it makes any, a cue for each step of the cursor
/// and one for a pick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySounds {
    /// Off silences both.
    pub enabled: bool,
    pub step: SoundSpec,
    pub pick: SoundSpec,
}

impl Default for OverlaySounds {
    fn default() -> Self {
        OverlaySounds {
            enabled: true,
            step: SoundSpec::new(SoundKind::Click, 0.12, 1.2),
            pick: SoundSpec::new(SoundKind::Pop, 0.3, 1.0),
        }
    }
}

/// The overlays that have a cursor to sound. The Guide + Start menu is a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundOverlay {
    Keyboard,
    Numpad,
    Menu,
    Media,
    Offer,
}

impl SoundOverlay {
    pub const ALL: [SoundOverlay; 5] = [
        SoundOverlay::Keyboard,
        SoundOverlay::Numpad,
        SoundOverlay::Menu,
        SoundOverlay::Media,
        SoundOverlay::Offer,
    ];

    /// What the settings call this overlay.
    pub fn label(self) -> &'static str {
        match self {
            SoundOverlay::Keyboard => "On-screen keyboard",
            SoundOverlay::Numpad => "Numpad",
            SoundOverlay::Menu => "Menus",
            SoundOverlay::Media => "Media controls",
            SoundOverlay::Offer => "Library offers",
        }
    }
}

/// The sounds of every overlay. A game's own set replaces the defaults.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundSet {
    pub keyboard: OverlaySounds,
    pub numpad: OverlaySounds,
    pub menu: OverlaySounds,
    pub media: OverlaySounds,
    pub offer: OverlaySounds,
}

impl SoundSet {
    pub fn get(&self, overlay: SoundOverlay) -> OverlaySounds {
        match overlay {
            SoundOverlay::Keyboard => self.keyboard,
            SoundOverlay::Numpad => self.numpad,
            SoundOverlay::Menu => self.menu,
            SoundOverlay::Media => self.media,
            SoundOverlay::Offer => self.offer,
        }
    }

    pub fn set(&mut self, overlay: SoundOverlay, sounds: OverlaySounds) {
        let slot = match overlay {
            SoundOverlay::Keyboard => &mut self.keyboard,
            SoundOverlay::Numpad => &mut self.numpad,
            SoundOverlay::Menu => &mut self.menu,
            SoundOverlay::Media => &mut self.media,
            SoundOverlay::Offer => &mut self.offer,
        };
        *slot = sounds;
    }

    /// The cue for `feedback` in `overlay`, or `None` when that overlay is silenced.
    pub fn cue(&self, overlay: SoundOverlay, feedback: Feedback) -> Option<SoundSpec> {
        let sounds = self.get(overlay);
        if !sounds.enabled {
            return None;
        }
        Some(match feedback {
            Feedback::Step => sounds.step,
            Feedback::Pick => sounds.pick,
        })
    }

    /// The same set with every overlay silenced (their cues kept, for turning them back on).
    pub fn silenced(mut self) -> Self {
        for overlay in SoundOverlay::ALL {
            self.set(overlay, OverlaySounds { enabled: false, ..self.get(overlay) });
        }
        self
    }
}

/// What an input did in an overlay: a pick, or a step of the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feedback {
    Step,
    Pick,
}

impl Feedback {
    /// A pick if something was chosen, else a step if the cursor moved, else nothing.
    pub fn of(picked: bool, moved: bool) -> Option<Self> {
        if picked {
            Some(Feedback::Pick)
        } else if moved {
            Some(Feedback::Step)
        } else {
            None
        }
    }
}

/// Synthesizes a cue at `pitch` and `length` (already clamped), normalized to a peak of 1
/// before volume.
fn synthesize(kind: SoundKind, pitch: f32, length: f32) -> Vec<f32> {
    let dur = kind.seconds() * length;
    let len = (dur * RATE as f32) as usize;
    // A little noise that is the same every time, so cues don't change between plays.
    let mut seed = 0x2545_f491_u32;
    let mut noise = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32 * 2.0 - 1.0
    };
    let mut out: Vec<f32> = (0..len)
        .map(|i| tone(kind, pitch, dur, i as f32 / RATE as f32, &mut noise))
        .collect();
    match kind {
        SoundKind::Crunch => lowpass(&mut out, |_| 0.35),
        // The air gets brighter as it goes by.
        SoundKind::Swish => {
            let n = out.len().max(1) as f32;
            lowpass(&mut out, |i| 0.03 + 0.4 * i as f32 / n);
        }
        _ => {}
    }
    // Fade the end so no cue stops with a click.
    let fade = (RATE as f32 * 0.004) as usize;
    let n = out.len();
    for (i, s) in out.iter_mut().enumerate().skip(n.saturating_sub(fade)) {
        *s *= (n - 1 - i) as f32 / fade as f32;
    }
    let peak = out.iter().fold(0.0_f32, |p, s| p.max(s.abs())).max(f32::EPSILON);
    out.iter_mut().for_each(|s| *s /= peak);
    out
}

/// One-pole low-pass over `out`, its smoothing (`alpha`, 0..=1) given per sample.
fn lowpass(out: &mut [f32], alpha: impl Fn(usize) -> f32) {
    let mut y = 0.0;
    for (i, s) in out.iter_mut().enumerate() {
        y += alpha(i) * (*s - y);
        *s = y;
    }
}

/// One sample of a cue, `x` seconds in, of a cue that lasts `dur` seconds.
fn tone(kind: SoundKind, pitch: f32, dur: f32, x: f32, noise: &mut impl FnMut() -> f32) -> f32 {
    match kind {
        SoundKind::Click => noise() * (-x * 160.0).exp() * 0.6 + (TAU * 1800.0 * pitch * x).sin() * (-x * 220.0).exp() * 0.5,
        SoundKind::Tick => (TAU * 3200.0 * pitch * x).sin() * (-x * 240.0).exp() * 0.8 + (TAU * 6400.0 * pitch * x).sin() * (-x * 400.0).exp() * 0.25,
        SoundKind::Pop => {
            // A sweep from high to low, the pitch falling as the bubble lets go.
            let f = 700.0 * pitch * (1.0 - 0.7 * x / dur);
            (TAU * f * x).sin() * (-x * 38.0).exp()
        }
        SoundKind::Blip => {
            // A square wave softened with a sine, rising as it plays.
            let f = pitch * (660.0 + 330.0 * x / dur);
            let s = (TAU * f * x).sin();
            let square = if s >= 0.0 { 1.0 } else { -1.0 };
            (0.5 * square + 0.5 * s) * (-x * 22.0).exp()
        }
        SoundKind::Chime => {
            let f = 1046.5 * pitch;
            (TAU * f * x).sin() * (-x * 7.0).exp() + 0.45 * (TAU * f * 2.76 * x).sin() * (-x * 11.0).exp() + 0.2 * (TAU * f * 5.4 * x).sin() * (-x * 16.0).exp()
        }
        SoundKind::Clink => {
            let f = 2300.0 * pitch;
            (TAU * f * x).sin() * (-x * 55.0).exp() + 0.5 * (TAU * f * 2.76 * x).sin() * (-x * 90.0).exp() + 0.25 * noise() * (-x * 600.0).exp()
        }
        SoundKind::Thud => {
            let f = (160.0 * pitch) * (1.0 - 0.6 * x / dur);
            (TAU * f * x).sin() * (-x * 22.0).exp()
        }
        SoundKind::Knock => {
            let f = 300.0 * pitch * (1.0 - 0.35 * x / dur);
            (TAU * f * x).sin() * (-x * 60.0).exp() * 0.9 + 0.35 * noise() * (-x * 500.0).exp()
        }
        SoundKind::Crunch => {
            // Noise in three short bursts.
            let burst: f32 = [0.0_f32, 0.045, 0.09]
                .iter()
                .map(|start| if x < *start { 0.0 } else { (-(x - start) * 90.0).exp() })
                .sum();
            noise() * burst * (0.6 + 0.2 * pitch)
        }
        SoundKind::Swish => noise() * (PI * x / dur).sin(),
        SoundKind::Keys => {
            // The switch's click, the keycap's thock under it, and the spring's tick.
            noise() * (-x * 300.0).exp() * 0.5 + (TAU * 220.0 * pitch * x).sin() * (-x * 120.0).exp() * 0.4 + (TAU * 3500.0 * pitch * x).sin() * (-x * 400.0).exp() * 0.3
        }
        SoundKind::Typewriter => {
            // The type slug hits the platen: a hard transient, the metal bar ringing at
            // inharmonic partials, and a second, softer thud as the paper takes it.
            let ring = [(1480.0, 0.4, 70.0), (2350.0, 0.3, 90.0), (3100.0, 0.15, 120.0)]
                .iter()
                .map(|&(f, gain, decay)| (TAU * f * pitch * x).sin() * gain * (-x * decay).exp())
                .sum::<f32>();
            let strike = noise() * (-x * 500.0).exp() * 0.8;
            let paper = if x < 0.018 { 0.0 } else { noise() * (-(x - 0.018) * 250.0).exp() * 0.5 };
            strike + ring + paper
        }
        SoundKind::Piano => {
            // Harmonics that die at different rates, the higher ones first, and a hammer tap.
            let f = 523.25 * pitch;
            let partial = |k: f32, gain: f32, decay: f32| (TAU * f * k * x).sin() * gain * (-x * decay).exp();
            partial(1.0, 1.0, 4.0) + partial(2.0, 0.5, 7.0) + partial(3.0, 0.25, 12.0) + partial(4.0, 0.12, 18.0) + noise() * (-x * 800.0).exp() * 0.1
        }
        SoundKind::Beep => {
            // A flat tone with a short fade in, so it doesn't start with a click.
            let attack = (x / 0.004).min(1.0);
            (TAU * 880.0 * pitch * x).sin() * attack
        }
    }
}

/// Plays cues on a background thread. If no audio device opens, cues are dropped silently.
pub struct Sounds {
    tx: Option<Sender<SoundSpec>>,
}

impl Sounds {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<SoundSpec>();
        let spawned = thread::Builder::new().name("padwight-sound".into()).spawn(move || {
            let Ok(sink) = rodio::DeviceSinkBuilder::open_default_sink() else {
                return;
            };
            for spec in rx {
                let player = rodio::Player::connect_new(sink.mixer());
                player.append(rodio::buffer::SamplesBuffer::new(MONO, RATE_NZ, spec.samples()));
                player.detach();
            }
        });
        Sounds { tx: spawned.ok().map(|_| tx) }
    }

    /// Plays `spec` now. Cues queue up behind each other rather than overlapping.
    pub fn play(&self, spec: &SoundSpec) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(*spec);
        }
    }
}

/// [`RATE`] and mono as the types the audio library takes.
const RATE_NZ: std::num::NonZero<u32> = std::num::NonZero::<u32>::new(RATE).unwrap();
const MONO: std::num::NonZero<u16> = std::num::NonZero::<u16>::MIN;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_cue_of_its_length_within_range() {
        for kind in SoundKind::ALL {
            let samples = SoundSpec::new(kind, 1.0, 1.0).samples();
            assert_eq!(samples.len(), (kind.seconds() * RATE as f32) as usize, "{kind:?}");
            assert!(samples.iter().all(|s| s.abs() <= 1.0), "{kind:?}");
            assert!(samples.iter().any(|s| s.abs() > 0.5), "{kind:?} is silent");
        }
    }

    #[test]
    fn length_stretches_the_cue() {
        let normal = SoundSpec::new(SoundKind::Knock, 1.0, 1.0).samples().len();
        let long = SoundSpec { length: 2.0, ..SoundSpec::new(SoundKind::Knock, 1.0, 1.0) }.samples().len();
        let short = SoundSpec { length: 0.5, ..SoundSpec::new(SoundKind::Knock, 1.0, 1.0) }.samples().len();
        assert!((long as f32 / normal as f32 - 2.0).abs() < 0.01);
        assert!((short as f32 / normal as f32 - 0.5).abs() < 0.01);
    }

    #[test]
    fn volume_scales_the_peak_and_zero_is_silent() {
        let loud = SoundSpec::new(SoundKind::Chime, 1.0, 1.0).samples();
        let quiet = SoundSpec::new(SoundKind::Chime, 0.5, 1.0).samples();
        let peak = |v: &[f32]| v.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
        assert!((peak(&loud) - PEAK).abs() < 1e-3);
        assert!((peak(&quiet) - PEAK / 2.0).abs() < 1e-3);
        assert!(SoundSpec::new(SoundKind::Pop, 0.0, 1.0).samples().iter().all(|s| *s == 0.0));
    }

    #[test]
    fn higher_pitch_means_more_cycles_in_a_click() {
        let zero_crossings = |v: &[f32]| v.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        let low = zero_crossings(&SoundSpec::new(SoundKind::Chime, 1.0, 0.5).samples());
        let high = zero_crossings(&SoundSpec::new(SoundKind::Chime, 1.0, 2.0).samples());
        assert!(high > low * 2, "{low} vs {high}");
    }

    #[test]
    fn cues_end_quietly() {
        for kind in SoundKind::ALL {
            let samples = SoundSpec::new(kind, 1.0, 1.0).samples();
            assert!(samples.last().is_some_and(|s| s.abs() < 1e-3), "{kind:?}");
        }
    }

    #[test]
    fn overlay_sounds_default_on_and_load_from_nothing() {
        let set: SoundSet = toml::from_str("").unwrap();
        assert_eq!(set, SoundSet::default());
        for overlay in SoundOverlay::ALL {
            let sounds = set.get(overlay);
            assert!(sounds.enabled, "{overlay:?}");
            assert!(sounds.step.volume < sounds.pick.volume, "steps are fainter than picks");
        }
    }

    #[test]
    fn a_table_sets_one_overlay_and_leaves_the_rest() {
        let set: SoundSet = toml::from_str("[menu]\nenabled = false\nstep = { kind = \"knock\", length = 1.5 }").unwrap();
        assert!(!set.menu.enabled);
        assert_eq!(set.menu.step.kind, SoundKind::Knock);
        assert_eq!(set.menu.step.length, 1.5);
        assert_eq!(set.menu.pick, OverlaySounds::default().pick);
        assert_eq!(set.keyboard, OverlaySounds::default());
    }

    #[test]
    fn a_silenced_overlay_plays_nothing_and_others_do() {
        let mut set = SoundSet::default();
        set.menu.enabled = false;
        assert_eq!(set.cue(SoundOverlay::Menu, Feedback::Step), None);
        assert_eq!(set.cue(SoundOverlay::Menu, Feedback::Pick), None);
        assert_eq!(set.cue(SoundOverlay::Keyboard, Feedback::Pick), Some(set.keyboard.pick));
        assert!(!SoundSet::default().silenced().keyboard.enabled);
    }

    #[test]
    fn feedback_prefers_a_pick() {
        assert_eq!(Feedback::of(true, true), Some(Feedback::Pick));
        assert_eq!(Feedback::of(false, true), Some(Feedback::Step));
        assert_eq!(Feedback::of(false, false), None);
    }
}

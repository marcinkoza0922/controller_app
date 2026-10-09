//! Menu sounds: short cues, synthesized in code, played as the menu cursor moves and when an
//! item is chosen. Playback runs on its own thread so a slow audio device never holds up input.

use std::{
    f32::consts::TAU,
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
    /// A soft bubble that drops in pitch.
    Pop,
    /// A bright two-note bell.
    Chime,
    /// A low, short thump.
    Thud,
    /// A few crisp bursts of noise.
    Crunch,
}

impl SoundKind {
    pub const ALL: [SoundKind; 5] = [
        SoundKind::Click,
        SoundKind::Pop,
        SoundKind::Chime,
        SoundKind::Thud,
        SoundKind::Crunch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SoundKind::Click => "Click",
            SoundKind::Pop => "Pop",
            SoundKind::Chime => "Chime",
            SoundKind::Thud => "Thud",
            SoundKind::Crunch => "Crunch",
        }
    }

    /// How long the cue lasts, in seconds, at normal pitch.
    fn seconds(self) -> f32 {
        match self {
            SoundKind::Click => 0.03,
            SoundKind::Pop => 0.09,
            SoundKind::Chime => 0.45,
            SoundKind::Thud => 0.22,
            SoundKind::Crunch => 0.14,
        }
    }
}

impl fmt::Display for SoundKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One cue: its kind, how loud it is, and how high. Volume and pitch are factors on the
/// default, so 1.0 is what the kind sounds like on its own.
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
}

fn default_volume() -> f32 {
    0.25
}

fn default_pitch() -> f32 {
    1.0
}

impl SoundSpec {
    pub fn new(kind: SoundKind, volume: f32, pitch: f32) -> Self {
        SoundSpec {
            kind,
            volume,
            pitch,
        }
    }

    /// The cue's samples, mono at [`RATE`], with volume applied.
    pub fn samples(&self) -> Vec<f32> {
        let pitch = self.pitch.clamp(0.5, 2.0);
        let volume = self.volume.clamp(0.0, 1.0);
        let mut out = synthesize(self.kind, pitch);
        let peak = out
            .iter()
            .fold(0.0_f32, |p, s| p.max(s.abs()))
            .max(f32::EPSILON);
        let gain = PEAK / peak * volume;
        for s in &mut out {
            *s *= gain;
        }
        out
    }
}

/// The menu sounds of one setup: a cue for each step of the cursor and one for a choice.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MenuSounds {
    /// Off silences both.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_step")]
    pub step: SoundSpec,
    #[serde(default = "default_pick")]
    pub pick: SoundSpec,
}

fn default_enabled() -> bool {
    true
}

fn default_step() -> SoundSpec {
    SoundSpec::new(SoundKind::Click, 0.12, 1.2)
}

fn default_pick() -> SoundSpec {
    SoundSpec::new(SoundKind::Pop, 0.3, 1.0)
}

impl Default for MenuSounds {
    fn default() -> Self {
        MenuSounds {
            enabled: default_enabled(),
            step: default_step(),
            pick: default_pick(),
        }
    }
}

/// Synthesizes a cue at `pitch` (already clamped), normalized to a peak of 1 before volume.
fn synthesize(kind: SoundKind, pitch: f32) -> Vec<f32> {
    let len = (kind.seconds() * RATE as f32) as usize;
    // A little noise that is the same every time, so cues don't change between plays.
    let mut seed = 0x2545_f491_u32;
    let mut noise = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32 * 2.0 - 1.0
    };
    let mut out: Vec<f32> = (0..len)
        .map(|i| tone(kind, pitch, i as f32 / RATE as f32, &mut noise))
        .collect();
    if kind == SoundKind::Crunch {
        // One-pole low-pass to take the edge off the noise.
        let alpha = 0.35;
        let mut y = 0.0;
        for s in &mut out {
            y += alpha * (*s - y);
            *s = y;
        }
    }
    // Fade the end so no cue stops with a click.
    let fade = (RATE as f32 * 0.004) as usize;
    let n = out.len();
    for (i, s) in out.iter_mut().enumerate().skip(n.saturating_sub(fade)) {
        *s *= (n - 1 - i) as f32 / fade as f32;
    }
    let peak = out
        .iter()
        .fold(0.0_f32, |p, s| p.max(s.abs()))
        .max(f32::EPSILON);
    out.iter_mut().for_each(|s| *s /= peak);
    out
}

/// One sample of a cue, `x` seconds in.
fn tone(kind: SoundKind, pitch: f32, x: f32, noise: &mut impl FnMut() -> f32) -> f32 {
    match kind {
        SoundKind::Click => {
            noise() * (-x * 160.0).exp() * 0.6
                + (TAU * 1800.0 * pitch * x).sin() * (-x * 220.0).exp() * 0.5
        }
        SoundKind::Pop => {
            // A sweep from high to low, the pitch falling as the bubble lets go.
            let f = 700.0 * pitch * (1.0 - 0.7 * x / kind.seconds());
            (TAU * f * x).sin() * (-x * 38.0).exp()
        }
        SoundKind::Chime => {
            let f = 1046.5 * pitch;
            (TAU * f * x).sin() * (-x * 7.0).exp()
                + 0.45 * (TAU * f * 2.76 * x).sin() * (-x * 11.0).exp()
                + 0.2 * (TAU * f * 5.4 * x).sin() * (-x * 16.0).exp()
        }
        SoundKind::Thud => {
            let f = (160.0 * pitch) * (1.0 - 0.6 * x / kind.seconds());
            (TAU * f * x).sin() * (-x * 22.0).exp()
        }
        SoundKind::Crunch => {
            // Noise in three short bursts.
            let burst: f32 = [0.0_f32, 0.045, 0.09]
                .iter()
                .map(|start| {
                    if x < *start {
                        0.0
                    } else {
                        (-(x - start) * 90.0).exp()
                    }
                })
                .sum();
            noise() * burst * (0.6 + 0.2 * pitch)
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
        let spawned = thread::Builder::new()
            .name("padwight-sound".into())
            .spawn(move || {
                let Ok(sink) = rodio::DeviceSinkBuilder::open_default_sink() else {
                    return;
                };
                for spec in rx {
                    let player = rodio::Player::connect_new(sink.mixer());
                    player.append(rodio::buffer::SamplesBuffer::new(
                        MONO,
                        RATE_NZ,
                        spec.samples(),
                    ));
                    player.detach();
                }
            });
        Sounds {
            tx: spawned.ok().map(|_| tx),
        }
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
            assert_eq!(
                samples.len(),
                (kind.seconds() * RATE as f32) as usize,
                "{kind:?}"
            );
            assert!(samples.iter().all(|s| s.abs() <= 1.0), "{kind:?}");
            assert!(samples.iter().any(|s| s.abs() > 0.5), "{kind:?} is silent");
        }
    }

    #[test]
    fn volume_scales_the_peak_and_zero_is_silent() {
        let loud = SoundSpec::new(SoundKind::Chime, 1.0, 1.0).samples();
        let quiet = SoundSpec::new(SoundKind::Chime, 0.5, 1.0).samples();
        let peak = |v: &[f32]| v.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
        assert!((peak(&loud) - PEAK).abs() < 1e-3);
        assert!((peak(&quiet) - PEAK / 2.0).abs() < 1e-3);
        assert!(
            SoundSpec::new(SoundKind::Pop, 0.0, 1.0)
                .samples()
                .iter()
                .all(|s| *s == 0.0)
        );
    }

    #[test]
    fn higher_pitch_means_more_cycles_in_a_click() {
        let zero_crossings = |v: &[f32]| {
            v.windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count()
        };
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
    fn menu_sounds_default_on_and_load_from_nothing() {
        let sounds: MenuSounds = toml::from_str("").unwrap();
        assert_eq!(sounds, MenuSounds::default());
        assert!(sounds.enabled);
        assert!(
            sounds.step.volume < sounds.pick.volume,
            "steps are fainter than picks"
        );
    }
}

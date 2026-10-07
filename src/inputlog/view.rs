//! Turning recorded entries into what overlays draw: sequences (presses with no pause longer
//! than a gap between them), repeats folded together, glyphs for the controller in use.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::{Entry, Input};
use crate::{
    config::{InputLogSettings, LogEnd, OverlayStyle, Stick},
    info::{PadFamily, Segment, button_glyph, glyph, trigger_glyph},
};

/// What the overlay window draws for one log overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogView {
    pub style: OverlayStyle,
    /// 1 normally, falling to 0 as the overlay fades out.
    pub opacity: f32,
    /// In drawing order, top first.
    pub lines: Vec<LogLine>,
    pub show_labels: bool,
    pub show_holds: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogLine {
    /// 1 normally, falling to 0 as the line fades out.
    pub opacity: f32,
    pub cells: Vec<LogCell>,
}

/// One input of a line: the button (or direction, or combo) and what it did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogCell {
    pub glyph: Vec<Segment>,
    pub label: Option<String>,
    /// How long it was held (the average, when repeats are merged); unset while held.
    pub hold_ms: Option<u32>,
    /// Presses merged into this cell.
    pub count: u16,
    /// Still held: drawn bright, and dimmed once let go.
    pub held: bool,
}

/// The entries split into sequences: a press more than `gap` after everything before it
/// was let go starts a new one.
pub(super) fn sequences(entries: &[Entry], gap: Duration) -> Vec<&[Entry]> {
    let mut out = Vec::new();
    let mut start = 0;
    // Some input of the current sequence is still held, so nothing after it starts another.
    let mut open = false;
    let mut last: Option<Instant> = None;
    for (i, e) in entries.iter().enumerate() {
        if i > start && !open && last.is_some_and(|l| e.pressed.saturating_duration_since(l) > gap) {
            out.push(&entries[start..i]);
            start = i;
            last = None;
        }
        match e.released {
            Some(r) => last = last.max(Some(r)),
            None => open = true,
        }
    }
    if start < entries.len() {
        out.push(&entries[start..]);
    }
    out
}

/// When a sequence's last input was let go; `None` while any is held.
fn ended(seq: &[Entry]) -> Option<Instant> {
    seq.iter().map(|e| e.released).collect::<Option<Vec<_>>>()?.into_iter().max()
}

/// Folds presses of the same input (with the same label) in a row into one cell.
fn cells(seq: &[Entry], merge: bool, family: PadFamily) -> Vec<LogCell> {
    let mut out: Vec<(LogCell, &Entry, Vec<Duration>)> = Vec::new();
    for e in seq {
        let hold = e.released.map(|r| r.saturating_duration_since(e.pressed));
        if merge
            && let Some((cell, prev, holds)) = out.last_mut()
            && prev.input == e.input
            && prev.label == e.label
        {
            cell.count += 1;
            cell.held |= e.released.is_none();
            holds.extend(hold);
            continue;
        }
        let cell = LogCell { glyph: input_glyph(&e.input, family), label: e.label.clone(), hold_ms: None, count: 1, held: e.released.is_none() };
        out.push((cell, e, hold.into_iter().collect()));
    }
    out.into_iter()
        .map(|(mut cell, _, holds)| {
            if !cell.held && !holds.is_empty() {
                let total: Duration = holds.iter().sum();
                cell.hold_ms = Some((total.as_millis() / holds.len() as u128).try_into().unwrap_or(u32::MAX));
            }
            cell
        })
        .collect()
}

/// An input's glyphs: the button's own, an arrow for a direction (in a disc with the
/// stick's letter for a stick), and a combo's buttons joined by `+`.
pub fn input_glyph(input: &Input, family: PadFamily) -> Vec<Segment> {
    match input {
        Input::Button(b) => vec![button_glyph(*b, family)],
        Input::Direction(None, d) => vec![glyph(d.arrow(), None, false)],
        Input::Direction(Some(s), d) => {
            let letter = if *s == Stick::Left { "L" } else { "R" };
            vec![glyph(&format!("{letter}{}", d.arrow()), None, true)]
        }
        Input::Trigger(t) => vec![trigger_glyph(*t, family)],
        Input::Combo(members) => {
            let mut out = Vec::new();
            for (i, b) in members.iter().enumerate() {
                if i > 0 {
                    out.push(Segment::Text("+".into()));
                }
                out.push(button_glyph(*b, family));
            }
            out
        }
    }
}

/// `{current_input}`: the latest sequence, in one line, until `stay` after it ends (`gap`
/// after its last input is let go). Empty once it's gone.
pub fn current_input(entries: &[Entry], gap: Duration, stay: Duration, family: PadFamily, now: Instant) -> Vec<Segment> {
    let Some(seq) = sequences(entries, gap).pop() else { return Vec::new() };
    if ended(seq).is_some_and(|end| now >= end + gap + stay) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for cell in cells(seq, true, family) {
        out.extend(cell.glyph);
        if cell.count > 1 {
            out.push(Segment::Text(format!("×{}", cell.count)));
        }
    }
    out
}

/// A log overlay's lines, newest at the end its settings name. Lines fade out `fade_after`
/// seconds after they end.
pub fn log_view(entries: &[Entry], s: &InputLogSettings, style: &OverlayStyle, family: PadFamily, now: Instant) -> LogView {
    let all = sequences(entries, Duration::from_millis(s.gap_ms));
    let skip = all.len().saturating_sub(usize::from(s.lines.max(1)));
    let mut lines: Vec<LogLine> = all[skip..]
        .iter()
        .filter_map(|seq| {
            let opacity = match (fade_after(s), ended(seq)) {
                (Some(after), Some(end)) => crate::info::fade(now, end + after)?,
                _ => 1.0,
            };
            Some(LogLine { opacity, cells: cells(seq, s.merge_repeats, family) })
        })
        .collect();
    if s.newest == LogEnd::Top {
        lines.reverse();
    }
    LogView { style: style.clone(), opacity: 1.0, lines, show_labels: s.show_labels, show_holds: s.show_holds }
}

fn fade_after(s: &InputLogSettings) -> Option<Duration> {
    (s.fade_after > 0.0).then(|| Duration::from_secs_f32(s.fade_after))
}

/// When something shown from these entries next changes on its own: the time each
/// sequence's display is due to end (`after` past its end), as fades start or every
/// `frame` during one.
pub fn next_change(entries: &[Entry], gap: Duration, after: Duration, now: Instant, frame: Duration) -> Option<Instant> {
    sequences(entries, gap)
        .into_iter()
        .filter_map(ended)
        .map(|end| end + after)
        .filter(|due| *due > now)
        .map(|due| crate::info::redraw_at(due, now, frame))
        .min()
}

/// Every controller's entries in one list, by when they were pressed.
pub fn merged<'a>(logs: impl IntoIterator<Item = &'a [Entry]>) -> Vec<Entry> {
    let mut all: Vec<Entry> = logs.into_iter().flatten().cloned().collect();
    all.sort_by_key(|e| e.pressed);
    all
}

/// How long a log overlay keeps a line, for [`next_change`]; `None` when lines stay.
pub fn line_life(s: &InputLogSettings) -> Option<Duration> {
    fade_after(s)
}

/// Every controller's recent input, by its number, as info overlays read it.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub logs: Vec<(u8, Vec<Entry>)>,
    /// The moment to show them as of; now when unset.
    pub now: Option<Instant>,
}

impl Inputs {
    /// One controller's entries, or every controller's merged.
    pub fn entries(&self, device: Option<u8>) -> Vec<Entry> {
        match device {
            Some(n) => self.logs.iter().find(|(d, _)| *d == n).map(|(_, e)| e.clone()).unwrap_or_default(),
            None => merged(self.logs.iter().map(|(_, e)| e.as_slice())),
        }
    }

    /// What `{current_input}` shows.
    pub fn current(&self, device: Option<u8>, gap_ms: u64, stay_ms: u64, family: PadFamily) -> Vec<Segment> {
        let now = self.now.unwrap_or_else(Instant::now);
        current_input(&self.entries(device), Duration::from_millis(gap_ms), Duration::from_millis(stay_ms), family, now)
    }

    /// Made-up input for the settings previews: a combo, a mashed button, then a fireball
    /// motion whose last button is still held.
    pub fn sample() -> Self {
        use super::Dir;
        use crate::config::Button;
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let entry = |id: u64, input: Input, from: u64, to: Option<u64>, label: Option<&str>| Entry {
            input,
            pressed: at(from),
            released: to.map(at),
            label: label.map(str::to_string),
            resolved: true,
            id,
        };
        let mut entries = vec![entry(0, Input::Combo(vec![Button::LeftBumper, Button::West]), 0, Some(180), Some("Heal"))];
        for (n, start) in [600, 700, 800, 900].into_iter().enumerate() {
            entries.push(entry(1 + n as u64, Input::Button(Button::South), start, Some(start + 50 + 10 * n as u64), Some("Space")));
        }
        entries.extend([
            entry(5, Input::Direction(None, Dir::Down), 1400, Some(1440), None),
            entry(6, Input::Direction(None, Dir::DownRight), 1440, Some(1480), None),
            entry(7, Input::Direction(None, Dir::Right), 1480, Some(1520), None),
            entry(8, Input::Button(Button::East), 1510, None, Some("Macro “Fireball”")),
        ]);
        Inputs { logs: vec![(0, entries)], now: Some(at(1600)) }
    }
}

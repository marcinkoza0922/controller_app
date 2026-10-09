//! How overlays move. Each kind of overlay (the keyboard, the numpad, menus, media controls,
//! offers, info overlays and input logs) has a motion style of its own, and a game can set its
//! own. A style says how fast things open and close, how the highlight glides from one item to
//! the next, and what a pick looks like. The overlay process samples it with [`Anim`] while it
//! draws, and keeps the timing of what is on screen with [`Presence`].

use std::{fmt, time::Instant};

use serde::{Deserialize, Deserializer, Serialize};

/// The overlays a motion style can be set for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayKind {
    Keyboard,
    Numpad,
    Menu,
    Media,
    Offer,
    Info,
    Log,
}

impl OverlayKind {
    pub const ALL: [OverlayKind; 7] = [
        OverlayKind::Keyboard,
        OverlayKind::Numpad,
        OverlayKind::Menu,
        OverlayKind::Media,
        OverlayKind::Offer,
        OverlayKind::Info,
        OverlayKind::Log,
    ];

    /// What the settings call this overlay.
    pub fn label(self) -> &'static str {
        match self {
            OverlayKind::Keyboard => "On-screen keyboard",
            OverlayKind::Numpad => "Numpad",
            OverlayKind::Menu => "Menus",
            OverlayKind::Media => "Media controls",
            OverlayKind::Offer => "Library offers",
            OverlayKind::Info => "Info overlays",
            OverlayKind::Log => "Input logs",
        }
    }
}

/// How an overlay moves. Subtle is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionStyle {
    /// Nothing moves.
    Off,
    /// A quick fade, and the highlight gliding from item to item.
    #[default]
    Subtle,
    /// Panels drop in with an overshoot, and the highlight bounces past its place. Cheerful games.
    Playful,
    /// Items fade in one after another, and the highlight glides between them.
    Stagger,
    /// Slow fades that rise into place, and a dim, heavy pulse on a pick. Dark fantasy.
    Grim,
    /// Snaps in and out, the highlight jumps, and a pick jolts and flashes. Shooters and fights.
    Brutal,
}

impl MotionStyle {
    pub const ALL: [MotionStyle; 6] = [
        MotionStyle::Off,
        MotionStyle::Subtle,
        MotionStyle::Playful,
        MotionStyle::Stagger,
        MotionStyle::Grim,
        MotionStyle::Brutal,
    ];

    /// What the settings say about this style.
    pub fn describe(self) -> &'static str {
        match self {
            MotionStyle::Off => "Nothing moves.",
            MotionStyle::Subtle => "A quick fade, and the highlight glides from item to item.",
            MotionStyle::Playful => "Panels drop in with a bounce, and the highlight overshoots its item. Suits cheerful games.",
            MotionStyle::Stagger => "Items fade in one after another, and the highlight glides between them.",
            MotionStyle::Grim => "Slow fades that rise into place, and a dim, heavy pulse on a pick. Suits dark fantasy.",
            MotionStyle::Brutal => "Snaps in and out, the highlight jumps, and a pick jolts and flashes. Suits shooters and fights.",
        }
    }

    /// The timing and shape of this style.
    pub fn feel(self) -> Feel {
        let none = Feel::OFF;
        match self {
            MotionStyle::Off => none,
            MotionStyle::Subtle => Feel { open: 0.18, close: 0.12, glide: 0.12, pick: 0.18, flash: 0.4, ..none },
            MotionStyle::Playful => Feel {
                open: 0.32,
                close: 0.2,
                glide: 0.25,
                pick: 0.35,
                slide: -18.0,
                flash: 0.5,
                curve: Curve::Back,
                ..none
            },
            MotionStyle::Stagger => Feel { open: 0.06, close: 0.12, glide: 0.12, pick: 0.18, item: 0.22, stagger: 0.045, flash: 0.4, ..none },
            MotionStyle::Grim => Feel {
                open: 0.6,
                close: 0.5,
                glide: 0.32,
                pick: 0.7,
                slide: 26.0,
                flash: 0.35,
                curve: Curve::InOut,
                ..none
            },
            MotionStyle::Brutal => Feel { open: 0.07, close: 0.06, glide: 0.0, pick: 0.16, flash: 1.0, jolt: 8.0, curve: Curve::Linear, ..none },
        }
    }
}

impl fmt::Display for MotionStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MotionStyle::Off => "Off",
            MotionStyle::Subtle => "Subtle",
            MotionStyle::Playful => "Playful",
            MotionStyle::Stagger => "Stagger",
            MotionStyle::Grim => "Grim",
            MotionStyle::Brutal => "Brutal",
        })
    }
}

/// The motion style of every kind of overlay. A game's own set replaces the global one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionSet {
    pub keyboard: MotionStyle,
    pub numpad: MotionStyle,
    pub menu: MotionStyle,
    pub media: MotionStyle,
    pub offer: MotionStyle,
    pub info: MotionStyle,
    pub log: MotionStyle,
}

impl MotionSet {
    /// One style for every kind of overlay.
    pub fn all(style: MotionStyle) -> Self {
        MotionSet { keyboard: style, numpad: style, menu: style, media: style, offer: style, info: style, log: style }
    }

    pub fn get(&self, kind: OverlayKind) -> MotionStyle {
        match kind {
            OverlayKind::Keyboard => self.keyboard,
            OverlayKind::Numpad => self.numpad,
            OverlayKind::Menu => self.menu,
            OverlayKind::Media => self.media,
            OverlayKind::Offer => self.offer,
            OverlayKind::Info => self.info,
            OverlayKind::Log => self.log,
        }
    }

    pub fn set(&mut self, kind: OverlayKind, style: MotionStyle) {
        let slot = match kind {
            OverlayKind::Keyboard => &mut self.keyboard,
            OverlayKind::Numpad => &mut self.numpad,
            OverlayKind::Menu => &mut self.menu,
            OverlayKind::Media => &mut self.media,
            OverlayKind::Offer => &mut self.offer,
            OverlayKind::Info => &mut self.info,
            OverlayKind::Log => &mut self.log,
        };
        *slot = style;
    }

    pub fn feel(&self, kind: OverlayKind) -> Feel {
        self.get(kind).feel()
    }

    pub fn is_default(&self) -> bool {
        *self == MotionSet::default()
    }
}

/// Reads a set either as the table it is written as, or as one style for every kind of overlay:
/// the single setting that earlier versions kept, for menus only.
pub fn deserialize_set<'de, D: Deserializer<'de>>(de: D) -> Result<MotionSet, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        Style(MotionStyle),
        Set(MotionSet),
    }
    Ok(match Either::deserialize(de)? {
        Either::Style(style) => MotionSet::all(style),
        Either::Set(set) => set,
    })
}

/// How an easing curve shapes progress from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Curve {
    #[default]
    Linear,
    /// Starts fast and slows into place.
    Out,
    /// Slow at both ends.
    InOut,
    /// Overshoots its end a little before settling.
    Back,
}

impl Curve {
    fn apply(self, t: f32) -> f32 {
        match self {
            Curve::Linear => t,
            Curve::Out => 1.0 - (1.0 - t).powi(3),
            Curve::InOut => t * t * (3.0 - 2.0 * t),
            Curve::Back => {
                let x = t - 1.0;
                1.0 + 2.701_58 * x * x * x + 1.701_58 * x * x
            }
        }
    }
}

/// The timing and shape of one motion style, in seconds and pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Feel {
    /// How long a panel takes to open, and to close.
    pub open: f32,
    pub close: f32,
    /// How long the highlight takes to glide to the next item, and a pick to settle.
    pub glide: f32,
    pub pick: f32,
    /// How long each item takes to fade in, and how far apart those fades start (Stagger).
    pub item: f32,
    pub stagger: f32,
    /// Pixels a panel starts below its place (positive) or above it (negative) on opening, and
    /// sinks by as it closes.
    pub slide: f32,
    /// How strongly a pick flashes the item it lands on, 0 to 1.
    pub flash: f32,
    /// Pixels a pick jolts the panel sideways.
    pub jolt: f32,
    pub curve: Curve,
}

impl Feel {
    /// No motion at all: everything is already where it ends up.
    pub const OFF: Feel = Feel {
        open: 0.0,
        close: 0.0,
        glide: 0.0,
        pick: 0.0,
        item: 0.0,
        stagger: 0.0,
        slide: 0.0,
        flash: 0.0,
        jolt: 0.0,
        curve: Curve::Linear,
    };
}

impl Default for Feel {
    fn default() -> Self {
        Feel::OFF
    }
}

/// Stagger staggers at most this many items; later ones wait no longer than this.
const STAGGER_ITEMS: f32 = 12.0;
/// Seconds since something happened, when it never did.
const NEVER: f32 = f32::INFINITY;

/// How far along `curve` an event `elapsed` seconds ago is, when it takes `secs`.
fn progress(elapsed: f32, secs: f32, curve: Curve) -> f32 {
    if secs <= 0.0 {
        return 1.0;
    }
    curve.apply((elapsed / secs).clamp(0.0, 1.0))
}

/// When each thing happened to an overlay, in seconds ago.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    pub opened: f32,
    /// `None` while it is still shown.
    pub closed: Option<f32>,
    /// When the cursor last moved.
    pub moved: f32,
    /// Where the cursor was before its last move.
    pub from: Option<u32>,
    pub picked: f32,
}

impl Default for Timing {
    fn default() -> Self {
        Timing { opened: NEVER, closed: None, moved: NEVER, from: None, picked: NEVER }
    }
}

/// An overlay's feel and timing, as the overlay process draws it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Anim {
    pub feel: Feel,
    pub timing: Timing,
}

impl Anim {
    /// Drawn as it ends up: no motion.
    pub fn still() -> Self {
        Anim::default()
    }

    /// How visible the whole overlay is, 0 to 1: fading in as it opens, and out as it closes.
    pub fn opacity(&self) -> f32 {
        let open = progress(self.timing.opened, self.feel.open, self.feel.curve).clamp(0.0, 1.0);
        let closing = self.timing.closed.map_or(1.0, |s| 1.0 - progress(s, self.feel.close, self.feel.curve).clamp(0.0, 1.0));
        open * closing
    }

    /// Where the overlay is pushed off its place, in pixels (x right, y down).
    pub fn offset(&self) -> (f32, f32) {
        let open = progress(self.timing.opened, self.feel.open, self.feel.curve);
        let mut y = (1.0 - open) * self.feel.slide;
        if let Some(s) = self.timing.closed {
            y += progress(s, self.feel.close, self.feel.curve) * self.feel.slide;
        }
        let p = self.pick_progress();
        let x = if p < 1.0 { self.feel.jolt * (1.0 - p) * (p * std::f32::consts::TAU * 1.5).sin() } else { 0.0 };
        (x, y)
    }

    /// How lit an item is, 0 to 1 (it can overshoot under Playful): the cursor's item glides
    /// in, and the one it came from glides out.
    pub fn lit(&self, selected: bool, from: bool) -> f32 {
        let glided = progress(self.timing.moved, self.feel.glide, self.feel.curve);
        if selected {
            glided
        } else if from {
            1.0 - glided.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// How much a pick still flashes the item it landed on, 0 to 1.
    pub fn flash(&self) -> f32 {
        let remaining = 1.0 - self.pick_progress();
        self.feel.flash * remaining * remaining
    }

    /// How visible item `index` is, 0 to 1: all at once, or one after another under Stagger.
    pub fn item_fade(&self, index: usize) -> f32 {
        if self.feel.stagger <= 0.0 {
            return 1.0;
        }
        let since = self.timing.opened - index as f32 * self.feel.stagger;
        progress(since, self.feel.item, Curve::Out).clamp(0.0, 1.0)
    }

    /// Whether this still changes from one frame to the next, so the screen needs redrawing.
    pub fn is_busy(&self) -> bool {
        let (t, f) = (&self.timing, &self.feel);
        // A closed overlay is kept until its close has played, then dropped.
        t.closed.is_some()
            || t.opened < f.open.max(f.item + f.stagger * STAGGER_ITEMS)
            || t.moved < f.glide
            || t.picked < f.pick
    }

    fn pick_progress(&self) -> f32 {
        progress(self.timing.picked, self.feel.pick, Curve::Out).clamp(0.0, 1.0)
    }
}

/// Something an overlay shows, so its timing can be kept from one frame to the next.
pub trait Tracked: Clone {
    /// Tells the things of one kind apart: a thing that keeps its key keeps its timing.
    fn key(&self) -> String;
    /// Which motion style it moves with.
    fn kind(&self) -> OverlayKind;
    /// Where the cursor is, if it has one: when that changes the highlight glides from the old place.
    fn cursor(&self) -> Option<u32> {
        None
    }
    /// Changes when something is picked in it.
    fn pick(&self) -> Option<u32> {
        None
    }
}

/// One thing on screen, or one that has just gone and is still fading out.
#[derive(Debug, Clone)]
pub struct Entry<T> {
    key: String,
    pub value: T,
    opened: Instant,
    closed: Option<Instant>,
    moved: Instant,
    from: Option<u32>,
    cursor: Option<u32>,
    picked: Option<Instant>,
    pick: Option<u32>,
}

impl<T: Tracked> Entry<T> {
    fn new(now: Instant, key: String, value: T) -> Self {
        Entry {
            key,
            cursor: value.cursor(),
            pick: value.pick(),
            value,
            opened: now,
            closed: None,
            moved: now,
            from: None,
            picked: None,
        }
    }

    fn refresh(&mut self, now: Instant, value: T) {
        let cursor = value.cursor();
        if cursor != self.cursor {
            self.from = self.cursor;
            self.moved = now;
            self.cursor = cursor;
        }
        let pick = value.pick();
        if pick != self.pick {
            if pick.is_some() {
                self.picked = Some(now);
            }
            self.pick = pick;
        }
        self.value = value;
    }

    fn timing(&self, now: Instant) -> Timing {
        let since = |t: Instant| now.saturating_duration_since(t).as_secs_f32();
        Timing {
            opened: since(self.opened),
            closed: self.closed.map(since),
            moved: since(self.moved),
            from: self.from,
            picked: self.picked.map_or(NEVER, since),
        }
    }
}

/// What is on screen, with when each thing opened, moved and was picked. A thing that goes is
/// kept, shown fading out, until its kind's close has played.
#[derive(Debug, Clone)]
pub struct Presence<T> {
    entries: Vec<Entry<T>>,
    /// Whether only one thing is shown at a time (the panels): a new one then replaces the old at
    /// once, rather than the two fading over each other.
    single: bool,
}

impl<T> Default for Presence<T> {
    fn default() -> Self {
        Presence { entries: Vec::new(), single: false }
    }
}

impl<T: Tracked> Presence<T> {
    /// For things where one at a time is shown.
    pub fn single() -> Self {
        Presence { entries: Vec::new(), single: true }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Takes in what is shown now: new things open, things no longer shown start to close, and
    /// the rest take their new values.
    pub fn update(&mut self, now: Instant, shown: Vec<T>) {
        let mut matched = vec![false; self.entries.len()];
        let mut opened = false;
        for value in shown {
            let key = value.key();
            let found = (0..self.entries.len()).find(|&i| !matched[i] && self.entries[i].closed.is_none() && self.entries[i].key == key);
            match found {
                Some(i) => {
                    matched[i] = true;
                    self.entries[i].refresh(now, value);
                }
                None => {
                    matched.push(true);
                    opened = true;
                    self.entries.push(Entry::new(now, key, value));
                }
            }
        }
        for (entry, matched) in self.entries.iter_mut().zip(matched) {
            if !matched && entry.closed.is_none() {
                entry.closed = Some(now);
            }
        }
        if self.single && opened {
            self.entries.retain(|e| e.closed != Some(now));
        }
    }

    /// Drops the things that have finished closing.
    pub fn prune(&mut self, now: Instant, motion: &MotionSet) {
        self.entries.retain(|e| {
            e.closed.is_none_or(|closed| now.saturating_duration_since(closed).as_secs_f32() < motion.feel(e.value.kind()).close)
        });
    }

    /// What to draw now, each with its animation, oldest first so newer things draw on top.
    pub fn shown<'a>(&'a self, now: Instant, motion: &MotionSet) -> impl Iterator<Item = (&'a T, Anim)> + 'a {
        let motion = *motion;
        self.entries.iter().map(move |e| (&e.value, Anim { feel: motion.feel(e.value.kind()), timing: e.timing(now) }))
    }

    /// Whether anything is still moving, so the screen needs redrawing.
    pub fn busy(&self, now: Instant, motion: &MotionSet) -> bool {
        self.shown(now, motion).any(|(_, anim)| anim.is_busy())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// A stand-in for an overlay: a name, a kind, a cursor and a pick count.
    #[derive(Debug, Clone, PartialEq)]
    struct Thing {
        name: &'static str,
        cursor: Option<u32>,
        picks: u32,
    }

    impl Tracked for Thing {
        fn key(&self) -> String {
            self.name.into()
        }
        fn kind(&self) -> OverlayKind {
            OverlayKind::Menu
        }
        fn cursor(&self) -> Option<u32> {
            self.cursor
        }
        fn pick(&self) -> Option<u32> {
            (self.picks > 0).then_some(self.picks)
        }
    }

    fn thing(name: &'static str, cursor: Option<u32>) -> Thing {
        Thing { name, cursor, picks: 0 }
    }

    fn only(motion: MotionStyle) -> MotionSet {
        MotionSet { menu: motion, ..MotionSet::default() }
    }

    #[test]
    fn subtle_is_the_default_for_every_kind() {
        assert_eq!(MotionSet::default().get(OverlayKind::Keyboard), MotionStyle::Subtle);
        assert!(MotionSet::default().is_default());
        assert!(!only(MotionStyle::Grim).is_default());
    }

    #[test]
    fn an_earlier_single_style_applies_to_every_kind() {
        #[derive(Deserialize)]
        struct Config {
            #[serde(default, deserialize_with = "deserialize_set")]
            motion: MotionSet,
        }
        let old: Config = toml::from_str("motion = \"playful\"").unwrap();
        assert_eq!(old.motion, MotionSet::all(MotionStyle::Playful));
        let table: Config = toml::from_str("[motion]\nmenu = \"grim\"").unwrap();
        assert_eq!(table.motion.get(OverlayKind::Menu), MotionStyle::Grim);
        assert_eq!(table.motion.get(OverlayKind::Keyboard), MotionStyle::Subtle, "unset kinds are subtle");
        let missing: Config = toml::from_str("").unwrap();
        assert!(missing.motion.is_default());
    }

    #[test]
    fn a_set_changes_only_its_kind() {
        let mut set = MotionSet::default();
        set.set(OverlayKind::Info, MotionStyle::Brutal);
        assert_eq!(set.get(OverlayKind::Info), MotionStyle::Brutal);
        assert_eq!(set.get(OverlayKind::Menu), MotionStyle::Subtle);
    }

    #[test]
    fn off_has_no_motion_so_nothing_is_kept_after_closing() {
        let t0 = Instant::now();
        let motion = only(MotionStyle::Off);
        let mut shown = Presence::default();
        shown.update(t0, vec![thing("a", Some(0))]);
        shown.update(t0, Vec::new());
        shown.prune(t0, &motion);
        assert!(shown.is_empty());
    }

    #[test]
    fn a_closed_thing_is_kept_until_its_close_has_played() {
        let t0 = Instant::now();
        let motion = only(MotionStyle::Subtle);
        let mut shown = Presence::default();
        shown.update(t0, vec![thing("a", Some(0))]);
        shown.update(t0 + Duration::from_millis(500), Vec::new());
        shown.prune(t0 + Duration::from_millis(600), &motion);
        assert!(!shown.is_empty(), "still fading out 100ms after going");
        assert!(shown.busy(t0 + Duration::from_millis(600), &motion));
        shown.prune(t0 + Duration::from_millis(700), &motion);
        assert!(shown.is_empty());
    }

    #[test]
    fn a_thing_that_stays_keeps_when_it_opened() {
        let t0 = Instant::now();
        let mut shown = Presence::default();
        shown.update(t0, vec![thing("belt", Some(0))]);
        let later = t0 + Duration::from_secs(1);
        shown.update(later, vec![thing("belt", Some(0))]);
        let motion = only(MotionStyle::Subtle);
        let (_, anim) = shown.shown(later, &motion).next().unwrap();
        assert!((anim.timing.opened - 1.0).abs() < 1e-3, "{:?}", anim.timing);
    }

    #[test]
    fn moving_the_cursor_glides_from_the_item_it_left() {
        let t0 = Instant::now();
        let mut shown = Presence::default();
        shown.update(t0, vec![thing("belt", Some(2))]);
        let t1 = t0 + Duration::from_millis(500);
        shown.update(t1, vec![thing("belt", Some(3))]);
        let motion = only(MotionStyle::Subtle);
        let (_, anim) = shown.shown(t1, &motion).next().unwrap();
        assert_eq!(anim.timing.from, Some(2));
        assert!(anim.timing.moved.abs() < 1e-3);
        let t2 = t1 + Duration::from_millis(50);
        shown.update(t2, vec![thing("belt", Some(3))]);
        let (_, still) = shown.shown(t2, &motion).next().unwrap();
        assert!((still.timing.moved - 0.05).abs() < 1e-3, "no move, no new glide");
    }

    #[test]
    fn a_pick_is_seen_when_the_pick_count_changes() {
        let t0 = Instant::now();
        let mut shown = Presence::default();
        shown.update(t0, vec![thing("belt", Some(0))]);
        let t1 = t0 + Duration::from_millis(300);
        shown.update(t1, vec![Thing { picks: 1, ..thing("belt", Some(0)) }]);
        let motion = only(MotionStyle::Brutal);
        let (_, anim) = shown.shown(t1, &motion).next().unwrap();
        assert!(anim.timing.picked.abs() < 1e-3);
        assert!(anim.flash() > 0.9, "a fresh pick flashes at full strength");
    }

    #[test]
    fn a_new_thing_under_the_same_key_after_closing_opens_again() {
        let t0 = Instant::now();
        let mut shown = Presence::default();
        shown.update(t0, vec![thing("belt", Some(0))]);
        shown.update(t0, Vec::new());
        let t1 = t0 + Duration::from_secs(5);
        shown.update(t1, vec![thing("belt", Some(0))]);
        let motion = only(MotionStyle::Subtle);
        assert_eq!(shown.shown(t1, &motion).count(), 2, "the old one fades beside the new one");
    }

    #[test]
    fn a_single_presence_replaces_its_thing_at_once() {
        let t0 = Instant::now();
        let mut panel = Presence::single();
        panel.update(t0, vec![thing("belt", Some(0))]);
        panel.update(t0, vec![thing("weapons", Some(0))]);
        assert_eq!(panel.shown(t0, &only(MotionStyle::Grim)).count(), 1, "the old menu is cut, not faded over");
        let mut infos = Presence::default();
        infos.update(t0, vec![thing("a", None)]);
        infos.update(t0, vec![thing("b", None)]);
        assert_eq!(infos.shown(t0, &only(MotionStyle::Grim)).count(), 2, "independent overlays still fade out");
    }

    #[test]
    fn repeated_frames_do_not_pile_up_entries() {
        let t0 = Instant::now();
        let mut shown = Presence::default();
        for _ in 0..5 {
            shown.update(t0, vec![thing("a", None), thing("a", None)]);
        }
        assert_eq!(shown.shown(t0, &MotionSet::default()).count(), 2);
    }

    #[test]
    fn open_and_close_fade_the_opacity() {
        let feel = MotionStyle::Subtle.feel();
        let anim = Anim { feel, timing: Timing { opened: 0.0, ..Timing::default() } };
        assert!(anim.opacity() < 0.01, "starts invisible");
        let open = Anim { feel, timing: Timing { opened: 1.0, ..Timing::default() } };
        assert!((open.opacity() - 1.0).abs() < 1e-6);
        let closing = Anim { feel, timing: Timing { opened: 1.0, closed: Some(feel.close), ..Timing::default() } };
        assert!(closing.opacity() < 0.01, "gone once closed");
    }

    #[test]
    fn brutal_highlight_jumps_and_grim_rises_into_place() {
        let jump = Anim { feel: MotionStyle::Brutal.feel(), timing: Timing { moved: 0.0, ..Timing::default() } };
        assert!((jump.lit(true, false) - 1.0).abs() < 1e-6, "no glide: lit at once");
        let rising = Anim { feel: MotionStyle::Grim.feel(), timing: Timing { opened: 0.0, ..Timing::default() } };
        let (_, y) = rising.offset();
        assert!(y > 20.0, "starts well below its place");
        let settled = Anim { feel: MotionStyle::Grim.feel(), timing: Timing { opened: 10.0, ..Timing::default() } };
        assert_eq!(settled.offset(), (0.0, 0.0));
    }

    #[test]
    fn playful_drops_in_from_above() {
        let drop = Anim { feel: MotionStyle::Playful.feel(), timing: Timing { opened: 0.0, ..Timing::default() } };
        assert!(drop.offset().1 < 0.0, "starts above its place");
    }

    #[test]
    fn stagger_fades_items_one_after_another() {
        let anim = Anim { feel: MotionStyle::Stagger.feel(), timing: Timing { opened: 0.1, ..Timing::default() } };
        assert!(anim.item_fade(0) > anim.item_fade(3));
        assert_eq!(Anim::still().item_fade(5), 1.0);
    }

    #[test]
    fn off_draws_as_it_ends_up() {
        let anim = Anim { feel: MotionStyle::Off.feel(), timing: Timing { opened: 0.0, moved: 0.0, ..Timing::default() } };
        assert_eq!(anim.opacity(), 1.0);
        assert_eq!(anim.offset(), (0.0, 0.0));
        assert_eq!(anim.lit(true, false), 1.0);
        assert_eq!(anim.lit(false, true), 0.0);
        assert!(!anim.is_busy());
    }
}

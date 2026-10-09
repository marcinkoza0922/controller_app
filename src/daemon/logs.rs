//! The input log in the daemon: each controller's raw input recorded, the actions it fired
//! attached, and the result shown by log overlays and `{current_input}` cells.

use std::{
    collections::HashSet,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

use super::{Daemon, FADE_FRAME, Managed, layered};
use crate::{
    config::{InfoOverlay, InputLogSettings, LogOverlay, LogSource, OverlayStyle, Profile, Stick, Trigger},
    input::InputEvent,
    inputlog::{self, Entry, FiredFrom, Inputs, LogView, Thresholds},
    overlay::OverlayAction,
};

/// The GUI's feed shows the log's default line count, one line per burst of presses, with no
/// fading, so every press stays until it scrolls out.
fn feed_settings() -> InputLogSettings {
    InputLogSettings { merge_repeats: false, fade_after: 0.0, ..InputLogSettings::default() }
}

impl Managed {
    /// How the log reads this controller's sticks and triggers: as its mappings do.
    fn thresholds(&self, base: &Profile) -> Thresholds {
        let p = layered(&self.layered, base);
        Thresholds::new([p.stick(Stick::Left), p.stick(Stick::Right)], [p.trigger(Trigger::Left), p.trigger(Trigger::Right)])
    }
}

/// The lowest controller number none of `taken` has, for a controller connecting now.
pub(super) fn free_number(taken: impl Iterator<Item = u8>) -> u8 {
    let taken: HashSet<u8> = taken.collect();
    (0..u8::MAX).find(|n| !taken.contains(n)).unwrap_or(u8::MAX)
}

impl Daemon {
    /// Records controller `id`'s raw input. Returns whether its log changed.
    pub(super) fn log_input(&mut self, id: u64, events: &[InputEvent], now: Instant) -> bool {
        let Some(dev) = self.devices.get_mut(&id) else { return false };
        let t = self.config.active().map(|base| dev.thresholds(base)).unwrap_or_default();
        events.iter().fold(false, |changed, ev| dev.log.event(*ev, t, now) | changed)
    }

    /// Attaches the actions controller `id`'s mappings fired to its log. Returns whether
    /// there were any.
    pub(super) fn log_fired(&mut self, id: u64) -> bool {
        let Some(dev) = self.devices.get_mut(&id) else { return false };
        let fired = dev.engine.take_fired();
        for (from, action) in &fired {
            dev.log.attach(from, inputlog::label_for(from, action));
        }
        !fired.is_empty()
    }

    /// Labels the press that just chose a menu item.
    pub(super) fn log_menu_choice(&mut self, id: u64, ev: InputEvent, action: &crate::config::ButtonAction) {
        if let InputEvent::Button(b, true) = ev
            && let Some(dev) = self.devices.get_mut(&id)
        {
            let from = FiredFrom::Button(b);
            dev.log.attach(&from, inputlog::label_for(&from, action));
        }
    }

    /// Labels the press that just typed on the on-screen keyboard with the keys it sent.
    pub(super) fn log_keyboard(&mut self, id: u64, ev: InputEvent, actions: &[OverlayAction]) {
        let keys: Vec<String> = actions
            .iter()
            .filter_map(|a| match a {
                OverlayAction::Key(code, true) => Some(crate::keyboard::label(&format!("{code:?}"))),
                _ => None,
            })
            .collect();
        if let InputEvent::Button(b, true) = ev
            && !keys.is_empty()
            && let Some(dev) = self.devices.get_mut(&id)
        {
            dev.log.attach(&FiredFrom::Button(b), Some(keys.join(" + ")));
        }
    }

    /// Redraws the overlay if the log changed while something shows it, and sends the GUI's
    /// feed its update.
    pub(super) fn log_changed(&mut self) {
        if self.shows_input {
            self.broadcast_overlay();
        }
        self.broadcast_feed();
    }

    /// The latest presses of the controller in use, drawn as the log overlays draw them.
    pub(super) fn feed_view(&self, now: Instant) -> Option<LogView> {
        let dev = self.devices.get(&self.last_active?)?;
        let view = inputlog::log_view(dev.log.entries(), &feed_settings(), &OverlayStyle::default(), dev.family.unwrap_or(self.config.info_glyphs), now);
        Some(LogView { opacity: 1.0, ..view })
    }

    /// Adds a watcher of the feed, sending it the current one first.
    pub(super) fn watch_feed(&mut self, watcher: Sender<Option<LogView>>) {
        if watcher.send(self.feed_view(Instant::now())).is_ok() {
            self.feed_watchers.push(watcher);
        }
    }

    /// Streams the feed to its watchers, if there are any.
    fn broadcast_feed(&mut self) {
        if self.feed_watchers.is_empty() {
            return;
        }
        let feed = self.feed_view(Instant::now());
        self.feed_watchers.retain(|w| w.send(feed.clone()).is_ok());
    }

    /// One controller's entries by its number, or every controller's merged.
    fn entries(&self, device: Option<u8>) -> Vec<Entry> {
        match device {
            Some(n) => self.devices.values().find(|d| d.number == n).map(|d| d.log.entries().to_vec()).unwrap_or_default(),
            None => inputlog::merged(self.devices.values().map(|d| d.log.entries())),
        }
    }

    /// Every controller's log, for `{current_input}` cells.
    pub(super) fn inputs(&self, now: Instant) -> Inputs {
        Inputs { logs: self.devices.values().map(|d| (d.number, d.log.entries().to_vec())).collect(), now: Some(now) }
    }

    /// Log overlays to draw now, and how visible each is: steady ones fully, lingering ones
    /// fading out.
    fn visible_logs(&mut self, now: Instant) -> Vec<(LogOverlay, f32)> {
        let held: HashSet<String> = self.devices.values().flat_map(|d| d.engine.shown_logs()).cloned().collect();
        let logs = &self.scope.logs;
        self.log_timers.update_with(held, |name| logs.iter().find(|o| o.name == name).and_then(|o| o.linger), now);
        logs.iter()
            .filter_map(|o| {
                let steady = o.always || self.log_timers.held(&o.name);
                let opacity = if steady { 1.0 } else { self.log_timers.opacity(&o.name, now) };
                (opacity > 0.0).then(|| (o.clone(), opacity))
            })
            .collect()
    }

    /// The log overlays' views (those with lines to show), noting when they next change on
    /// their own. Also returns whether any log overlay is up, even one with nothing in it yet.
    pub(super) fn log_frame(&mut self, now: Instant, family: crate::info::PadFamily) -> (Vec<LogView>, bool) {
        let shown = self.visible_logs(now);
        let mut redraw = self.log_timers.next_redraw(now, FADE_FRAME);
        let views = shown
            .iter()
            .map(|(o, opacity)| {
                let LogSource::Input(s) = &o.source;
                let entries = self.entries(s.tracking.device);
                if let Some(life) = inputlog::line_life(s) {
                    let due = inputlog::next_change(&entries, &s.tracking, life, now, FADE_FRAME);
                    redraw = redraw.into_iter().chain(due).min();
                }
                LogView { opacity: *opacity, ..inputlog::log_view(&entries, s, &o.style, family, now) }
            })
            .filter(|v| !v.lines.is_empty())
            .collect();
        self.log_redraw = redraw;
        (views, !shown.is_empty())
    }

    /// When the `{current_input}` cells of these overlays next change on their own.
    pub(super) fn token_redraw(&self, shown: &[&InfoOverlay], inputs: &Inputs, now: Instant) -> Option<Instant> {
        shown
            .iter()
            .flat_map(|o| crate::info::input_tokens(o))
            .filter_map(|c| {
                let after = Duration::from_millis(c.tracking.gap_ms + c.stay_ms);
                inputlog::next_change(&inputs.entries(c.tracking.device), &c.tracking, after, now, FADE_FRAME)
            })
            .min()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_new_controller_takes_the_lowest_free_number() {
        assert_eq!(super::free_number([].into_iter()), 0);
        assert_eq!(super::free_number([0, 2].into_iter()), 1);
        assert_eq!(super::free_number([1, 0].into_iter()), 2);
    }
}

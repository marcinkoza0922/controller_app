//! What the daemon did lately, for the GUI to explain: each profile switch, and why it was made.

use std::{collections::VecDeque, time::Instant};

use crate::{config::ProfileRef, ipc::SwitchEvent};

/// Switches kept; the GUI shows far fewer.
const SWITCHES: usize = 20;

#[derive(Debug, Default)]
pub struct Activity {
    /// Newest first.
    switches: VecDeque<(Instant, ProfileRef, String)>,
}

impl Activity {
    /// Records that the profile changed to `to` at `at`, for `why`.
    pub fn switched(&mut self, at: Instant, to: &ProfileRef, why: &str) {
        self.switches.push_front((at, to.clone(), why.to_owned()));
        self.switches.truncate(SWITCHES);
    }

    /// The recorded switches as of `now`, newest first.
    pub fn switches(&self, now: Instant) -> Vec<SwitchEvent> {
        self.switches
            .iter()
            .map(|(at, to, why)| SwitchEvent {
                age_ms: u64::try_from(now.saturating_duration_since(*at).as_millis()).unwrap_or(u64::MAX),
                game: to.game.clone(),
                profile: to.profile.clone(),
                why: why.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn at(game: &str, profile: &str) -> ProfileRef {
        ProfileRef::new(Some(game), profile)
    }

    #[test]
    fn switches_are_newest_first_with_their_age_and_reason() {
        let mut activity = Activity::default();
        let start = Instant::now();
        activity.switched(start, &at("Doom", "Play"), "Doom matches Executable “doom.exe”");
        activity.switched(start + Duration::from_secs(5), &ProfileRef::new(None, "Gamepad"), "Doom lost focus");

        let now = start + Duration::from_secs(7);
        let switches = activity.switches(now);
        assert_eq!(switches.len(), 2);
        assert_eq!((switches[0].game.as_deref(), switches[0].profile.as_str()), (None, "Gamepad"));
        assert_eq!(switches[0].age_ms, 2_000);
        assert_eq!(switches[1].why, "Doom matches Executable “doom.exe”");
        assert_eq!(switches[1].age_ms, 7_000);
    }

    #[test]
    fn only_the_newest_switches_are_kept() {
        let mut activity = Activity::default();
        let start = Instant::now();
        for i in 0..SWITCHES + 5 {
            activity.switched(start, &at("Doom", &format!("P{i}")), "why");
        }
        let switches = activity.switches(start);
        assert_eq!(switches.len(), SWITCHES);
        assert_eq!(switches[0].profile, format!("P{}", SWITCHES + 4));
    }
}

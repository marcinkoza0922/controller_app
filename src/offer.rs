//! Offers a library game's controller profiles: when Guide is tapped over a game that the
//! library knows but the config doesn't, a panel asks whether to add it (and which pack, when
//! more than one fits). `OfferSession` turns controller input into an answer; the overlay
//! process draws `OfferView`.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{
    config::{Button, Config, Feature, OverlayStyle, ScreenPosition},
    focus,
    input::InputEvent,
    ipc::WindowInfo,
    library::Entry,
};

/// An unanswered offer goes away by itself, so it can't hold the controller forever.
const IDLE_CLOSE: Duration = Duration::from_secs(30);

/// Library packs for the window in front, if it is a game the config doesn't have: no game
/// rule matches it, none of the packs is installed or turned down for good, and at least one
/// of its profiles works with the controllers' features `have` (`None`: no controller to
/// judge by). Indexes into `library`.
pub fn candidates(config: &Config, library: &[Entry], window: &WindowInfo, have: Option<&[Feature]>) -> Vec<usize> {
    if focus::is_own_window(window) || focus::profile_for(config, window).is_some() {
        return Vec::new();
    }
    let seen = std::slice::from_ref(window);
    library
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            let id = &e.pack.pack.id;
            !config.declined_packs.contains(id)
                && !config.games.iter().any(|g| g.origin.as_ref().is_some_and(|o| &o.id == id))
                && e.pack.rules.iter().any(|r| focus::rule_matches_any(r, seen))
                && e.pack.profiles.iter().any(|p| have.is_none_or(|h| p.usable_with(h)))
        })
        .map(|(i, _)| i)
        .collect()
}

/// What the overlay draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfferView {
    pub title: String,
    /// What the pack is, or what to pick between.
    pub detail: String,
    /// The packs to pick from; empty while asking yes or no.
    pub choices: Vec<String>,
    pub selected: usize,
    /// The answers on offer while asking: (button, what it does).
    pub answers: Vec<(String, String)>,
    pub hint: String,
    pub style: OverlayStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferOutcome {
    /// Add the candidate at this position, with this one of its profiles in use for the
    /// window (`None`: the pack's own rules stand).
    Add(usize, Option<usize>),
    /// Not now; ask again next time the game is started.
    No,
    /// Never ask about these packs again.
    Never,
}

struct Offered {
    name: String,
    description: String,
    id: String,
    /// The profiles the controllers can play, with their positions in the pack.
    profiles: Vec<(usize, String)>,
    /// How many profiles the pack has in all.
    total: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Ask,
    /// Choosing between packs.
    Pack,
    /// Choosing between the profiles of pack `.0`.
    Profile(usize),
}

pub struct OfferSession {
    packs: Vec<Offered>,
    stage: Stage,
    selected: usize,
    last_input: Instant,
}

impl OfferSession {
    pub fn new(library: &[Entry], candidates: &[usize], have: Option<&[Feature]>, now: Instant) -> Self {
        let packs = candidates
            .iter()
            .map(|&i| {
                let pack = &library[i].pack;
                let p = &pack.pack;
                Offered {
                    name: p.name.clone(),
                    description: p.description.clone(),
                    id: p.id.clone(),
                    profiles: pack
                        .profiles
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| have.is_none_or(|h| p.usable_with(h)))
                        .map(|(i, p)| (i, p.name.clone()))
                        .collect(),
                    total: pack.profiles.len(),
                }
            })
            .collect();
        OfferSession { packs, stage: Stage::Ask, selected: 0, last_input: now }
    }

    /// The IDs of the packs on offer.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.packs.iter().map(|p| p.id.as_str())
    }

    pub fn view(&self) -> OfferView {
        let style = OverlayStyle { position: ScreenPosition::Center, ..OverlayStyle::default() };
        let pick = |detail: String, choices: Vec<String>| OfferView {
            title: "Which profiles?".into(),
            detail,
            choices,
            selected: self.selected,
            answers: Vec::new(),
            hint: "D-pad ▲ ▼ choose   A add   B back".into(),
            style: style.clone(),
        };
        match self.stage {
            Stage::Pack => {
                return pick("Several sets of profiles fit this game".into(), self.packs.iter().map(|p| p.name.clone()).collect());
            }
            Stage::Profile(i) => return pick(format!("{} has several profiles", self.packs[i].name), self.packs[i].profiles.iter().map(|(_, n)| n.clone()).collect()),
            Stage::Ask => {}
        }
        let (title, detail) = match self.packs.as_slice() {
            [p] => (format!("Add controller profiles for {}?", p.name), p.description.clone()),
            [p, ..] => (format!("Add controller profiles for {}?", p.name), format!("{} sets of profiles fit this game", self.packs.len())),
            [] => (String::new(), String::new()),
        };
        let answers = [("A", "Yes"), ("B", "No"), ("X", "No, don't ask again")];
        OfferView {
            title,
            detail,
            choices: Vec::new(),
            selected: 0,
            answers: answers.iter().map(|(b, t)| ((*b).into(), (*t).into())).collect(),
            hint: "To be asked again, tap Guide in a game padwight doesn't know yet".into(),
            style,
        }
    }

    pub fn handle(&mut self, ev: InputEvent, now: Instant) -> Option<OfferOutcome> {
        let InputEvent::Button(b, true) = ev else { return None };
        self.last_input = now;
        if self.stage != Stage::Ask {
            return self.handle_pick(b);
        }
        match b {
            Button::South if self.packs.len() == 1 => self.chosen_pack(0),
            Button::South => {
                self.stage = Stage::Pack;
                self.selected = 0;
                None
            }
            Button::East => Some(OfferOutcome::No),
            Button::West => Some(OfferOutcome::Never),
            _ => None,
        }
    }

    /// Pack `i` is chosen: add it, after asking for a profile if it has several.
    fn chosen_pack(&mut self, i: usize) -> Option<OfferOutcome> {
        let pack = &self.packs[i];
        if pack.profiles.len() > 1 {
            self.stage = Stage::Profile(i);
            self.selected = 0;
            return None;
        }
        // Some profiles don't suit the controllers: the one left is the one to use.
        let only = pack.profiles.first().filter(|_| pack.total > 1).map(|(at, _)| *at);
        Some(OfferOutcome::Add(i, only))
    }

    fn handle_pick(&mut self, b: Button) -> Option<OfferOutcome> {
        let len = match self.stage {
            Stage::Profile(i) => self.packs[i].profiles.len(),
            _ => self.packs.len(),
        };
        match (b, self.stage) {
            (Button::DpadUp, _) => self.selected = self.selected.checked_sub(1).unwrap_or(len - 1),
            (Button::DpadDown, _) => self.selected = (self.selected + 1) % len,
            (Button::South, Stage::Profile(i)) => return Some(OfferOutcome::Add(i, Some(self.packs[i].profiles[self.selected].0))),
            (Button::South, _) => return self.chosen_pack(self.selected),
            (Button::East, Stage::Profile(_)) if self.packs.len() > 1 => {
                self.stage = Stage::Pack;
                self.selected = 0;
            }
            (Button::East, _) => self.stage = Stage::Ask,
            _ => {}
        }
        None
    }

    pub fn expired(&self, now: Instant) -> bool {
        self.last_input + IDLE_CLOSE <= now
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        Some(self.last_input + IDLE_CLOSE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{Game, Profile, Rule, RuleKind},
        pack,
    };

    fn entry(name: &str, exe: &str) -> Entry {
        let mut game = Game::new(name, vec![Profile::passthrough("P")]);
        game.rules = vec![Rule::new(RuleKind::Executable, exe, "P")];
        let pack = pack::export(&game, &Default::default(), &pack::draft(&game, false)).pack;
        Entry { file: "x.padpack", pack }
    }

    fn window(exe: &str) -> WindowInfo {
        WindowInfo { exe: exe.into(), class: "game".into(), ..Default::default() }
    }

    fn press(s: &mut OfferSession, b: Button) -> Option<OfferOutcome> {
        s.handle(InputEvent::Button(b, true), Instant::now())
    }

    #[test]
    fn only_unknown_library_games_are_offered() {
        let library = vec![entry("Doom", "doom.exe"), entry("Doom Remix", "DOOM.EXE"), entry("Quake", "quake.exe")];
        let mut config = Config::default();
        assert_eq!(candidates(&config, &library, &window("doom.exe"), None), [0, 1], "both packs fit");
        assert!(candidates(&config, &library, &window("other.exe"), None).is_empty());

        config.declined_packs.push(library[1].pack.pack.id.clone());
        assert_eq!(candidates(&config, &library, &window("doom.exe"), None), [0]);

        let plan = pack::plan(&config, library[0].pack.clone(), true);
        pack::apply(&mut config, &plan, &pack::Choices::default());
        assert!(candidates(&config, &library, &window("doom.exe"), None).is_empty(), "a setup with rules is the config's");
    }

    #[test]
    fn answers_and_picking() {
        let library = vec![entry("Doom", "doom.exe"), entry("Doom Remix", "doom.exe")];
        let one = |i: usize| OfferSession::new(&library, &[i], None, Instant::now());
        assert_eq!(press(&mut one(0), Button::South), Some(OfferOutcome::Add(0, None)));
        assert_eq!(press(&mut one(0), Button::East), Some(OfferOutcome::No));
        assert_eq!(press(&mut one(0), Button::West), Some(OfferOutcome::Never));
        assert_eq!(press(&mut one(0), Button::North), None);

        let mut two = OfferSession::new(&library, &[0, 1], None, Instant::now());
        assert_eq!(press(&mut two, Button::South), None, "yes leads to the list");
        assert_eq!(two.view().choices, ["Doom", "Doom Remix"]);
        press(&mut two, Button::DpadDown);
        press(&mut two, Button::DpadDown);
        press(&mut two, Button::DpadUp);
        assert_eq!(press(&mut two, Button::South), Some(OfferOutcome::Add(1, None)));

        // A pack with several profiles asks which one.
        let mut multi = entry("Doom", "doom.exe");
        multi.pack.profiles.push(Profile::passthrough("Alt"));
        let library = vec![multi];
        let mut s = OfferSession::new(&library, &[0], None, Instant::now());
        assert_eq!(press(&mut s, Button::South), None);
        assert_eq!(s.view().choices, ["P", "Alt"]);
        press(&mut s, Button::DpadDown);
        assert_eq!(press(&mut s, Button::South), Some(OfferOutcome::Add(0, Some(1))));
        press(&mut s, Button::East);
        assert!(s.view().choices.is_empty(), "B from the profile list goes back to the question");

    }

    #[test]
    fn profiles_the_controller_cannot_play_are_not_offered() {
        // Profiles that need a gyro are neither listed nor offered without one.
        let mut gyro = entry("Doom", "doom.exe");
        gyro.pack.profiles[0].requires = vec![Feature::Gyro];
        gyro.pack.profiles.push(Profile::passthrough("Plain"));
        gyro.pack.profiles.push(Profile::passthrough("Plain 2"));
        let library = vec![gyro];
        let mut s = OfferSession::new(&library, &[0], Some(&[]), Instant::now());
        press(&mut s, Button::South);
        assert_eq!(s.view().choices, ["Plain", "Plain 2"]);
        press(&mut s, Button::DpadDown);
        assert_eq!(press(&mut s, Button::South), Some(OfferOutcome::Add(0, Some(2))), "the position in the pack");
        let mut s = OfferSession::new(&library, &[0], Some(&[Feature::Gyro]), Instant::now());
        press(&mut s, Button::South);
        assert_eq!(s.view().choices, ["P", "Plain", "Plain 2"]);

        let mut one = entry("Doom", "doom.exe");
        one.pack.profiles[0].requires = vec![Feature::Gyro];
        one.pack.profiles.push(Profile::passthrough("Plain"));
        let library = vec![one];
        let mut s = OfferSession::new(&library, &[0], Some(&[]), Instant::now());
        assert_eq!(press(&mut s, Button::South), Some(OfferOutcome::Add(0, Some(1))), "a lone fit is used without asking");
        let mut gyro_only = entry("Doom", "doom.exe");
        gyro_only.pack.profiles[0].requires = vec![Feature::Gyro];
        assert!(candidates(&Config::default(), &[gyro_only.clone()], &window("doom.exe"), Some(&[])).is_empty());
        assert_eq!(candidates(&Config::default(), &[gyro_only.clone()], &window("doom.exe"), Some(&[Feature::Gyro])), [0]);
        assert_eq!(candidates(&Config::default(), &[gyro_only], &window("doom.exe"), None), [0]);
    }

    #[test]
    fn b_goes_back_to_the_question() {
        let library = vec![entry("Doom", "doom.exe"), entry("Doom Remix", "doom.exe")];
        let mut back = OfferSession::new(&library, &[0, 1], None, Instant::now());
        press(&mut back, Button::South);
        assert_eq!(press(&mut back, Button::East), None, "B goes back to the question");
        assert!(back.view().choices.is_empty());
    }
}

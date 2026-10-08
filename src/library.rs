//! The built-in game library: packs embedded from `packs/` at build time (see `build.rs`).

use crate::{
    config::Config,
    pack::{self, Pack},
};

mod files {
    include!(concat!(env!("OUT_DIR"), "/library.rs"));
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub file: &'static str,
    pub pack: Pack,
}

/// Every library pack, by name. Packs that don't parse are left out (tests keep that from
/// happening).
pub fn entries() -> Vec<Entry> {
    let mut all: Vec<Entry> = files::FILES
        .iter()
        .filter_map(|(file, text)| pack::parse(text).ok().map(|pack| Entry { file, pack }))
        .collect();
    all.sort_by_key(|e| e.pack.pack.name.to_lowercase());
    all
}

/// Games added from the library that it now has a newer version of: (game name, index of
/// the entry in `library`).
pub fn updates<'a>(config: &'a Config, library: &[Entry]) -> Vec<(&'a str, usize)> {
    config
        .games
        .iter()
        .filter_map(|g| {
            let origin = g.origin.as_ref().filter(|o| o.library)?;
            let i = library.iter().position(|e| e.pack.pack.id == origin.id)?;
            pack::compare_versions(&library[i].pack.pack.version, &origin.version)
                .is_gt()
                .then_some((g.name.as_str(), i))
        })
        .collect()
}

/// What's wrong with a pack as a library pack, beyond parsing.
#[cfg(test)]
pub fn problems(pack: &Pack) -> Vec<String> {
    let mut problems = Vec::new();
    let out = pack::export(&pack.to_game(), &crate::config::Shared::default(), &pack.info());
    problems.extend(out.dangling.iter().map(|d| format!("refers to missing {d}")));
    if pack.rules.is_empty() {
        problems.push("has no auto-switch rule".into());
    }
    // Every key it presses must be a real key name.
    let game = pack.to_game();
    let mut actions: Vec<&crate::config::ButtonAction> = game.profiles.iter().flat_map(|p| p.actions()).collect();
    actions.extend(game.layers.iter().flat_map(|l| l.actions()));
    actions.extend(game.menus.iter().flat_map(|m| m.items.iter().map(|i| &i.action)));
    actions.extend(game.macros.iter().flat_map(|m| m.steps.iter().filter_map(crate::config::MacroStep::action)));
    let mut keys: Vec<&String> = actions.into_iter().flat_map(|a| a.key_names()).collect();
    for s in game.profiles.iter().flat_map(|p| [&p.left_stick, &p.right_stick]) {
        if let crate::config::StickAction::Keys { up, down, left, right } = &s.action {
            keys.extend([up, down, left, right]);
        }
    }
    for k in keys {
        if <evdev::KeyCode as std::str::FromStr>::from_str(k).is_err() {
            problems.push(format!("presses unknown key {k:?}"));
        }
    }
    // Guide opens the Guide layer in every profile, which the pack has.
    if game.profiles.iter().any(|p| !p.holds_guide_layer()) {
        problems.push("a profile's Guide button doesn't hold the Guide layer".into());
    }
    if !game.layers.iter().any(|l| l.name == crate::config::GUIDE_LAYER) {
        problems.push("has no Guide layer".into());
    }
    // Players with a plain pad must get something.
    if !pack.profiles.iter().any(|p| p.usable_with(&[])) {
        problems.push("has no profile that works on a plain pad".into());
    }
    problems
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::config::Shared;
    use crate::config::{Button, ButtonAction, Game, Profile, Rule, RuleKind};

    #[test]
    fn every_library_pack_is_valid() {
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for (file, text) in files::FILES {
            let pack = pack::parse(text).unwrap_or_else(|e| panic!("{file}: {e:#}"));
            assert_eq!(pack.format, pack::FORMAT, "{file}: write library packs at the current format");
            assert!(ids.insert(pack.pack.id.clone()), "{file}: pack ID used twice");
            assert!(names.insert(pack.pack.name.to_lowercase()), "{file}: game name used twice");
            assert_eq!(problems(&pack), Vec::<String>::new(), "{file}");
        }
        assert_eq!(entries().len(), files::FILES.len());
    }

    #[test]
    fn shooters_offer_plain_gyro_and_flick_stick_profiles() {
        use crate::config::{GyroMode, StickAction};
        for file in ["deus-ex", "fear", "max-payne", "max-payne-2"] {
            let text = files::FILES.iter().find(|(f, _)| f.contains(file)).map(|(_, t)| t).unwrap();
            let pack = pack::parse(text).unwrap();
            let flick = |p: &Profile| [&p.left_stick, &p.right_stick].iter().any(|s| matches!(s.action, StickAction::Flick { .. }));
            let gyro = |p: &Profile| p.gyro.mode != GyroMode::Off;
            let plain: Vec<_> = pack.profiles.iter().filter(|p| !gyro(p) && !flick(p) && p.usable_with(&[])).collect();
            assert_eq!(plain.len(), 1, "{file}: one profile without gyro");
            assert!(pack.profiles.iter().any(|p| gyro(p) && !flick(p) && p.usable_with(&[])), "{file}: gyro as an extra, playable without");
            // Flick stick turns up and down with gyro only, so it can't be played without.
            assert!(pack.profiles.iter().any(|p| flick(p) && gyro(p) && !p.usable_with(&[])), "{file}: flick stick that declares gyro");
            assert!(pack.rules.iter().all(|r| plain[0].name == r.profile), "{file}: rules start on the plain profile");
        }
    }

    #[test]
    fn starcraft_has_a_gyro_pointer_profile_that_works_without_gyro() {
        use crate::config::GyroMode;
        let text = files::FILES.iter().find(|(f, _)| f.contains("starcraft")).map(|(_, t)| t).unwrap();
        let pack = pack::parse(text).unwrap();
        let p = pack.profiles.iter().find(|p| p.name == "Gameplay + Gyro").unwrap();
        assert!(matches!(p.gyro.mode, GyroMode::Mouse { .. }) && p.usable_with(&[]));
        assert!(pack.layers.iter().any(|l| l.name == "Menus"), "menus are a layer, not a profile");
        assert!(pack.profiles.iter().all(|p| !p.name.contains("Menus")));
        // Guide stays for things that aren't about this game: a long press of Select toggles it.
        let menus = ButtonAction::toggle(ButtonAction::Layer("Menus".into()));
        let long_press = |g: Option<&crate::config::Gestures>| g.and_then(|g| g.get(crate::config::GestureKind::LongPress)).cloned();
        for p in &pack.profiles {
            assert_eq!(long_press(p.gestures.get(&Button::Select)), Some(menus.clone()), "{}", p.name);
        }
        let layer = pack.layers.iter().find(|l| l.name == "Menus").unwrap();
        assert_eq!(long_press(layer.gestures.get(&Button::Select)), Some(menus), "and off again");
    }

    #[test]
    fn the_checks_catch_broken_packs() {
        let mut game = Game::new("Doom", vec![Profile::passthrough("P")]);
        game.profiles[0].set_button(Button::North, ButtonAction::Macro { name: "Gone".into(), repeat: false });
        let mut pack = pack::export(&game, &Shared::default(), &pack::draft(&game, false)).pack;
        pack.profiles[0].requires = vec![crate::config::Feature::Gyro];
        let found = problems(&pack);
        assert_eq!(found.len(), 3, "{found:?}");
        pack.rules.push(Rule::new(RuleKind::Executable, "doom.exe", "P"));
        assert_eq!(problems(&pack).len(), 2);
        pack.profiles[0].set_button(Button::South, ButtonAction::Keys(vec!["KEY_NOPE".into()]));
        assert!(problems(&pack).iter().any(|p| p.contains("KEY_NOPE")));
    }

    #[test]
    fn newer_library_versions_are_offered_as_updates() {
        let game = Game::new("Doom", vec![Profile::passthrough("P")]);
        let mut entry = Entry { file: "doom.padpack", pack: pack::export(&game, &Shared::default(), &pack::draft(&game, false)).pack };
        let mut config = Config::default();
        let plan = pack::plan(&config, entry.pack.clone(), true);
        pack::apply(&mut config, &plan, &pack::Choices::default());
        assert!(updates(&config, std::slice::from_ref(&entry)).is_empty());
        entry.pack.pack.version = "1.1".into();
        assert_eq!(updates(&config, std::slice::from_ref(&entry)).len(), 1);
        config.games[0].origin.as_mut().unwrap().library = false;
        assert!(updates(&config, &[entry]).is_empty(), "only library games follow the library");
    }
}

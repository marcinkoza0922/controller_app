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
    use std::collections::BTreeSet;
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
    let used: BTreeSet<_> = out.features.iter().map(|f| f.feature).collect();
    let declared: BTreeSet<_> = pack.pack.requires.iter().copied().collect();
    if used != declared {
        problems.push(format!("requires {declared:?} but uses {used:?}"));
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
            assert!([pack::FORMAT_WITHOUT_KEYBOARD, pack::FORMAT].contains(&pack.format), "{file}: write library packs at the current format");
            assert!(ids.insert(pack.pack.id.clone()), "{file}: pack ID used twice");
            assert!(names.insert(pack.pack.name.to_lowercase()), "{file}: game name used twice");
            assert_eq!(problems(&pack), Vec::<String>::new(), "{file}");
        }
        assert_eq!(entries().len(), files::FILES.len());
    }

    #[test]
    fn the_checks_catch_broken_packs() {
        let mut game = Game::new("Doom", vec![Profile::passthrough("P")]);
        game.profiles[0].set_button(Button::North, ButtonAction::Macro { name: "Gone".into(), repeat: false });
        let mut pack = pack::export(&game, &Shared::default(), &pack::draft(&game, false)).pack;
        pack.pack.requires = vec![pack::Feature::Gyro];
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

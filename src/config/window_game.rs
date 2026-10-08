//! Making a game (a pack) for the window in front, when a setting is changed outside any game.

use super::{Config, Game, Profile, ProfileRef};
use crate::ipc::WindowInfo;

impl Config {
    /// Adds a game for `window` holding `profile`, with a rule that picks it for the window.
    /// The game takes General's macros, menus, layers and overlays, so the profile's references
    /// still resolve. Returns the new profile.
    pub fn add_game_for_window(&mut self, window: &WindowInfo, profile: Profile) -> ProfileRef {
        let base = window_game_name(window);
        let name = (1..)
            .map(|i| if i == 1 { base.clone() } else { format!("{base} {i}") })
            .find(|n| n != &self.general.name && !self.games.iter().any(|g| &g.name == n))
            .unwrap_or(base);
        let profile_name = profile.name.clone();
        let mut game = Game::new(&name, vec![profile]);
        game.rules = vec![crate::focus::rule_for_window(window, profile_name.clone())];
        game.macros.clone_from(&self.general.macros);
        game.menus.clone_from(&self.general.menus);
        game.info.clone_from(&self.general.info);
        game.logs.clone_from(&self.general.logs);
        game.layers.clone_from(&self.general.layers);
        self.games.push(game);
        ProfileRef::new(Some(&name), &profile_name)
    }
}

/// The window's executable without `.exe`, else its class, else a plain name.
fn window_game_name(window: &WindowInfo) -> String {
    let exe = window.exe.rsplit_once('.').filter(|(_, ext)| ext.eq_ignore_ascii_case("exe")).map_or(window.exe.as_str(), |(stem, _)| stem);
    [exe, window.class.as_str()].into_iter().map(str::trim).find(|s| !s.is_empty()).unwrap_or("Game").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_game_after_the_window_and_keeps_names_unique() {
        let mut config = Config::default();
        let window = WindowInfo { exe: "Eldenring.EXE".into(), class: "steam_app_1".into(), ..Default::default() };
        let profile = config.general.profiles[0].clone();
        let first = config.add_game_for_window(&window, profile.clone());
        assert_eq!(first, ProfileRef::new(Some("Eldenring"), &profile.name));
        let second = config.add_game_for_window(&window, profile.clone());
        assert_eq!(second.game.as_deref(), Some("Eldenring 2"));
        let rule = &config.game(Some("Eldenring")).unwrap().rules[0];
        assert_eq!(rule.profile, profile.name);
    }

    #[test]
    fn a_window_with_no_name_still_gets_a_game() {
        let mut config = Config::default();
        let profile = config.general.profiles[0].clone();
        let at = config.add_game_for_window(&WindowInfo::default(), profile);
        assert_eq!(at.game.as_deref(), Some("Game"));
    }
}

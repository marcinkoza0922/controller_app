//! What saving checks: names, references and the rules each game, profile and item must follow.

use super::*;

/// What's wrong with a menu item's action: the usual checks, plus which menus it may open
/// (see [`crate::menu::can_open`]).
pub(super) fn item_problem(menu: &Menu, action: &ButtonAction, menus: &[&Menu], names: &Names) -> Option<String> {
    if holds_layer(action) {
        return Some("a menu item can only toggle a layer (wrap it in Toggle)".into());
    }
    action_problem(action, names).or_else(|| {
        let mut problem = None;
        action.walk(&mut |a| {
            if let ButtonAction::OpenMenu(name) = a
                && problem.is_none()
            {
                if matches!(menu.kind, MenuKind::Radial { .. }) {
                    problem = Some("radial menus can't open other menus".to_string());
                } else if let Some(child) = menus.iter().find(|m| &m.name == name)
                    && child.kind.tag() != menu.kind.tag()
                {
                    problem = Some(format!("{name:?} isn't a {} menu", menu.kind.tag().short().to_lowercase()));
                }
            }
        });
        problem
    })
}

pub(super) fn info_has_problem(info: &[InfoOverlay]) -> bool {
    info.iter().enumerate().any(|(i, o)| o.name.trim().is_empty() || info[..i].iter().any(|other| other.name == o.name))
}

pub(super) fn menus_have_problem(menus: &[Menu], reachable: &[&Menu], names: &Names) -> bool {
    menus.iter().enumerate().any(|(i, m)| {
        m.name.trim().is_empty()
            || menus[..i].iter().any(|o| o.name == m.name)
            || m.items.iter().any(|item| item_problem(m, &item.action, reachable, names).is_some())
    })
}

/// Macro, menu, info overlay and layer names, for the "Macro…"/"Open menu…"/"Show info
/// overlay…"/"Layer…" pickers and for checking references.
#[derive(Debug, Clone, Default)]
pub(super) struct Names {
    pub(super) macros: Vec<String>,
    pub(super) menus: Vec<String>,
    /// Info overlays an action can show: not those set to always show.
    pub(super) infos: Vec<String>,
    /// Info overlays always on screen, which actions can't show.
    pub(super) always_infos: Vec<String>,
    /// Log overlays an action can show, and those always on screen, likewise.
    pub(super) logs: Vec<String>,
    pub(super) always_logs: Vec<String>,
    pub(super) layers: Vec<String>,
    /// False for shared items, which can't use layers (they're always a game's own).
    pub(super) layers_allowed: bool,
}

impl Names {
    pub(super) fn of(scope: &ScopeRef, layers_allowed: bool) -> Self {
        let list = |kind| scope.names(kind).into_iter().map(str::to_string).collect();
        let infos = |always: bool| scope.info.iter().filter(|o| o.always == always).map(|o| o.name.clone()).collect();
        let logs = |always: bool| scope.logs.iter().filter(|o| o.always == always).map(|o| o.name.clone()).collect();
        Names {
            macros: list(ItemKind::Macro),
            menus: list(ItemKind::Menu),
            infos: infos(false),
            always_infos: infos(true),
            logs: logs(false),
            always_logs: logs(true),
            layers: list(ItemKind::Layer),
            layers_allowed,
        }
    }

    /// What a game's profiles and items can use: its own items, then shared ones.
    pub(super) fn for_game(config: &Config, game: &Game) -> Self {
        Names::of(&config.scope_of(Some(game)), true)
    }

    /// What shared items can use: only other shared items.
    pub(super) fn shared(config: &Config) -> Self {
        Names::of(&config.scope_of(None), false)
    }

    pub(super) fn list(&self, kind: ItemKind) -> &[String] {
        match kind {
            ItemKind::Macro => &self.macros,
            ItemKind::Menu => &self.menus,
            ItemKind::Info => &self.infos,
            ItemKind::Log => &self.logs,
            ItemKind::Layer => &self.layers,
        }
    }
}

/// Whether `action` holds a layer outside any Toggle, which a tap (a menu item) can't do.
pub(super) fn holds_layer(action: &ButtonAction) -> bool {
    match action {
        ButtonAction::Layer(_) => true,
        ButtonAction::Multi(list) => list.iter().any(holds_layer),
        ButtonAction::Turbo { action, .. } => holds_layer(action),
        _ => false,
    }
}

/// What's wrong with an action, if anything (the same things saving checks).
pub(super) fn action_problem(action: &ButtonAction, names: &Names) -> Option<String> {
    let mut problem = None;
    action.walk(&mut |a| {
        if problem.is_some() {
            return;
        }
        match a {
            ButtonAction::Keys(keys) => {
                if let Some(bad) = keys.iter().find(|k| KeyCode::from_str(k).is_err()) {
                    problem = Some(format!("unknown key {:?}", short_key(bad)));
                }
            }
            ButtonAction::Macro { name, .. } if !names.macros.contains(name) => {
                problem = Some(format!("missing macro {name:?}"));
            }
            ButtonAction::OpenMenu(name) if !names.menus.contains(name) => {
                problem = Some(format!("missing menu {name:?}"));
            }
            ButtonAction::ShowInfo(name) if names.always_infos.contains(name) => {
                problem = Some(format!("info overlay {name:?} is always shown, so an action can't show it"));
            }
            ButtonAction::ShowInfo(name) if !names.infos.contains(name) => {
                problem = Some(format!("missing info overlay {name:?}"));
            }
            ButtonAction::ShowLog(name) if names.always_logs.contains(name) => {
                problem = Some(format!("log overlay {name:?} is always shown, so an action can't show it"));
            }
            ButtonAction::ShowLog(name) if !names.logs.contains(name) => {
                problem = Some(format!("missing log overlay {name:?}"));
            }
            ButtonAction::Layer(_) if !names.layers_allowed => {
                problem = Some("shared items can't use layers".into());
            }
            ButtonAction::Layer(name) if !names.layers.contains(name) => {
                problem = Some(format!("missing layer {name:?}"));
            }
            _ => {}
        }
    });
    problem
}

/// Whether a profile section contains anything saving would reject.
pub(super) fn section_has_problem(p: &Profile, tab: ProfileTab, names: &Names) -> bool {
    let bad = |a: &ButtonAction| action_problem(a, names).is_some();
    let button_bad = |b: Button| {
        bad(p.button(b)) || p.gestures.get(&b).is_some_and(|g| GestureKind::ALL.iter().any(|k| g.get(*k).is_some_and(bad)))
    };
    match tab {
        ProfileTab::Buttons => Button::ALL.into_iter().any(button_bad),
        ProfileTab::Sticks => {
            let dirs = [Stick::Left, Stick::Right].into_iter().flat_map(Button::stick_directions).any(button_bad);
            let zones = [Analog::Stick(Stick::Left), Analog::Stick(Stick::Right), Analog::Trigger(Trigger::Left), Analog::Trigger(Trigger::Right)]
                .into_iter()
                .any(|a| p.zones(a).iter().any(|z| z.min >= z.max || bad(&z.action)));
            let triggers = [Trigger::Left, Trigger::Right]
                .into_iter()
                .any(|t| matches!(p.trigger(t), TriggerAction::Button { action, .. } if bad(action)));
            let keys = [Stick::Left, Stick::Right].into_iter().any(|s| match &p.stick(s).action {
                StickAction::Keys { up, down, left, right } => [up, down, left, right].iter().any(|k| KeyCode::from_str(k).is_err()),
                _ => false,
            });
            dirs || zones || triggers || keys
        }
        ProfileTab::Combos => p.combos.iter().any(|c| c.buttons.len() < 2 || bad(&c.action)),
        ProfileTab::Gyro => false,
    }
}

pub(super) fn macros_have_problem(macros: &[Macro], names: &Names) -> bool {
    macros.iter().enumerate().any(|(i, m)| {
        m.name.trim().is_empty()
            || macros[..i].iter().any(|o| o.name == m.name)
            || m.steps.iter().filter_map(MacroStep::action).any(|a| action_problem(a, names).is_some())
    })
}

/// The menus a game's items can open: its own, then shared ones (only shared ones for
/// `None`, the shared items themselves).
pub(super) fn reachable_menus<'a>(config: &'a Config, game: Option<&'a Game>) -> Vec<&'a Menu> {
    config.scope_of(game).menus
}

/// The first thing saving would reject in a list of macros, menus and info overlays.
/// `names` is what they may refer to; `shared` holds the shared items' names, which a game's
/// items may not reuse.
#[expect(clippy::too_many_arguments, reason = "predates the size lints")]
pub(super) fn items_problem(macros: &[Macro], menus: &[Menu], info: &[InfoOverlay], names: &Names, reachable: &[&Menu], shared: Option<&Names>) -> Option<String> {
    let clash = |kind: ItemKind, name: &str| shared.is_some_and(|s| s.list(kind).iter().any(|n| n == name));
    for (i, m) in macros.iter().enumerate() {
        if m.name.trim().is_empty() {
            return Some("Macro names cannot be empty.".into());
        }
        if macros[..i].iter().any(|o| o.name == m.name) {
            return Some(format!("Two macros are named {:?}.", m.name));
        }
        if clash(ItemKind::Macro, &m.name) {
            return Some(format!("Macro {:?} has the same name as a shared macro.", m.name));
        }
        let keys = m.steps.iter().filter_map(MacroStep::action).flat_map(|a| a.key_names());
        if let Some(bad) = keys.into_iter().find(|k| KeyCode::from_str(k).is_err()) {
            return Some(format!("Macro {:?}: unknown key {:?}", m.name, short_key(bad)));
        }
        if m.steps.iter().filter_map(MacroStep::action).any(|a| {
            let mut layer = false;
            a.walk(&mut |a| layer |= matches!(a, ButtonAction::Layer(_)));
            layer
        }) {
            return Some(format!("Macro {:?}: macros can't hold layers.", m.name));
        }
        if let Some(problem) = m.steps.iter().filter_map(MacroStep::action).find_map(|a| action_problem(a, names)) {
            return Some(format!("Macro {:?}: {problem}", m.name));
        }
    }
    for (i, m) in menus.iter().enumerate() {
        if m.name.trim().is_empty() {
            return Some("Menu names cannot be empty.".into());
        }
        if menus[..i].iter().any(|o| o.name == m.name) {
            return Some(format!("Two menus are named {:?}.", m.name));
        }
        if clash(ItemKind::Menu, &m.name) {
            return Some(format!("Menu {:?} has the same name as a shared menu.", m.name));
        }
        if let Some(problem) = m.items.iter().find_map(|item| item_problem(m, &item.action, reachable, names)) {
            return Some(format!("Menu {:?}: {problem}", m.name));
        }
    }
    for (i, o) in info.iter().enumerate() {
        if o.name.trim().is_empty() {
            return Some("Info overlay names cannot be empty.".into());
        }
        if info[..i].iter().any(|other| other.name == o.name) {
            return Some(format!("Two info overlays are named {:?}.", o.name));
        }
        if clash(ItemKind::Info, &o.name) {
            return Some(format!("Info overlay {:?} has the same name as a shared one.", o.name));
        }
    }
    None
}

/// The first thing saving would reject in a list of log overlays: empty or repeated names,
/// or (for a game's) a shared one's name.
pub(super) fn logs_problem(logs: &[LogOverlay], shared: Option<&Names>) -> Option<String> {
    let clash = |name: &str| shared.is_some_and(|s| s.logs.iter().chain(&s.always_logs).any(|n| n == name));
    for (i, o) in logs.iter().enumerate() {
        if o.name.trim().is_empty() {
            return Some("Log overlay names cannot be empty.".into());
        }
        if logs[..i].iter().any(|other| other.name == o.name) {
            return Some(format!("Two log overlays are named {:?}.", o.name));
        }
        if clash(&o.name) {
            return Some(format!("Log overlay {:?} has the same name as a shared one.", o.name));
        }
    }
    None
}

/// The first thing saving would reject in a game (or General).
pub(super) fn game_problem(config: &Config, g: &Game) -> Option<String> {
    if g.profiles.is_empty() {
        return Some("needs at least one profile.".into());
    }
    let names = Names::for_game(config, g);
    let reachable = reachable_menus(config, Some(g));
    if let Some(problem) = items_problem(&g.macros, &g.menus, &g.info, &names, &reachable, Some(&Names::shared(config))) {
        return Some(problem);
    }
    if let Some(problem) = logs_problem(&g.logs, Some(&Names::shared(config))) {
        return Some(problem);
    }
    for (i, p) in g.profiles.iter().enumerate() {
        if p.name.trim().is_empty() {
            return Some("Profile names cannot be empty.".into());
        }
        if g.profiles[..i].iter().any(|o| o.name == p.name) {
            return Some(format!("Two profiles are named {:?}.", p.name));
        }
        if let Some(problem) = p.actions().into_iter().find_map(|a| {
            action_problem(a, &names).filter(|problem| !problem.starts_with("unknown key"))
        }) {
            return Some(format!("Profile {:?}: {problem}.", p.name));
        }
        if p.kind == crate::config::ProfileKind::Keyboard
            && let Some(problem) = keymap::keyboard_problem(&p.keyboard)
        {
            return Some(format!("Profile {:?}: {problem}.", p.name));
        }
        if let Some(c) = p.combos.iter().find(|c| c.buttons.len() < 2) {
            return Some(format!("Profile {:?}: a combo needs at least two buttons (has {}).", p.name, c.buttons.len()));
        }
        let analogs = [
            Analog::Stick(Stick::Left),
            Analog::Stick(Stick::Right),
            Analog::Trigger(Trigger::Left),
            Analog::Trigger(Trigger::Right),
        ];
        if analogs.into_iter().any(|a| p.zones(a).iter().any(|z| z.min >= z.max)) {
            return Some(format!("Profile {:?}: a zone's range must start below where it ends.", p.name));
        }
        let mut keys: Vec<&String> = p.actions().into_iter().flat_map(|a| a.key_names()).collect();
        for s in [Stick::Left, Stick::Right] {
            if let StickAction::Keys { up, down, left, right } = &p.stick(s).action {
                keys.extend([up, down, left, right]);
            }
        }
        if let Some(bad) = keys.iter().find(|k| KeyCode::from_str(k).is_err()) {
            return Some(format!("Profile {:?}: unknown key {:?}", p.name, short_key(bad)));
        }
    }
    layers_problem(g, &names).or_else(|| rules_problem(g))
}

/// The first thing saving would reject in a game's layers.
pub(super) fn layers_problem(g: &Game, names: &Names) -> Option<String> {
    use crate::config::Indicator;
    for (i, l) in g.layers.iter().enumerate() {
        if l.name.trim().is_empty() {
            return Some("Layer names cannot be empty.".into());
        }
        if g.layers[..i].iter().any(|o| o.name == l.name) {
            return Some(format!("Two layers are named {:?}.", l.name));
        }
        let label = format!("Layer {:?}", l.name);
        if let Indicator::Info(info) = &l.indicator
            && !names.infos.contains(info)
        {
            return Some(if names.always_infos.contains(info) {
                format!("{label}: info overlay {info:?} is always shown, so it can't show the layer is on.")
            } else {
                format!("{label}: missing info overlay {info:?} to show.")
            });
        }
        if let Some(problem) = l.actions().into_iter().find_map(|a| action_problem(a, names)) {
            return Some(format!("{label}: {problem}."));
        }
        if let Some(c) = l.combos.iter().find(|c| c.buttons.len() < 2) {
            return Some(format!("{label}: a combo needs at least two buttons (has {}).", c.buttons.len()));
        }
        let zones = [&l.left_stick, &l.right_stick].into_iter().flatten().flat_map(|s| &s.zones);
        let zones = zones.chain([&l.left_trigger, &l.right_trigger].into_iter().flatten().flat_map(|t| &t.zones));
        if zones.into_iter().any(|z| z.min >= z.max) {
            return Some(format!("{label}: a zone's range must start below where it ends."));
        }
        let stick_keys = [&l.left_stick, &l.right_stick].into_iter().flatten().flat_map(|s| match &s.action {
            StickAction::Keys { up, down, left, right } => vec![up, down, left, right],
            _ => Vec::new(),
        });
        if let Some(bad) = stick_keys.into_iter().find(|k| KeyCode::from_str(k).is_err()) {
            return Some(format!("{label}: unknown key {:?}", short_key(bad)));
        }
    }
    None
}

/// What's wrong with a game's auto-switch rules, if anything.
pub(super) fn rules_problem(g: &Game) -> Option<String> {
    if let Some(r) = g.rules.iter().find(|r| r.value.trim().is_empty()) {
        return Some(format!("A per-game rule for profile {:?} has no {} to match.", r.profile, r.kind));
    }
    if let Some(r) = g.rules.iter().find(|r| g.profile(&r.profile).is_none()) {
        return Some(format!("A per-game rule points to missing profile {:?}.", r.profile));
    }
    None
}

impl App {
    pub(super) fn validate(&self) -> Option<String> {
        let config = &self.config;
        let shared = Names::shared(config);
        let reachable = reachable_menus(config, None);
        if let Some(problem) = items_problem(&config.shared.macros, &config.shared.menus, &config.shared.info, &shared, &reachable, None) {
            return Some(format!("Shared: {problem}"));
        }
        if let Some(problem) = logs_problem(&config.shared.logs, None) {
            return Some(format!("Shared: {problem}"));
        }
        for (i, g) in config.games.iter().enumerate() {
            if g.name.trim().is_empty() {
                return Some("Game names cannot be empty.".into());
            }
            if config.games[..i].iter().any(|o| o.name == g.name) {
                return Some(format!("Two games are named {:?}.", g.name));
            }
        }
        for (key, g) in config.all_games() {
            let label = key.unwrap_or("General");
            if let Some(problem) = game_problem(config, g) {
                return Some(format!("{label}: {problem}"));
            }
        }
        if let Some(d) = config.auto_switch.default_profile.as_ref().filter(|d| config.profile(d).is_none()) {
            return Some(format!("The per-game default profile {d} no longer exists."));
        }
        panic_chord_problem(&config.panic_chord)
    }

}

/// Why the panic chord can't be used: it must name at least one known key.
pub(super) fn panic_chord_problem(chord: &[String]) -> Option<String> {
    if chord.is_empty() {
        return Some("The panic chord can't be empty.".into());
    }
    chord.iter().find(|k| KeyCode::from_str(k).is_err()).map(|bad| format!("The panic chord has an unknown key: {:?}.", short_key(bad)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problems_are_found_inside_nested_actions_and_flag_their_section() {
        let names = Names { macros: vec!["Known".to_string()], ..Names::default() };
        let nested = ButtonAction::Multi(vec![
            ButtonAction::Mouse(MouseButton::Left),
            ButtonAction::toggle(ButtonAction::Keys(vec!["KEY_NOPE".into()])),
        ]);
        assert_eq!(action_problem(&nested, &names).as_deref(), Some("unknown key \"NOPE\""));
        let missing = ButtonAction::Macro { name: "Gone".into(), repeat: false };
        assert_eq!(action_problem(&missing, &names).as_deref(), Some("missing macro \"Gone\""));
        assert_eq!(action_problem(&ButtonAction::Macro { name: "Known".into(), repeat: false }, &names), None);

        let mut p = Profile::passthrough("p");
        assert!(ProfileTab::ALL.iter().all(|t| !section_has_problem(&p, *t, &names)));
        p.set_button(Button::RightStickUp, missing);
        assert!(section_has_problem(&p, ProfileTab::Sticks, &names), "stick directions live on the Sticks tab");
        assert!(!section_has_problem(&p, ProfileTab::Buttons, &names));
        p.combos.push(Combo { buttons: vec![Button::South], action: ButtonAction::Disabled });
        assert!(section_has_problem(&p, ProfileTab::Combos, &names));
    }
}

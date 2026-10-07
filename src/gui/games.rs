//! The sidebar, a game's page and Details tab, the Settings page, and the controller list.

use iced::widget::{column, row};

use super::*;

/// "from <pack> 1.2 by <author>" for a game made from a pack.
pub(super) fn origin_note(game: &Game) -> Option<String> {
    let origin = game.origin.as_ref()?;
    let by = if game.pack.author.is_empty() { String::new() } else { format!(" by {}", game.pack.author) };
    let source = if origin.library { "library" } else { "pack" };
    Some(format!("from {source}, version {}{by}", origin.version))
}

/// An entry in the auto-switch "Otherwise use" list.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DefaultChoice(pub(super) Option<ProfileRef>);

impl fmt::Display for DefaultChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(at) => at.fmt(f),
            None => f.write_str("(keep current profile)"),
        }
    }
}

/// The motion-sensor udev rule shipped in dist/, built in so the command works from anywhere.
pub(super) const MOTION_RULE_FILE: &str = include_str!("../../dist/70-controller-app-motion.rules");

/// Shell command (bash or fish) that installs the motion-sensor udev rule and applies it.
pub(super) fn motion_rule_command() -> String {
    let rules: Vec<&str> = MOTION_RULE_FILE
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    format!(
        "echo '{}' | sudo tee /etc/udev/rules.d/70-controller-app-motion.rules >/dev/null \
         && sudo udevadm control --reload && sudo udevadm trigger",
        rules.join("\n")
    )
}

/// Rule for a window using its most specific identifier: Steam App ID, then a Windows `.exe`
/// name, then the window class, then the native executable name.
pub(super) fn rule_for_window(w: &WindowInfo, profile: String) -> Rule {
    let (kind, value) = if let Some(id) = &w.steam_app_id {
        (RuleKind::SteamAppId, id.clone())
    } else if w.exe.to_ascii_lowercase().ends_with(".exe") {
        (RuleKind::Executable, w.exe.clone())
    } else if !w.class.is_empty() {
        (RuleKind::WindowClass, w.class.clone())
    } else {
        (RuleKind::Executable, w.exe.clone())
    };
    Rule::new(kind, value, profile)
}

impl App {
    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    pub(super) fn update_games(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TestRumble(path) => return call_ok(Request::TestRumble(path)),
            Message::ToggleOverlay => return call_ok(Request::ToggleOverlay),
            Message::ToggleNumpad => return call_ok(Request::ToggleNumpad),
            Message::ToggleNumpadAppearance => self.numpad_appearance = !self.numpad_appearance,
            Message::SetNumpadStyle(style) => self.config.numpad_style = style,
            Message::CalibrateGyro(path) => {
                self.message = Some(("Calibrating gyro: keep the controller still for 2 seconds.".into(), false));
                return call_ok(Request::CalibrateGyro(path));
            }
            Message::CopyMotionRuleCommand => {
                self.message = Some(("Copied. Paste it into a terminal and enter your password.".into(), false));
                return iced::clipboard::write(motion_rule_command());
            }
            Message::SelectPage(page) => {
                // Also how a dialog's "Open it" leaves it.
                self.dialog = None;
                match page {
                    Page::Game(key) => self.show_game(key),
                    other => {
                        self.page = other;
                        self.reset_page_state();
                    }
                }
            }
            Message::SetGameSearch(search) => self.game_search = search,
            Message::SelectGameTab(tab) => {
                self.game_tab = tab;
                self.found = None;
            }
            Message::SetSharedView(shared) => {
                self.shared_view = shared;
                self.reset_page_state();
            }
            Message::ToggleSharedSection => self.shared_open = !self.shared_open,
            Message::RenameGame(name) => {
                let Some(old) = self.game_key().map(str::to_string) else { return Task::none() };
                let taken = self.config.games.iter().any(|g| g.name == name && g.name != old);
                if !taken {
                    self.game_mut().name = name.clone();
                    if let Some(d) = self.config.auto_switch.default_profile.as_mut().filter(|d| d.game.as_ref() == Some(&old)) {
                        d.game = Some(name.clone());
                    }
                    let rename = Rename::Game { old, new: name.clone() };
                    self.config.active = follow_renames(std::slice::from_ref(&rename), self.config.active.clone());
                    self.renames.push(rename);
                    self.page = Page::Game(Some(name));
                }
            }
            Message::SetAutoSwitch(on) => self.config.auto_switch.enabled = on,
            Message::SetDefaultProfile(choice) => self.config.auto_switch.default_profile = choice.0,
            Message::AddRule(window) => {
                let profile = self.profile().map(|p| p.name.clone()).unwrap_or_default();
                let rule = match window {
                    Some(w) => rule_for_window(&w, profile),
                    None => Rule::new(RuleKind::Executable, "", profile),
                };
                self.game_mut().rules.push(rule);
            }
            Message::RemoveRule(i) => {
                if i < self.game().rules.len() {
                    self.game_mut().rules.remove(i);
                }
            }
            Message::SetRuleKind(i, kind) => {
                if let Some(r) = self.game_mut().rules.get_mut(i) {
                    r.kind = kind;
                }
            }
            Message::SetRuleValue(i, value) => {
                if let Some(r) = self.game_mut().rules.get_mut(i) {
                    r.value = value;
                }
            }
            Message::SetRuleProfile(i, profile) => {
                if let Some(r) = self.game_mut().rules.get_mut(i) {
                    r.profile = profile;
                }
            }
            Message::SetRuleEnabled(i, on) => {
                if let Some(r) = self.game_mut().rules.get_mut(i) {
                    r.enabled = on;
                }
            }
            Message::SetInfoGlyphs(family) => self.config.info_glyphs = family,
            Message::SetKeyboardStyle(style) => self.config.keyboard_style = style,
            Message::SetOverlayFont(font) => self.config.overlay_font = font,
            Message::SetGameOverlayFont(font) => self.game_mut().overlay_font = font,
            Message::SetGameOverlayStyle(layout, style) => self.set_game_overlay_style(layout, style),
            Message::SetIgnored(name, ignored) => {
                // Applies immediately, independent of unsaved profile edits.
                for c in [&mut self.config, &mut self.saved] {
                    c.ignored_devices.retain(|n| n != &name);
                    if ignored {
                        c.ignored_devices.push(name.clone());
                    }
                }
                return self.push(self.saved.clone());
            }
            other => return self.update_items(other),
        }
        Task::none()
    }

    /// False only when every controller in use has on/off triggers (e.g. Switch pads), in
    /// which case pull-depth settings are hidden. With nothing detected, assume analog.
    pub(super) fn analog_triggers(&self) -> bool {
        let Some(status) = &self.status else { return true };
        let mut in_use = status.devices.iter().filter(|d| !d.ignored).peekable();
        in_use.peek().is_none() || in_use.any(|d| d.analog_triggers)
    }

    /// Whether any managed controller has a gyro (only affects hints, not what can be set).
    pub(super) fn any_gyro(&self) -> bool {
        self.status.as_ref().is_some_and(|s| s.devices.iter().any(|d| d.gyro))
    }

    /// `p` with the layers the daemon says are on right now (they belong to the active game).
    pub(super) fn with_active_layers(&self, p: &Profile) -> Profile {
        let layers = self.status.as_ref().map(|s| s.active_layers.as_slice()).unwrap_or_default();
        p.with_layers(self.config.active_game().layers_named(layers))
    }

    /// Overview and Settings, then General and the games, with a search over the games.
    pub(super) fn view_sidebar(&self) -> Element<'_, Message> {
        let entry = |label: String, page: Page, note: Option<&'static str>| -> Element<'_, Message> {
            let selected = self.page == page;
            let mut line = row![text(label).size(15).width(Length::Fill)].spacing(6).align_y(Alignment::Center);
            if let Some(note) = note {
                line = line.push(text(note).size(12).color(style_accent()));
            }
            button(line)
                .width(Length::Fill)
                .padding([6, 10])
                .style(style::nav(selected))
                .on_press(Message::SelectPage(page))
                .into()
        };
        let active_game = self.config.active_ref().game;
        let playing = |key: &Option<String>| (&active_game == key && self.status.is_some()).then_some("●");
        let updates: Vec<&str> = library::updates(&self.config, &self.library).into_iter().map(|(name, _)| name).collect();

        let mut col = column![
            entry("Overview".into(), Page::Overview, None),
            entry("Settings".into(), Page::Settings, None),
            rule::horizontal(1),
            entry("General".into(), Page::Game(None), playing(&None)),
            text("Games").size(13).color(MUTED_COLOR),
            field("Search games", &self.game_search).on_input(Message::SetGameSearch).size(14),
        ]
        .spacing(6);
        let search = self.game_search.trim().to_lowercase();
        let mut games: Vec<&Game> = self.config.games.iter().filter(|g| g.name.to_lowercase().contains(&search)).collect();
        games.sort_by_key(|g| g.name.to_lowercase());
        for g in games {
            let key = Some(g.name.clone());
            let note = if updates.contains(&g.name.as_str()) { Some("update") } else { playing(&key) };
            let label = if g.name.is_empty() { "(unnamed)".to_string() } else { g.name.clone() };
            col = col.push(entry(label, Page::Game(key), note));
        }
        if self.config.games.is_empty() {
            col = col.push(text("No games yet.").size(13).color(MUTED_COLOR));
        }
        col = col.push(button(text("+ Add game").size(14)).width(Length::Fill).style(style::secondary).on_press(Message::OpenAddGame));
        container(scrollable(col.padding(12)).height(Length::Fill)).width(SIDEBAR_WIDTH).into()
    }

    /// A game's (or General's) page: its name and sub-tabs.
    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    pub(super) fn view_game(&self) -> Element<'_, Message> {
        let general = self.game_key().is_none();
        let game = self.game();
        let names = self.names();
        let mut title = row![text(if general { "General" } else { game.name.as_str() }).size(26)]
            .spacing(12)
            .align_y(Alignment::Center);
        if let Some(note) = origin_note(game) {
            title = title.push(text(note).size(13).color(MUTED_COLOR));
        }
        if general {
            title = title.push(help(
                "Profiles for no particular game, such as the desktop or a plain gamepad. Its \
                 Macros, Menus, Info overlays and Log overlays tabs also hold the shared items every game can use."
                    .into(),
            ));
        }

        let profile_issue = self.profile().is_some_and(|p| ProfileTab::ALL.iter().any(|t| section_has_problem(p, *t, &names)));
        let reachable = reachable_menus(&self.config, (!self.on_shared()).then_some(game));
        let issue = |tab: GameTab| match tab {
            GameTab::Profiles => profile_issue,
            GameTab::Layers => layers_problem(game, &names).is_some(),
            GameTab::Macros => macros_have_problem(self.macros(), &names),
            GameTab::Menus => menus_have_problem(self.menus(), &reachable, &names),
            GameTab::Info => info_has_problem(self.infos()),
            GameTab::Logs => self.logs_have_problem(),
            GameTab::Details => !general && (rules_problem(game).is_some() || game.name.trim().is_empty()),
        };
        let mut segments = row![].spacing(2);
        for tab in GameTab::ALL.into_iter().filter(|t| !(general && *t == GameTab::Details)) {
            let label = if issue(tab) { format!("{tab}  ⚠") } else { tab.to_string() };
            segments = segments.push(
                button(text(label).size(15))
                    .style(style::segment(self.game_tab == tab))
                    .padding([6, 16])
                    .on_press(Message::SelectGameTab(tab)),
            );
        }
        let tab = if general && self.game_tab == GameTab::Details { GameTab::Profiles } else { self.game_tab };
        let mut col = column![title, container(segments).padding(3).style(style::segments)].spacing(16);

        let item_tab = matches!(tab, GameTab::Macros | GameTab::Menus | GameTab::Info | GameTab::Logs);
        if general && item_tab {
            let choice = |label: &'static str, shared: bool| {
                button(text(label).size(14))
                    .style(style::segment(self.shared_view == shared))
                    .padding([5, 14])
                    .on_press(Message::SetSharedView(shared))
            };
            col = col.push(
                row![
                    container(row![choice("General's own", false), choice("Shared by all games", true)].spacing(2))
                        .padding(3)
                        .style(style::segments),
                    help(
                        "Shared macros, menus and info overlays can be used by every profile of every game. \
                         A game's own items can't reuse a shared item's name."
                            .into(),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        let body = match tab {
            GameTab::Profiles => self.view_profile_tab(&names),
            GameTab::Layers => self.view_layers(&names),
            GameTab::Macros => self.view_macros(&names),
            GameTab::Menus => self.view_menus(&names, &reachable),
            GameTab::Info => self.view_infos(),
            GameTab::Logs => self.view_logs(),
            GameTab::Details => self.view_details(),
        };
        col = col.push(body);
        if item_tab && !self.on_shared() {
            col = col.push(self.view_shared_section(match tab {
                GameTab::Macros => ItemKind::Macro,
                GameTab::Menus => ItemKind::Menu,
                GameTab::Logs => ItemKind::Log,
                _ => ItemKind::Info,
            }));
        }
        col.into()
    }

    /// The shared items a game's profiles can also use, read-only.
    pub(super) fn view_shared_section(&self, kind: ItemKind) -> Element<'_, Message> {
        let names = Names::shared(&self.config).list(kind).to_vec();
        let mut col = column![disclosure(
            &format!("Shared {}s usable here ({})", kind.noun(), names.len()),
            self.shared_open,
            Message::ToggleSharedSection
        )]
        .spacing(6);
        if self.shared_open {
            if names.is_empty() {
                col = col.push(text(format!("No shared {}s.", kind.noun())).size(13).color(MUTED_COLOR));
            }
            for name in names {
                col = col.push(text(format!("• {name}")).size(14));
            }
            col = col.push(
                button(text("Edit shared items under General").size(13))
                    .style(button::text)
                    .on_press(Message::SelectPage(Page::Game(None))),
            );
        }
        col.into()
    }

    pub(super) fn view_settings(&self) -> Element<'_, Message> {
        let glyphs = section(
            "Info overlays",
            None,
            vec![labeled(
                "Fallback glyphs",
                row![
                    dropdown(PadFamily::ALL, Some(self.config.info_glyphs), Message::SetInfoGlyphs).width(170),
                    help(
                        "Glyphs follow the controller in use: Xbox, PlayStation or Nintendo labels. For a \
                         controller that can't be recognized, they're drawn like this kind instead. Also \
                         used for previews."
                            .into(),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            )],
        );
        column![self.view_auto_switch(), self.view_font_card(), self.view_keyboard_card(), self.view_numpad_card(), self.view_panic_card(), glyphs]
            .spacing(16)
            .into()
    }

    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    pub(super) fn view_devices(&self) -> Element<'_, Message> {
        let mut list = column![text("Controllers").size(20)].spacing(8);
        match &self.status {
            Some(status) if !status.devices.is_empty() => {
                for d in &status.devices {
                    let state = if d.managed {
                        "remapping"
                    } else if d.ignored {
                        "ignored"
                    } else if !status.enabled {
                        "idle (disabled)"
                    } else {
                        "not grabbed"
                    };
                    let state = if d.analog_triggers { state.to_string() } else { format!("{state} · digital triggers") };
                    let state = if d.rumble { state } else { format!("{state} · no rumble") };
                    let state = if d.gyro { format!("{state} · gyro") } else { state };
                    let calibrate: Element<'_, Message> = if d.gyro {
                        button(text("Calibrate gyro").size(13))
                            .style(style::secondary)
                            .on_press(Message::CalibrateGyro(d.path.clone()))
                            .into()
                    } else {
                        space().into()
                    };
                    let name = d.name.clone();
                    let rumble: Element<'_, Message> = if d.rumble {
                        button(text("Test rumble").size(13))
                            .style(style::secondary)
                            .on_press(Message::TestRumble(d.path.clone()))
                            .into()
                    } else {
                        space().into()
                    };
                    list = list.push(
                        row![
                            column![text(&d.name), text(format!("{} · {state}", d.path)).size(12).color(MUTED_COLOR)]
                                .width(Length::Fill),
                            calibrate,
                            rumble,
                            checkbox(!d.ignored)
                                .label("Manage")
                                .on_toggle(move |on| Message::SetIgnored(name.clone(), !on)),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    );
                }
            }
            Some(_) => list = list.push(text("No controllers detected.").color(MUTED_COLOR)),
            None => list = list.push(text("Unavailable while the daemon is not running.").color(MUTED_COLOR)),
        }
        if let Some(status) = &self.status
            && !status.motion_access_denied.is_empty()
        {
            let names = status.motion_access_denied.join(", ");
            list = list.push(
                column![
                    text(format!(
                        "Can't use the gyro on {names}: this app isn't allowed to read its motion sensors. \
                         Installing a udev rule fixes this (it asks for your password once):"
                    ))
                    .size(12)
                    .color(ERROR_COLOR),
                    row![
                        text(motion_rule_command()).size(11).font(iced::Font::MONOSPACE).width(Length::Fill),
                        button(text("Copy command").size(13))
                            .style(style::secondary)
                            .on_press(Message::CopyMotionRuleCommand),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
            );
        }
        list.into()
    }

    pub(super) fn view_auto_switch(&self) -> Element<'_, Message> {
        let auto = &self.config.auto_switch;
        let how = match self.status.as_ref().map(|s| s.focus_backend) {
            Some(FocusBackend::Kwin) => "Profiles switch as the focused window changes (KWin).",
            Some(FocusBackend::ProcessScan) => {
                "Window tracking isn't available on this desktop, so rules apply while a matching \
                 process is running."
            }
            None => "Automatic switching needs the daemon to be running.",
        };
        let mut defaults = vec![DefaultChoice(None)];
        defaults.extend(
            self.config
                .all_games()
                .flat_map(|(key, g)| g.profiles.iter().map(move |p| DefaultChoice(Some(ProfileRef::new(key, &p.name))))),
        );
        let default = DefaultChoice(auto.default_profile.clone());
        section(
            "Per-game profiles",
            Some("Each game's rules (on its Details tab) say which windows switch to which of its profiles.".into()),
            vec![
                row![
                    toggler(auto.enabled).label("Switch profiles automatically").on_toggle(Message::SetAutoSwitch),
                    space::horizontal(),
                    text("Outside games use"),
                    dropdown(defaults, Some(default), Message::SetDefaultProfile).width(280),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
                text(how).size(13).color(MUTED_COLOR).into(),
            ],
        )
    }

    /// A game's name, rules, pack details, export and deletion.
    #[expect(clippy::too_many_lines, clippy::cognitive_complexity, reason = "predates the size lints")]
    pub(super) fn view_details(&self) -> Element<'_, Message> {
        let game = self.game();
        let names: Vec<String> = game.profiles.iter().map(|p| p.name.clone()).collect();
        let mut rules = column![].spacing(8);
        for (i, r) in game.rules.iter().enumerate() {
            let placeholder = match r.kind {
                RuleKind::Executable => "e.g. eldenring.exe",
                RuleKind::SteamAppId => "e.g. 1245620",
                RuleKind::WindowClass => "e.g. steam_app_1245620",
            };
            let mut line = row![
                checkbox(r.enabled).on_toggle(move |on| Message::SetRuleEnabled(i, on)),
                text("When").size(14),
                dropdown(RuleKind::ALL, Some(r.kind), move |k| Message::SetRuleKind(i, k)).width(150),
                text("is").size(14),
                field(placeholder, &r.value).on_input(move |v| Message::SetRuleValue(i, v)).width(200),
                text("use").size(14),
                dropdown(names.clone(), Some(r.profile.clone()), move |p| Message::SetRuleProfile(i, p)).width(160),
                button(text("✕").size(13)).style(style::secondary).on_press(Message::RemoveRule(i)),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            if !names.contains(&r.profile) {
                line = line.push(text("missing profile").size(12).color(ERROR_COLOR));
            }
            if let Some(other) = self.config.games.iter().find(|g| g.name != game.name && g.rules.iter().any(|o| o.enabled && o.same_match(r))) {
                line = line.push(text(format!("also in {}", other.name)).size(12).color(MUTED_COLOR));
            }
            rules = rules.push(line);
        }
        if game.rules.is_empty() {
            rules = rules.push(text("No rules: this game is only used when picked by hand.").size(13).color(MUTED_COLOR));
        }
        rules = rules.push(button(text("+ Add rule").size(13)).style(style::secondary).on_press(Message::AddRule(None)));
        // This app's own window is never a game.
        let recent: Vec<_> =
            self.status.iter().flat_map(|s| &s.recent_windows).filter(|w| w.exe != env!("CARGO_PKG_NAME")).collect();
        if !recent.is_empty() {
            let mut list = column![
                text("Recently focused windows: \"+ Rule\" adds a rule for the profile you're editing on the Profiles tab").size(13).color(MUTED_COLOR)
            ]
            .spacing(6);
            for w in recent {
                let mut details = Vec::new();
                if !w.exe.is_empty() {
                    details.push(w.exe.clone());
                }
                if let Some(id) = &w.steam_app_id {
                    details.push(format!("Steam {id}"));
                }
                if !w.class.is_empty() {
                    details.push(format!("class {}", w.class));
                }
                // Untitled windows (e.g. plasmashell) go by their executable instead.
                let name = [&w.title, &w.exe, &w.class].into_iter().find(|s| !s.is_empty()).map_or("Untitled window", |s| s.as_str());
                let title: String = name.chars().take(60).collect();
                list = list.push(
                    row![
                        column![text(title).size(14), text(details.join(" · ")).size(12).color(MUTED_COLOR)].width(Length::Fill),
                        button(text("+ Rule").size(13)).style(style::secondary).on_press(Message::AddRule(Some(w.clone()))),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
            rules = rules.push(list);
        }

        let mut pack_rows: Vec<Element<'_, Message>> = Vec::new();
        if let Some(origin) = &game.origin {
            let source = if origin.library { "the built-in library" } else { "a pack file" };
            let by = if game.pack.author.is_empty() { String::new() } else { format!(" by {}", game.pack.author) };
            pack_rows.push(text(format!("Added from {source}: version {}{by}.", origin.version)).into());
            if !self.edited.is_empty() {
                pack_rows.push(text(format!("Changed since: {}.", self.edited.join(", "))).size(13).color(MUTED_COLOR).into());
            }
            let update = library::updates(&self.config, &self.library).into_iter().find(|(n, _)| *n == game.name);
            if let Some(entry) = update.map(|(_, i)| &self.library[i]) {
                pack_rows.push(
                    row![
                        text(format!("The library has version {}.", entry.pack.pack.version)).color(style_accent()),
                        button(text("Update…").size(13)).on_press(Message::UpdateFromLibrary(game.name.clone())),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                );
            }
        } else if !game.pack.id.is_empty() {
            pack_rows.push(text(format!("You've exported this game (version {}).", game.pack.version)).into());
        } else {
            pack_rows.push(text("Export this game to share it as a .padpack file.").size(13).color(MUTED_COLOR).into());
        }
        if let Some(b) = &game.pack.based_on {
            pack_rows.push(text(format!("Based on {} {} by {}.", b.name, b.version, b.author)).size(13).color(MUTED_COLOR).into());
        }
        if !game.pack.description.is_empty() {
            pack_rows.push(text(game.pack.description.clone()).size(13).into());
        }
        pack_rows.push(
            row![
                button(text("Export…")).on_press(Message::OpenExport),
                space::horizontal(),
                button(text("Delete game")).style(button::danger).on_press(Message::AskDeleteGame),
            ]
            .spacing(8)
            .into(),
        );

        column![
            labeled(
                "Name",
                field("Game name", &game.name).on_input(Message::RenameGame).width(260).into(),
            ),
            section(
                "Auto-switch rules",
                Some("When the focused window (or a running process) matches a rule, its profile becomes active. Untick a rule to switch it off.".into()),
                vec![rules.into()],
            ),
            self.view_game_overlays(),
            section("Pack", None, pack_rows),
        ]
        .spacing(16)
        .into()
    }

    pub(super) fn view_keyboard_card(&self) -> Element<'_, Message> {
        let keyboard_open = self.status.as_ref().is_some_and(|s| s.overlay_visible);
        let appearance_open = self.open_appearance.contains(&None);
        let mut rows: Vec<Element<'_, Message>> = vec![
            row![
                button(text(if keyboard_open { "Close it" } else { "Open it now" }).size(14))
                    .style(style::secondary)
                    .on_press(Message::ToggleOverlay),
                disclosure("Appearance", appearance_open, Message::ToggleAppearance(None)),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
        ];
        if appearance_open {
            let style = &self.config.keyboard_style;
            rows.push(style_editor(style, Rc::new(Message::SetKeyboardStyle)));
            let sample = crate::overlay::KeyboardView {
                layout: Layout::Keyboard,
                style: preview_style(style),
                cursor: crate::keyboard::find(Layout::Keyboard, "KEY_H").unwrap_or_default(),
                latched: vec!["KEY_LEFTSHIFT".into()],
                pressed: None,
                closing: 0.0,
            };
            rows.push(preview(crate::overlay::draw::keyboard_panel(&sample, self.preview_font())));
        }
        section(
            "On-screen keyboard",
            Some(
                "A keyboard over everything, typed with the controller: D-pad or stick to move, A to \
                 press, X backspace, Y space, Start enter, hold LT for Shift, hold B to close. Map \"On-screen keyboard\" \
                 to a button or gesture to open it from the controller."
                    .into(),
            ),
            rows,
        )
    }

    pub(super) fn view_numpad_card(&self) -> Element<'_, Message> {
        let numpad_open = self.status.as_ref().is_some_and(|s| s.numpad_visible);
        let mut rows: Vec<Element<'_, Message>> = vec![
            row![
                button(text(if numpad_open { "Close it" } else { "Open it now" }).size(14))
                    .style(style::secondary)
                    .on_press(Message::ToggleNumpad),
                disclosure("Appearance", self.numpad_appearance, Message::ToggleNumpadAppearance),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
        ];
        if self.numpad_appearance {
            let style = &self.config.numpad_style;
            rows.push(style_editor(style, Rc::new(Message::SetNumpadStyle)));
            let sample = crate::overlay::KeyboardView {
                layout: Layout::Numpad,
                style: preview_style(style),
                cursor: Layout::Numpad.home(),
                latched: Vec::new(),
                pressed: None,
                closing: 0.0,
            };
            rows.push(preview(crate::overlay::draw::keyboard_panel(&sample, self.preview_font())));
        }
        section(
            "On-screen numpad",
            Some(
                "Digits 0–9 and a dot, typed with the controller: D-pad or stick to move, A to press, \
                 X backspace, Start enter, hold B to close. Map \"On-screen numpad\" to a button or \
                 gesture to open it from the controller."
                    .into(),
            ),
            rows,
        )
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::tests::*;

    #[test]
    fn rules_from_windows_prefer_the_most_specific_identifier() {
        let w = |class: &str, exe: &str, steam: Option<&str>| WindowInfo {
            class: class.into(),
            exe: exe.into(),
            steam_app_id: steam.map(Into::into),
            ..Default::default()
        };
        let pick = |w: WindowInfo| {
            let r = rule_for_window(&w, "P".into());
            (r.kind, r.value)
        };
        assert_eq!(pick(w("steam_app_1", "game.exe", Some("1"))), (RuleKind::SteamAppId, "1".into()));
        assert_eq!(pick(w("wine", "game.exe", None)), (RuleKind::Executable, "game.exe".into()));
        // Native exe names can be generic (java, python), so the class is preferred.
        assert_eq!(pick(w("Minecraft", "java", None)), (RuleKind::WindowClass, "Minecraft".into()));
        assert_eq!(pick(w("", "factorio", None)), (RuleKind::Executable, "factorio".into()));
    }

    #[test]
    fn rules_must_point_at_existing_profiles() {
        let mut app = with_game();
        let _ = app.update(Message::AddRule(None));
        assert!(app.validate().unwrap().contains("no Executable to match"), "{:?}", app.validate());
        let _ = app.update(Message::SetRuleValue(1, "x".into()));
        let _ = app.update(Message::SetRuleProfile(1, "Gone".into()));
        let err = app.validate().unwrap();
        assert!(err.starts_with("Doom: ") && err.contains("missing profile"), "{err}");
        let _ = app.update(Message::RemoveRule(1));
        let _ = app.update(Message::SetRuleEnabled(0, false));
        assert!(!app.game().rules[0].enabled);
        assert_eq!(app.validate(), None);
    }

    #[test]
    fn motion_rule_command_installs_the_shipped_rule() {
        let cmd = motion_rule_command();
        assert!(cmd.contains("ENV{ID_INPUT_ACCELEROMETER}==\"1\""), "{cmd}");
        assert!(cmd.contains("TAG+=\"uaccess\""), "{cmd}");
        // Single-quoted for the shell, so the rule itself must not contain a quote.
        let quoted = cmd.split('\'').nth(1).unwrap();
        assert!(!quoted.is_empty() && !quoted.contains('#'), "comments are left out: {quoted}");
        assert_eq!(cmd.matches('\'').count(), 2, "{cmd}");
    }

    #[test]
    fn games_are_renamed_deleted_and_added_empty() {
        let mut app = with_game();
        app.config.auto_switch.default_profile = Some(ProfileRef::new(Some("Doom"), "Menus"));
        let _ = app.update(Message::RenameGame("Doom II".into()));
        assert_eq!(app.page, Page::Game(Some("Doom II".into())));
        assert_eq!(app.config.auto_switch.default_profile, Some(ProfileRef::new(Some("Doom II"), "Menus")));
        assert_eq!(app.game().name, "Doom II");

        let _ = app.update(Message::AskDeleteGame);
        assert!(matches!(app.dialog, Some(Dialog::DeleteGame(ref n)) if n == "Doom II"));
        let _ = app.update(Message::ConfirmDeleteGame);
        assert!(app.config.games.is_empty() && app.dialog.is_none());
        assert_eq!(app.page, Page::Game(None));
        assert_eq!(app.config.auto_switch.default_profile, None);

        let _ = app.update(Message::AddEmptyGame(Template::Strategy));
        assert_eq!(app.page, Page::Game(Some("New game".into())));
        assert_eq!(app.game().profiles[0].name, "Strategy");
        assert_eq!(app.game_tab, GameTab::Details);
        let _ = app.update(Message::AddEmptyGame(Template::Gamepad));
        assert_eq!(app.config.games[1].name, "New game 2");
    }
}

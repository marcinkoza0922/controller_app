//! The sidebar, and a game's page and Details tab.

use iced::widget::{column, row};

use super::*;
use super::overlays::{motion_guide, motion_rows};

/// "from <pack> 1.2 by <author>" for a game made from a pack.
pub(super) fn origin_note(game: &Game) -> Option<String> {
    let origin = game.origin.as_ref()?;
    let by = if game.pack.author.is_empty() { String::new() } else { format!(" by {}", game.pack.author) };
    let source = if origin.library { "library" } else { "pack" };
    Some(format!("from {source}, version {}{by}", origin.version))
}

impl App {
    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    pub(super) fn update_games(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TestRumble(path) => return call_ok(Request::TestRumble(path)),
            Message::TestTurn(px) => {
                self.message = Some(("Switch to your game: the mouse turns right in 3 seconds.".into(), false));
                return call_ok(Request::TestTurn(px));
            }
            Message::ToggleOverlay => return call_ok(Request::ToggleOverlay),
            Message::ToggleNumpad => return call_ok(Request::ToggleNumpad),
            Message::ToggleNumpadAppearance => self.numpad_appearance = !self.numpad_appearance,
            Message::ToggleSoundCues(overlay) => {
                if !self.open_sounds.remove(&overlay) {
                    self.open_sounds.insert(overlay);
                }
            }
            Message::SetNumpadStyle(style) => self.config.numpad_style = style,
            Message::ToggleMediaAppearance => self.media_appearance = !self.media_appearance,
            Message::SetMediaStyle(style) => self.config.media_style = style,
            Message::ToggleInGameMenuAppearance => self.menu_appearance = !self.menu_appearance,
            Message::SetInGameMenuStyle(style) => self.config.menu_style = style,
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
            Message::SelectChapter(chapter) => self.manual_chapter = chapter,
            Message::ManualLink(link) => {
                if let Some(chapter) = manual::chapter_of(&link) {
                    self.manual_chapter = chapter;
                }
            }
            Message::SystemMode(mode) => self.theme_mode = mode,
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
                    Some(w) => crate::focus::rule_for_window(&w, profile),
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
            Message::SetAppearance(appearance) => self.config.appearance = appearance,
            Message::SetNintendoLayout(on) => self.config.nintendo_layout = on,
            Message::SetGameNintendoLayout(choice) => self.game_mut().nintendo_layout = choice,
            Message::SetGameControllers(support) => self.game_mut().controllers = Some(support),
            Message::SetGameSounds(sounds) => self.game_mut().sounds = Some(sounds),
            Message::SetSounds(sounds) => self.config.sounds = sounds,
            Message::ClearGameSounds => self.game_mut().sounds = None,
            Message::PreviewSound(spec) => self.sounds.play(&spec),
            Message::SetKeyboardStyle(style) => self.config.keyboard_style = style,
            Message::SetOverlayFont(font) => self.config.overlay_font = font,
            Message::SetColorblindTones(on) => self.config.colorblind_tones = on,
            Message::SetMotion(kind, style) => self.config.motion.set(kind, style),
            Message::SetGameMotion(kind, style) => {
                let mut set = self.game().motion.unwrap_or(self.config.motion);
                set.set(kind, style);
                self.game_mut().motion = Some(set);
            }
            Message::OwnGameMotion(on) => self.game_mut().motion = on.then_some(self.config.motion),
            Message::SetGameOverlayFont(font) => self.game_mut().overlay_font = font,
            Message::SetGameOverlayStyle(layout, style) => self.set_game_overlay_style(layout, style),
            Message::SetGameMediaStyle(style) => self.game_mut().media_style = style,
            Message::SetGameMenuStyle(style) => self.game_mut().menu_style = style,
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

    /// Whether any managed controller has back paddles (only affects hints).
    pub(super) fn any_paddles(&self) -> bool {
        self.status.as_ref().is_some_and(|s| s.devices.iter().any(|d| d.paddles))
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
            entry("App settings".into(), Page::Settings, None),
            entry("Manual".into(), Page::Manual, None),
            rule::horizontal(1),
            text("Setups").size(13).color(MUTED_COLOR),
            entry("General".into(), Page::Game(None), playing(&None)),
        ]
        .spacing(6);
        // General is always listed, so the search box and the empty note are about the games.
        if self.config.games.is_empty() {
            col = col.push(text("No game setups yet. Use + Add setup to add one.").size(13).color(MUTED_COLOR));
        } else {
            col = col.push(field("Search setups", &self.game_search).on_input(Message::SetGameSearch).size(14));
        }
        let search = self.game_search.trim().to_lowercase();
        let mut games: Vec<&Game> = self.config.games.iter().filter(|g| g.name.to_lowercase().contains(&search)).collect();
        games.sort_by_key(|g| g.name.to_lowercase());
        for g in games {
            let key = Some(g.name.clone());
            let note = if updates.contains(&g.name.as_str()) { Some("update") } else { playing(&key) };
            let label = if g.name.is_empty() { "(unnamed)".to_string() } else { g.name.clone() };
            col = col.push(entry(label, Page::Game(key), note));
        }
        col = col.push(button(text("+ Add setup").size(14)).width(Length::Fill).style(style::secondary).on_press(Message::OpenAddGame));
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
        let family = self.glyph_family();
        let mut segments = row![].spacing(2);
        for tab in GameTab::ALL.into_iter().filter(|t| !(general && *t == GameTab::Details)) {
            let label = if issue(tab) { format!("{tab}  ⚠") } else { tab.to_string() };
            segments = segments.push(
                button(row![tab.glyph(family), text(label).size(15)].spacing(6).align_y(Alignment::Center))
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
                    container(row![choice("General's own", false), choice("Shared by all setups", true)].spacing(2))
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

    /// The setup's overlay sounds: each overlay's cues, on or off, and what its steps and picks sound like.
    /// Its own sounds start as a copy of the App settings' ones.
    fn view_game_sounds(&self) -> Element<'_, Message> {
        let own = self.game().sounds;
        let global = self.config.sounds;
        let mut rows = own_rows("sounds", own.is_some(), move |on| if on { Message::SetGameSounds(global) } else { Message::ClearGameSounds });
        if let Some(set) = own {
            rows.extend(SoundOverlay::ALL.iter().map(|&overlay| overlay_sounds(set, overlay, self.open_sounds.contains(&overlay), Message::SetGameSounds)));
            rows.push(button(text("Turn every sound off").size(13)).style(style::secondary).on_press(Message::SetGameSounds(set.silenced())).into());
        }
        section(
            "Overlay sounds",
            Some("A faint sound as the cursor moves in an overlay, another when something is picked, and a softer one as it pops in and out. A setup's own sounds start as a copy of the ones on App settings. Any overlay can be turned off.".into()),
            rows,
        )
    }

    /// The setup's own overlay motion: each kind of overlay, starting as a copy of the global set.
    fn view_game_motion(&self) -> Element<'_, Message> {
        let own = self.game().motion;
        let mut rows = own_rows("motion", own.is_some(), Message::OwnGameMotion);
        if let Some(set) = own {
            rows.extend(motion_rows(set, Message::SetGameMotion));
        }
        section(
            "Overlay motion",
            Some(format!("How this setup's overlays move, while it is active. Each kind of overlay has its own style; a setup's own motion starts as a copy of the one on App settings. {}", motion_guide())),
            rows,
        )
    }

    /// Which other controllers a setup supports: a physical one of a ticked kind makes the pad
    /// present itself as that kind while the setup is active.
    fn view_game_controllers(&self) -> Element<'_, Message> {
        let support = self.game().controllers.unwrap_or_default();
        let toggle = |label: &'static str, on: bool, set: fn(&mut crate::pad_identity::ControllerSupport, bool)| {
            checkbox(on).label(label).on_toggle(move |value| {
                let mut next = support;
                set(&mut next, value);
                Message::SetGameControllers(next)
            })
        };
        section(
            "Controllers this setup supports",
            Some("Tick a kind when the game has explicit support for it. While the game is active, a controller of that kind is seen by the game as that kind, with its own buttons and glyphs. Other controllers, and kinds left unticked, are seen as an Xbox controller.".into()),
            vec![
                toggle("DualShock 4", support.dualshock, |s, on| s.dualshock = on).into(),
                toggle("DualSense", support.dualsense, |s, on| s.dualsense = on).into(),
                toggle("Switch Pro Controller", support.switch_pro, |s, on| s.switch_pro = on).into(),
            ],
        )
    }

    /// A game's own choice of the Nintendo layout, or the one App settings gives.
    fn view_game_nintendo_layout(&self) -> Element<'_, Message> {
        let own = self.game().nintendo_layout;
        let global = self.config.nintendo_layout;
        let mut rows = own_rows("button layout", own.is_some(), move |on| Message::SetGameNintendoLayout(on.then_some(global)));
        if let Some(on) = own {
            rows.push(toggler(on).label("Nintendo button layout").on_toggle(|on| Message::SetGameNintendoLayout(Some(on))).into());
        }
        let settings = if global { "on" } else { "off" };
        section(
            "Button layout",
            Some(format!("Nintendo layout for this setup's glyphs. On the App settings page it is currently {settings}.")),
            rows,
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
            rules = rules.push(text("No rules yet. This setup only starts when you pick it yourself.").size(13).color(MUTED_COLOR));
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
            pack_rows.push(text(format!("You've exported this setup (version {}).", game.pack.version)).into());
        } else {
            pack_rows.push(text("Export this setup to share it as a .padpack file.").size(13).color(MUTED_COLOR).into());
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
                button(text("Delete setup")).style(button::danger).on_press(Message::AskDeleteGame),
            ]
            .spacing(8)
            .into(),
        );

        column![
            section(
                "Setup",
                None,
                vec![labeled("Name", field("Setup name", &game.name).on_input(Message::RenameGame).width(260).into())],
            ),
            section(
                "Auto-switch rules",
                Some("A rule switches to its profile when a matching window has focus, or a matching program is running. Untick a rule to turn it off.".into()),
                vec![rules.into()],
            ),
            self.view_game_overlays(),
            self.view_game_nintendo_layout(),
            self.view_game_controllers(),
            self.view_game_sounds(),
            self.view_game_motion(),
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
            rows.push(preview(crate::overlay::draw::keyboard_panel(&sample, self.preview_font(), &crate::overlay::fit::Fit::contents(), &crate::motion::Anim::still())));
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

    pub(super) fn view_media_card(&self) -> Element<'_, Message> {
        let mut rows: Vec<Element<'_, Message>> = vec![disclosure("Appearance", self.media_appearance, Message::ToggleMediaAppearance)];
        if self.media_appearance {
            let style = &self.config.media_style;
            rows.push(style_editor(style, Rc::new(Message::SetMediaStyle)));
            let sample = crate::media::MediaView::sample(preview_style(style));
            rows.push(preview(crate::overlay::draw::media_panel(&sample, self.preview_font(), &crate::overlay::fit::Fit::contents(), &crate::motion::Anim::still())));
        }
        section(
            "Media controls",
            Some(
                "The panel for the player you're listening to, shown over the game with Guide + LB. \
                 Pick where it sits and how it looks."
                    .into(),
            ),
            rows,
        )
    }

    pub(super) fn view_in_game_menu_card(&self) -> Element<'_, Message> {
        let mut rows: Vec<Element<'_, Message>> = vec![disclosure("Appearance", self.menu_appearance, Message::ToggleInGameMenuAppearance)];
        if self.menu_appearance {
            let style = &self.config.menu_style;
            rows.push(style_editor(style, Rc::new(Message::SetInGameMenuStyle)));
            let sample = crate::system_menu::main_page(preview_style(style), 0);
            rows.push(preview(crate::overlay::draw::menu_panel(&sample, self.preview_font(), self.menu_look(), &crate::overlay::fit::Fit::contents())));
        }
        section(
            "In-game menu",
            Some(
                "The menu Guide + Start opens over a game, with Quick Settings and Edit Controls. Edit \
                 Controls uses this look too. Pick where it sits and how it looks."
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
            rows.push(preview(crate::overlay::draw::keyboard_panel(&sample, self.preview_font(), &crate::overlay::fit::Fit::contents(), &crate::motion::Anim::still())));
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

/// One overlay's sounds: on or off, and (when on and opened) its step, pick, open and close cues.
pub(super) fn overlay_sounds<'a>(set: SoundSet, overlay: SoundOverlay, open: bool, change: fn(SoundSet) -> Message) -> Element<'a, Message> {
    let current = set.get(overlay);
    let put = move |sounds: OverlaySounds| {
        let mut next = set;
        next.set(overlay, sounds);
        change(next)
    };
    let toggle = toggler(current.enabled).label(overlay.label()).on_toggle(move |on| put(OverlaySounds { enabled: on, ..current }));
    let mut head = row![container(toggle).width(220)].align_y(Alignment::Center);
    if current.enabled {
        head = head.push(disclosure("Adjust", open, Message::ToggleSoundCues(overlay)));
    }
    let mut block = column![head].spacing(6);
    if current.enabled && open {
        block = block.push(cue_row("Step", current.step, move |step| put(OverlaySounds { step, ..current })));
        block = block.push(cue_row("Pick", current.pick, move |pick| put(OverlaySounds { pick, ..current })));
        block = block.push(cue_row("Open", current.open, move |open| put(OverlaySounds { open, ..current })));
        block = block.push(cue_row("Close", current.close, move |close| put(OverlaySounds { close, ..current })));
    }
    block.into()
}

/// One cue on one line: its label, kind and preview, then its volume, pitch and length.
fn cue_row<'a>(label: &'static str, spec: SoundSpec, put: impl Fn(SoundSpec) -> Message + Copy + 'a) -> Element<'a, Message> {
    let knob = |name: &'static str, value: f32, range: std::ops::RangeInclusive<f32>, set: fn(&mut SoundSpec, f32)| {
        row![
            text(name).width(56),
            slider(range, value, move |v| {
                let mut next = spec;
                set(&mut next, v);
                put(next)
            })
            .step(5.0_f32)
            .width(90),
            text(format!("{value:.0}%")).size(13).color(MUTED_COLOR).width(40),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    };
    let line = row![
        text(label).width(48),
        dropdown(SoundKind::ALL, Some(spec.kind), move |kind| put(SoundSpec { kind, ..spec })).width(110),
        button(text("Play")).style(style::secondary).on_press(Message::PreviewSound(spec)),
        space().width(8),
        knob("Volume", spec.volume * 100.0, 0.0..=100.0, |s, v| s.volume = v / 100.0),
        knob("Pitch", spec.pitch * 100.0, 50.0..=200.0, |s, v| s.pitch = v / 100.0),
        knob("Length", spec.length * 100.0, 50.0..=200.0, |s, v| s.length = v / 100.0),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .wrap();
    container(line).padding(iced::Padding::ZERO.left(28)).into()
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
            let r = crate::focus::rule_for_window(&w, "P".into());
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
        assert!(err.starts_with("Doom: ") && err.contains("doesn't exist"), "{err}");
        let _ = app.update(Message::RemoveRule(1));
        let _ = app.update(Message::SetRuleEnabled(0, false));
        assert!(!app.game().rules[0].enabled);
        assert_eq!(app.validate(), None);
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
        assert_eq!(app.page, Page::Game(Some("New setup".into())));
        assert_eq!(app.game().profiles[0].name, "Strategy");
        assert_eq!(app.game_tab, GameTab::Details);
        let _ = app.update(Message::AddEmptyGame(Template::Gamepad));
        assert_eq!(app.config.games[1].name, "New setup 2");
    }
}

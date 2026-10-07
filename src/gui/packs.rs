//! Dialogs for games as packs: the library picker ("+ Add game"), the import preview,
//! export, deleting a game, and copying items from another game.

use std::path::PathBuf;

use iced::widget::{column, row};

use super::*;
use crate::{
    config::PackInfo,
    pack::{Choices, Plan},
};

pub(super) enum Dialog {
    AddGame { search: String, selected: Option<usize> },
    Import { plan: Box<Plan>, choices: Choices },
    /// Export of the game with this name, and what it would write (kept up to date as the
    /// details are edited, rather than worked out every frame).
    Export { game: String, info: PackInfo, library: bool, preview: Box<pack::Export> },
    DeleteGame(String),
    /// Copy a macro, menu or info overlay from another game into the shown list.
    Browse { kind: ItemKind, from: Option<BrowseSource>, open: Option<usize> },
}

/// Where "Copy from another game…" takes items from.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum BrowseSource {
    /// A game (`None`: General).
    Game(Option<String>),
    /// A library pack not added yet, by index in `App::library`.
    Library(usize, String),
}

impl fmt::Display for BrowseSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrowseSource::Game(None) => f.write_str("General"),
            BrowseSource::Game(Some(name)) => f.write_str(name),
            BrowseSource::Library(_, name) => write!(f, "{name} (library)"),
        }
    }
}

/// Editable pack metadata in the export dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PackField {
    Version,
    Author,
    Description,
    MadeWith,
}

/// The library picker lists at most this many packs when nothing is searched for.
const LIBRARY_LIST_HEIGHT: f32 = 380.0;

impl App {
    #[expect(clippy::too_many_lines, clippy::cognitive_complexity, reason = "predates the size lints")]
    pub(super) fn update_packs(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenAddGame => {
                self.dialog = Some(Dialog::AddGame { search: String::new(), selected: None });
                if self.installed.is_none() {
                    return Task::perform(
                        async { tokio::task::spawn_blocking(launchers::scan).await.unwrap_or_default() },
                        Message::Installed,
                    );
                }
            }
            Message::Installed(installed) => self.installed = Some(installed),
            Message::CloseDialog => self.dialog = None,
            Message::SetLibrarySearch(text) => {
                if let Some(Dialog::AddGame { search, .. }) = &mut self.dialog {
                    *search = text;
                }
            }
            Message::SelectLibrary(i) => {
                if let Some(Dialog::AddGame { selected, .. }) = &mut self.dialog {
                    *selected = Some(i);
                }
            }
            Message::AddEmptyGame(template) => {
                let name = unique_name("New game", |n| self.config.games.iter().any(|g| g.name == n));
                let profile = template.make(template.base_name());
                self.config.games.push(Game::new(&name, vec![profile]));
                self.dialog = None;
                self.show_game(Some(name));
                self.game_tab = GameTab::Details;
                self.message = Some(("Name the game and add a rule for it, then Save & apply.".into(), false));
            }
            Message::PreviewLibrary(i) => {
                if let Some(entry) = self.library.get(i) {
                    let plan = pack::plan(&self.config, entry.pack.clone(), true);
                    self.open_import(plan);
                }
            }
            Message::UpdateFromLibrary(name) => {
                let found = library::updates(&self.config, &self.library).into_iter().find(|(n, _)| *n == name).map(|(_, i)| i);
                if let Some(i) = found {
                    let plan = pack::plan(&self.config, self.library[i].pack.clone(), true);
                    self.open_import(plan);
                }
            }
            Message::ImportFile => return Task::perform(pick_pack(), Message::PackFileRead),
            Message::PackFileRead(None) => {}
            Message::PackFileRead(Some(read)) => match read.map_err(anyhow::Error::msg).and_then(|text| pack::parse(&text)) {
                Ok(pack) => {
                    let plan = pack::plan(&self.config, pack, false);
                    self.open_import(plan);
                }
                Err(e) => {
                    self.dialog = None;
                    self.message = Some((format!("Can't import that pack: {e:#}"), true));
                }
            },
            Message::SetReplaceGame(replace) => {
                if let Some(Dialog::Import { choices, .. }) = &mut self.dialog {
                    choices.replace = replace;
                }
            }
            Message::SetKeepMine(c, keep) => {
                if let Some(Dialog::Import { choices, .. }) = &mut self.dialog
                    && let Some(slot) = choices.keep_mine.get_mut(c)
                {
                    *slot = keep;
                }
            }
            Message::ConfirmImport => {
                if let Some(Dialog::Import { plan, choices }) = self.dialog.take() {
                    let name = pack::apply(&mut self.config, &plan, &choices);
                    self.show_game(Some(name.clone()));
                    self.message = Some((format!("Added {name}. Save & apply to start using it."), false));
                }
            }
            Message::OpenExport => {
                if let Some(name) = self.game_key().map(str::to_string) {
                    let info = pack::draft(self.game(), false);
                    let preview = Box::new(pack::export(self.game(), &self.config.shared, &info));
                    self.dialog = Some(Dialog::Export { game: name, info, library: false, preview });
                }
            }
            Message::SetPackField(field, value) => {
                if let Some(Dialog::Export { info, .. }) = &mut self.dialog {
                    *match field {
                        PackField::Version => &mut info.version,
                        PackField::Author => &mut info.author,
                        PackField::Description => &mut info.description,
                        PackField::MadeWith => &mut info.made_with,
                    } = value;
                }
                self.refresh_export();
            }
            Message::SetLibraryExport(on) => {
                let game = match &self.dialog {
                    Some(Dialog::Export { game, .. }) => game.clone(),
                    _ => return Task::none(),
                };
                let fresh = self.config.game(Some(&game)).map(|g| pack::draft(g, on));
                if let (Some(Dialog::Export { info, library, .. }), Some(fresh)) = (&mut self.dialog, fresh) {
                    *info = PackInfo { description: info.description.clone(), made_with: info.made_with.clone(), ..fresh };
                    *library = on;
                }
                self.refresh_export();
            }
            Message::SaveExport => {
                let Some(Dialog::Export { game, library, preview, .. }) = &self.dialog else { return Task::none() };
                let text = match preview.pack.to_toml() {
                    Ok(text) => text,
                    Err(e) => {
                        self.message = Some((format!("Can't export: {e:#}"), true));
                        return Task::none();
                    }
                };
                // The maintainer saves library packs into the repository's packs/ folder.
                let dir = (*library && cfg!(debug_assertions)).then(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packs"));
                return Task::perform(save_pack(file_stem(game), text, dir), Message::Exported);
            }
            Message::Exported(None) => {}
            Message::Exported(Some(Ok(path))) => {
                if let Some(Dialog::Export { game, info, .. }) = self.dialog.take()
                    && let Some(g) = self.config.game_mut(Some(&game))
                {
                    // Remembered for the next export of this game.
                    g.pack = info;
                }
                self.message = Some((format!("Exported to {path}. Save & apply to remember its details."), false));
            }
            Message::Exported(Some(Err(e))) => self.message = Some((format!("Can't export: {e}"), true)),
            Message::AskDeleteGame => {
                if let Some(name) = self.game_key() {
                    self.dialog = Some(Dialog::DeleteGame(name.to_string()));
                }
            }
            Message::ConfirmDeleteGame => {
                if let Some(Dialog::DeleteGame(name)) = self.dialog.take() {
                    self.config.games.retain(|g| g.name != name);
                    if self.config.auto_switch.default_profile.as_ref().is_some_and(|d| d.game.as_ref() == Some(&name)) {
                        self.config.auto_switch.default_profile = None;
                    }
                    self.show_game(None);
                    self.message = Some((format!("Deleted {name}. Save & apply to make it final."), false));
                }
            }
            Message::OpenBrowse(kind) => self.dialog = Some(Dialog::Browse { kind, from: None, open: None }),
            Message::BrowseFrom(source) => {
                if let Some(Dialog::Browse { from, open, .. }) = &mut self.dialog {
                    *from = Some(source);
                    *open = None;
                }
            }
            Message::BrowseOpen(i) => {
                if let Some(Dialog::Browse { open, .. }) = &mut self.dialog {
                    *open = if *open == Some(i) { None } else { Some(i) };
                }
            }
            Message::CopyItem(i) => {
                let Some(Dialog::Browse { kind, from: Some(source), .. }) = &self.dialog else { return Task::none() };
                let (kind, source) = (*kind, source.clone());
                let Some(items) = self.browse_items(&source) else { return Task::none() };
                let Some(root) = items.names(kind).get(i).map(std::string::ToString::to_string) else { return Task::none() };
                self.dialog = None;
                self.copy_items(&items, kind, &root, &source);
            }
            _ => {}
        }
        Task::none()
    }

    /// Copies `root` (a `kind` item of `items`) into the shown list, with whatever it refers
    /// to that isn't already usable here. Names taken here get "(2)", and references among the
    /// copies follow.
    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    fn copy_items(&mut self, items: &Game, kind: ItemKind, root: &str, source: &BrowseSource) {
        let names = self.names();
        let needed = pack::dependencies(items, kind, root, |k, n| names.list(k).iter().any(|x| x == n));
        let mut copy = Game::new("", Vec::new());
        for (k, n) in &needed {
            match k {
                ItemKind::Macro => copy.macros.extend(items.macros.iter().find(|m| &m.name == n).cloned()),
                ItemKind::Menu => copy.menus.extend(items.menus.iter().find(|m| &m.name == n).cloned()),
                ItemKind::Info => copy.info.extend(items.info.iter().find(|o| &o.name == n).cloned()),
                ItemKind::Layer => copy.layers.extend(items.layers.iter().find(|l| &l.name == n).cloned()),
            }
        }
        let mut renamed_root = root.to_string();
        for (k, old) in &needed {
            let taken = |x: &str| {
                let here = match k {
                    ItemKind::Layer => self.game().layers.iter().any(|l| l.name == x),
                    _ => !self.item_name_free(*k, None, x),
                };
                here || (x != old && copy.names(*k).contains(&x))
            };
            let new = free_name(old, taken);
            if &new == old {
                continue;
            }
            copy.rename_item(*k, old, &new);
            copy.rename_refs(*k, old, &new);
            if *k == kind && old == root {
                renamed_root = new;
            }
        }
        // Copies go first, the one asked for at the top of its list, opened.
        let shift = |set: &HashSet<usize>, by: usize| set.iter().map(|j| j + by).collect::<HashSet<_>>();
        let (macros, menus, info) = (copy.macros.len(), copy.menus.len(), copy.info.len());
        let order = |list_kind: ItemKind, n: &str| !(list_kind == kind && n == renamed_root);
        copy.macros.sort_by_key(|m| order(ItemKind::Macro, &m.name));
        copy.menus.sort_by_key(|m| order(ItemKind::Menu, &m.name));
        copy.info.sort_by_key(|o| order(ItemKind::Info, &o.name));
        copy.layers.sort_by_key(|l| order(ItemKind::Layer, &l.name));
        self.macros_mut().splice(0..0, copy.macros);
        self.menus_mut().splice(0..0, copy.menus);
        self.infos_mut().splice(0..0, copy.info);
        self.game_mut().layers.splice(0..0, copy.layers);
        self.open_macros = shift(&self.open_macros, macros);
        self.open_menus = shift(&self.open_menus, menus);
        self.open_appearance = self.open_appearance.iter().map(|a| a.map(|j| j + menus)).collect();
        self.open_infos = shift(&self.open_infos, info);
        self.open_info_appearance = shift(&self.open_info_appearance, info);
        match kind {
            ItemKind::Macro => {
                self.open_macros.insert(0);
            }
            ItemKind::Menu => {
                self.open_menus.insert(0);
            }
            ItemKind::Info => {
                self.open_infos.insert(0);
            }
            ItemKind::Layer => self.layer = 0,
        }
        let extra: Vec<String> = needed[1..].iter().map(|(k, n)| format!("{} “{n}”", k.noun())).collect();
        let with = if extra.is_empty() { String::new() } else { format!(", with {}", extra.join(", ")) };
        self.message = Some((format!("Copied {} “{renamed_root}” from {source}{with}.", kind.noun()), false));
    }

    /// Works out the export dialog's pack again after its details changed.
    fn refresh_export(&mut self) {
        if let Some(Dialog::Export { game, info, preview, .. }) = &mut self.dialog
            && let Some(g) = self.config.game(Some(game))
        {
            **preview = pack::export(g, &self.config.shared, info);
        }
    }

    fn open_import(&mut self, plan: Plan) {
        let choices = Choices { replace: false, keep_mine: vec![false; plan.rule_clashes.len()] };
        self.dialog = Some(Dialog::Import { plan: Box::new(plan), choices });
    }

    /// A "Copy from" source, as a game.
    fn browse_items(&self, source: &BrowseSource) -> Option<Game> {
        match source {
            BrowseSource::Game(key) => self.config.game(key.as_deref()).cloned(),
            BrowseSource::Library(i, _) => Some(self.library.get(*i)?.pack.to_game()),
        }
    }

    /// Whether any connected controller has a gyro: `None` with none connected.
    fn pad_has_gyro(&self) -> Option<bool> {
        let pads: Vec<_> = self.status.as_ref()?.devices.iter().filter(|d| !d.ignored).collect();
        (!pads.is_empty()).then(|| pads.iter().any(|d| d.gyro))
    }

    pub(super) fn view_dialog<'a>(&'a self, dialog: &'a Dialog) -> Element<'a, Message> {
        let body = match dialog {
            Dialog::AddGame { search, selected } => self.view_add_game(search, *selected),
            Dialog::Import { plan, choices } => self.view_import(plan, choices),
            Dialog::Export { game, info, library, preview } => self.view_export(game, info, *library, preview),
            Dialog::DeleteGame(name) => self.view_delete(name),
            Dialog::Browse { kind, from, open } => self.view_browse(*kind, from.as_ref(), *open),
        };
        modal(body)
    }

    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    fn view_add_game<'a>(&'a self, search: &str, selected: Option<usize>) -> Element<'a, Message> {
        let query = search.trim().to_lowercase();
        let added = |id: &str| self.config.games.iter().find(|g| g.origin.as_ref().is_some_and(|o| o.id == id));
        let installed = |i: usize| self.installed.as_ref().map(|inst| inst.has(&self.library[i].pack.rules));
        let mut order: Vec<usize> = (0..self.library.len())
            .filter(|i| self.library[*i].pack.pack.name.to_lowercase().contains(&query))
            .collect();
        // The user's own games first.
        order.sort_by_key(|i| !installed(*i).unwrap_or(false));

        let mut list = column![
            row![
                dropdown(Template::NEW, None::<Template>, Message::AddEmptyGame).placeholder("Empty game from a template…").width(300),
                button(text("Import a file…")).style(button::secondary).on_press(Message::ImportFile),
            ]
            .spacing(8),
            text(match &self.installed {
                None => "Checking which games you have installed…",
                Some(_) => "Built-in games (installed ones first):",
            })
            .size(13)
            .color(MUTED_COLOR),
        ]
        .spacing(8);
        let mut entries = column![].spacing(4);
        for i in &order {
            let pack = &self.library[*i].pack;
            let mut line = row![text(pack.pack.name.clone()).size(15).width(Length::Fill)].spacing(8).align_y(Alignment::Center);
            for f in &pack.pack.requires {
                line = line.push(text(f.label()).size(12).color(MUTED_COLOR));
            }
            if added(&pack.pack.id).is_some() {
                line = line.push(text("added").size(12).color(MUTED_COLOR));
            } else if installed(*i) == Some(true) {
                line = line.push(text("installed").size(12).color(style_accent()));
            }
            entries = entries.push(
                button(line).width(Length::Fill).padding([6, 10]).style(style::segment(selected == Some(*i))).on_press(Message::SelectLibrary(*i)),
            );
        }
        if self.library.is_empty() {
            entries = entries.push(text("The built-in library is empty in this build.").size(13).color(MUTED_COLOR));
        } else if order.is_empty() {
            entries = entries.push(text("No built-in game matches.").size(13).color(MUTED_COLOR));
        }
        list = list.push(scrollable(entries).height(LIBRARY_LIST_HEIGHT));

        let mut details = column![].spacing(8).width(320);
        match selected.and_then(|i| self.library.get(i).map(|e| (i, e))) {
            Some((i, entry)) => {
                let p = &entry.pack;
                details = details.push(text(p.pack.name.clone()).size(20)).push(pack_summary(p, self.pad_has_gyro()));
                details = details.push(match added(&p.pack.id) {
                    Some(g) => button(text("Open it")).on_press(Message::SelectPage(Page::Game(Some(g.name.clone())))),
                    None => button(text("Preview & add…")).on_press(Message::PreviewLibrary(i)),
                });
            }
            None => details = details.push(text("Pick a game to see what's in it.").size(13).color(MUTED_COLOR)),
        }

        column![
            text("Add a game").size(22),
            field("Search the library", search).on_input(Message::SetLibrarySearch),
            row![list.width(Length::Fill), rule::vertical(1), details].spacing(16),
            row![space::horizontal(), button(text("Cancel")).style(button::secondary).on_press(Message::CloseDialog)],
        ]
        .spacing(12)
        .width(820)
        .into()
    }

    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    fn view_import<'a>(&'a self, plan: &'a Plan, choices: &'a Choices) -> Element<'a, Message> {
        let p = &plan.pack;
        let verb = plan.update_kind.map_or("Add", |k| k.verb());
        let mut col = column![text(format!("{verb} {}", p.pack.name)).size(22), pack_summary(p, self.pad_has_gyro())].spacing(12);

        if let Some(name) = &plan.update_of {
            let installed = self.config.game(Some(name)).and_then(|g| g.origin.as_ref()).map(|o| o.version.clone()).unwrap_or_default();
            let what = match plan.update_kind {
                Some(pack::UpdateKind::Downgrade) => "an older version than",
                Some(pack::UpdateKind::Reinstall) => "the same version as",
                _ => "a newer version than",
            };
            col = col.push(text(format!("This is {what} the {name} you have ({installed}); it replaces it.")));
            if !plan.edited.is_empty() {
                col = col.push(
                    text(format!("You've changed these since adding it, and the {} replaces them: {}.", verb.to_lowercase(), plan.edited.join(", ")))
                        .size(13)
                        .color(ERROR_COLOR),
                );
            }
        }
        if plan.name_clash {
            col = col.push(
                column![
                    text(format!("You already have a game named {}.", p.pack.name)),
                    row![
                        button(text(format!("Add as {}", plan.renamed)).size(13))
                            .style(if choices.replace { button::secondary } else { button::primary })
                            .on_press(Message::SetReplaceGame(false)),
                        button(text("Replace mine").size(13))
                            .style(if choices.replace { button::danger } else { button::secondary })
                            .on_press(Message::SetReplaceGame(true)),
                    ]
                    .spacing(8),
                ]
                .spacing(6),
            );
        }
        if !plan.shared_clashes.is_empty() {
            let mut list = column![text("These are renamed, since shared items have their names:").size(13)].spacing(4);
            for (kind, old, new) in &plan.shared_clashes {
                list = list.push(text(format!("• {} “{old}” → “{new}”", kind.noun())).size(13));
            }
            col = col.push(list);
        }
        let clashes: Vec<_> = pack::live_clashes(plan, choices).collect();
        if !clashes.is_empty() {
            let mut list = column![text("Another game already switches on the same window. By default this one takes over:").size(13)].spacing(4);
            for (c, clash) in clashes {
                let rule = &p.rules[clash.rule];
                let keep = choices.keep_mine.get(c).copied().unwrap_or(false);
                list = list.push(
                    checkbox(keep)
                        .label(format!("Keep {}'s rule for {} {}", clash.other_game, rule.kind, rule.value))
                        .on_toggle(move |k| Message::SetKeepMine(c, k)),
                );
            }
            col = col.push(list);
        }
        col.push(text("Nothing changes until you Save & apply.").size(13).color(MUTED_COLOR))
            .push(row![
                space::horizontal(),
                button(text("Cancel")).style(button::secondary).on_press(Message::CloseDialog),
                button(text(verb)).on_press(Message::ConfirmImport),
            ]
            .spacing(8))
            .width(620)
            .into()
    }

    fn view_export<'a>(&'a self, game: &'a str, info: &'a PackInfo, library: bool, out: &'a pack::Export) -> Element<'a, Message> {
        let Some(g) = self.config.game(Some(game)) else { return text("This game is gone.").into() };
        let p = &out.pack;
        let mut col = column![
            text(format!("Export {game}")).size(22),
            text(contents_line(p)).size(13),
        ]
        .spacing(10);
        if !out.pulled_in.is_empty() {
            let names: Vec<String> = out.pulled_in.iter().map(|(k, n)| format!("{} “{n}”", k.noun())).collect();
            col = col.push(text(format!("Shared items it uses are copied in: {}.", names.join(", "))).size(13).color(MUTED_COLOR));
        }
        if !out.dangling.is_empty() {
            col = col.push(text(format!("It refers to things that don't exist: {}.", out.dangling.join(", "))).size(13).color(ERROR_COLOR));
        }
        for f in &out.features {
            col = col.push(
                text(format!(
                    "The {} uses {}: players on a plain XInput pad won't get it.",
                    f.place,
                    f.feature.label()
                ))
                .size(13)
                .color(ERROR_COLOR),
            );
        }
        if g.rules.is_empty() {
            col = col.push(text("It has no auto-switch rules, so it won't switch on by itself for others.").size(13).color(MUTED_COLOR));
        }
        if let Some(b) = info.based_on.as_ref().filter(|_| !library) {
            col = col.push(
                text(format!("Based on {} {} by {}: it's exported as your own pack, crediting the original.", b.name, b.version, b.author))
                    .size(13)
                    .color(MUTED_COLOR),
            );
        }
        let input = |label: &'static str, value: &'a str, field_kind: PackField, width: f32| {
            labeled(label, field(label, value).on_input(move |v| Message::SetPackField(field_kind, v)).width(width).into())
        };
        col = col
            .push(input("Version", &info.version, PackField::Version, 120.0))
            .push(input("Author", &info.author, PackField::Author, 260.0))
            .push(input("Made with", &info.made_with, PackField::MadeWith, 260.0))
            .push(input("Description", &info.description, PackField::Description, 420.0));
        if cfg!(debug_assertions) || g.origin.as_ref().is_some_and(|o| o.library) {
            col = col.push(
                checkbox(library)
                    .label("Library pack: keep the library's ID, so users get it as an update")
                    .on_toggle(Message::SetLibraryExport),
            );
        }
        col.push(row![
            space::horizontal(),
            button(text("Cancel")).style(button::secondary).on_press(Message::CloseDialog),
            button(text("Save…")).on_press(Message::SaveExport),
        ]
        .spacing(8))
        .width(620)
        .into()
    }

    fn view_delete<'a>(&'a self, name: &'a str) -> Element<'a, Message> {
        let Some(g) = self.config.game(Some(name)) else { return text("This game is gone.").into() };
        let list = |label: &str, names: Vec<&String>| -> Option<Element<'a, Message>> {
            (!names.is_empty()).then(|| {
                let names: Vec<&str> = names.iter().map(|n| n.as_str()).collect();
                text(format!("{label}: {}", names.join(", "))).size(13).into()
            })
        };
        let rules: Vec<String> = g.rules.iter().map(|r| format!("{} {}", r.kind, r.value)).collect();
        let mut col = column![text(format!("Delete {name}?")).size(22), text("This removes:")].spacing(8);
        col = col.extend(
            [
                list("Profiles", g.profiles.iter().map(|p| &p.name).collect()),
                list("Macros", g.macros.iter().map(|m| &m.name).collect()),
                list("Menus", g.menus.iter().map(|m| &m.name).collect()),
                list("Info overlays", g.info.iter().map(|o| &o.name).collect()),
                list("Rules", rules.iter().collect()),
            ]
            .into_iter()
            .flatten(),
        );
        col.push(text("Nothing changes until you Save & apply.").size(13).color(MUTED_COLOR))
            .push(row![
                space::horizontal(),
                button(text("Cancel")).style(button::secondary).on_press(Message::CloseDialog),
                button(text("Delete")).style(button::danger).on_press(Message::ConfirmDeleteGame),
            ]
            .spacing(8))
            .width(520)
            .into()
    }

    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    fn view_browse<'a>(&'a self, kind: ItemKind, from: Option<&BrowseSource>, open: Option<usize>) -> Element<'a, Message> {
        let here = (!self.on_shared()).then(|| self.game_key().map(str::to_string));
        let mut sources: Vec<BrowseSource> = self
            .config
            .all_games()
            .map(|(key, _)| key.map(str::to_string))
            .filter(|key| Some(key) != here.as_ref())
            .map(BrowseSource::Game)
            .collect();
        let added = |id: &str| self.config.games.iter().any(|g| g.origin.as_ref().is_some_and(|o| o.id == id));
        sources.extend(
            self.library
                .iter()
                .enumerate()
                .filter(|(_, e)| !added(&e.pack.pack.id))
                .map(|(i, e)| BrowseSource::Library(i, e.pack.pack.name.clone())),
        );
        let mut col = column![
            text(format!("Copy a {} from another game", kind.noun())).size(22),
            dropdown(sources, from.cloned(), Message::BrowseFrom).placeholder("Pick a game…").width(320),
        ]
        .spacing(12);
        if let Some(items) = from.and_then(|s| self.browse_items(s)) {
            let mut list = column![].spacing(6);
            let count = items.names(kind).len();
            for i in 0..count {
                let (name, summary, preview_panel): (String, String, Option<Element<'a, Message>>) = match kind {
                    ItemKind::Macro => {
                        let m = &items.macros[i];
                        let steps: Vec<String> = m.steps.iter().map(step_summary).collect();
                        (m.name.clone(), format!("{} steps", m.steps.len()), Some(text(steps.join(" · ")).size(13).into()))
                    }
                    ItemKind::Menu => {
                        let m = &items.menus[i];
                        let view = MenuSession::open(std::slice::from_ref(m), &m.name, Opener { buttons: vec![Button::LeftBumper], ..Opener::default() })
                            .and_then(|s| s.view(std::slice::from_ref(m)))
                            .map(|v| preview(crate::overlay::draw::menu_panel(&crate::menu::MenuView { style: preview_style(&m.style), ..v }, self.preview_font())));
                        (m.name.clone(), format!("{} · {} items", m.kind.tag().short(), m.items.len()), view)
                    }
                    ItemKind::Info => {
                        let o = &items.info[i];
                        let sample = InfoOverlay { style: preview_style(&o.style), ..o.clone() };
                        let view = crate::info::resolve(&sample, &crate::info::Live::sample(self.config.info_glyphs));
                        (o.name.clone(), format!("{} rows", o.rows.len()), Some(preview(crate::overlay::draw::info_panel(&view, self.preview_font()))))
                    }
                    ItemKind::Layer => {
                        let l = &items.layers[i];
                        let lines = layer_lines(l);
                        let summary = format!("{} override{}", l.overrides(), if l.overrides() == 1 { "" } else { "s" });
                        (l.name.clone(), summary, Some(text(lines.join("\n")).size(13).into()))
                    }
                };
                let chevron = if open == Some(i) { "▾" } else { "▸" };
                let mut card = column![
                    row![
                        button(text(format!("{chevron}  {name}")).size(16)).style(button::text).padding(0).on_press(Message::BrowseOpen(i)),
                        text(summary).size(13).color(MUTED_COLOR),
                        space::horizontal(),
                        button(text("Copy here").size(13)).on_press(Message::CopyItem(i)),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                ]
                .spacing(8);
                if open == Some(i)
                    && let Some(panel) = preview_panel
                {
                    card = card.push(panel);
                }
                list = list.push(container(card).padding(10).width(Length::Fill).style(style::inset));
            }
            if count == 0 {
                list = list.push(text(format!("It has no {}s.", kind.noun())).size(13).color(MUTED_COLOR));
            }
            col = col.push(scrollable(list).height(Length::Shrink));
        }
        col.push(row![space::horizontal(), button(text("Close")).style(button::secondary).on_press(Message::CloseDialog)])
            .width(680)
            .into()
    }
}

/// A pack's description, contents, game IDs and controller needs, for the picker and preview.
fn pack_summary<'a>(p: &pack::Pack, gyro: Option<bool>) -> Element<'a, Message> {
    let h = &p.pack;
    let mut col = column![].spacing(6);
    let mut by = Vec::new();
    if !h.version.is_empty() {
        by.push(format!("version {}", h.version));
    }
    if !h.author.is_empty() {
        by.push(format!("by {}", h.author));
    }
    if !h.made_with.is_empty() {
        by.push(format!("made with {}", h.made_with));
    }
    if !by.is_empty() {
        col = col.push(text(by.join(" · ")).size(13).color(MUTED_COLOR));
    }
    if let Some(b) = &h.based_on {
        col = col.push(text(format!("Based on {} {} by {}", b.name, b.version, b.author)).size(13).color(MUTED_COLOR));
    }
    if !h.description.is_empty() {
        col = col.push(text(h.description.clone()).size(14));
    }
    col = col.push(text(contents_line(p)).size(13));
    let ids: Vec<String> = p.rules.iter().map(|r| format!("{} {}", r.kind, r.value)).collect();
    if !ids.is_empty() {
        col = col.push(text(format!("Switches on for: {}", ids.join(", "))).size(13).color(MUTED_COLOR));
    }
    for f in &h.requires {
        let line = match gyro {
            Some(false) => text(format!("Your controller has no {}: {}.", f.label(), f.missing())).color(ERROR_COLOR),
            Some(true) => text(format!("Uses {}, which your controller has.", f.label())).color(MUTED_COLOR),
            None => text(format!("Uses {} (no controller connected to check).", f.label())).color(MUTED_COLOR),
        };
        col = col.push(line.size(13));
    }
    col.into()
}

/// "2 profiles (Play, Menus) · 3 macros · 1 menu · 1 info overlay".
fn contents_line(p: &pack::Pack) -> String {
    let count = |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
    let profiles: Vec<&str> = p.profiles.iter().map(|x| x.name.as_str()).collect();
    format!(
        "{} ({}) · {} · {} · {} · {}",
        count(p.profiles.len(), "profile"),
        profiles.join(", "),
        count(p.layers.len(), "layer"),
        count(p.macros.len(), "macro"),
        count(p.menus.len(), "menu"),
        count(p.info_overlays.len(), "info overlay"),
    )
}

/// What a layer changes, one line each: "A → F1", "Right Stick → scroll, 15 notches/s".
fn layer_lines(l: &crate::config::Layer) -> Vec<String> {
    let mut lines: Vec<String> = l.buttons.iter().map(|(b, a)| format!("{} → {}", short_button(*b), summarize(a))).collect();
    lines.extend(l.gestures.keys().map(|b| format!("{} gestures", short_button(*b))));
    for (stick, cfg) in [(Stick::Left, &l.left_stick), (Stick::Right, &l.right_stick)] {
        if let Some(cfg) = cfg {
            lines.push(format!("{stick} → {}", stick_summary(cfg)));
        }
    }
    for (t, cfg) in [(Trigger::Left, &l.left_trigger), (Trigger::Right, &l.right_trigger)] {
        if let Some(cfg) = cfg {
            lines.push(format!("{t} → {}", trigger_summary(cfg)));
        }
    }
    if l.gyro.is_some() {
        lines.push("Gyro settings".into());
    }
    lines.extend(l.combos.iter().map(|c| {
        let name: Vec<&str> = c.buttons.iter().map(|b| short_button(*b)).collect();
        format!("{} → {}", name.join(" + "), summarize(&c.action))
    }));
    if !l.disabled_combos.is_empty() {
        lines.push(format!("{} of the profile's combos off", l.disabled_combos.len()));
    }
    lines
}

/// One macro step in a few words.
fn step_summary(step: &MacroStep) -> String {
    match step {
        MacroStep::Tap { action, hold_ms } => format!("tap {} ({hold_ms} ms)", summarize(action)),
        MacroStep::Press(action) => format!("hold {}", summarize(action)),
        MacroStep::Release(action) => format!("release {}", summarize(action)),
        MacroStep::Wait(ms) => format!("wait {ms} ms"),
        MacroStep::Stick { stick, x, y } => format!("{stick} {}", StickPreset::of(*x, *y)),
    }
}

/// A dialog over a dimmed backdrop. Clicking the backdrop doesn't close it, so a choice in
/// progress isn't lost by a stray click.
fn modal(body: Element<'_, Message>) -> Element<'_, Message> {
    let dialog = container(scrollable(body)).padding(20).max_height(820.0).style(style::inset);
    opaque(
        container(center(opaque(dialog)))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style { background: Some(Color { a: 0.6, ..Color::BLACK }.into()), ..container::Style::default() }),
    )
}

/// A file name for a game's pack: "Elden Ring" → "elden-ring".
fn file_stem(name: &str) -> String {
    let stem: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("{}.{}", if stem.is_empty() { "game" } else { &stem }, pack::EXTENSION)
}

/// Asks for a pack file and reads it; `None` if the user cancelled.
async fn pick_pack() -> Option<Result<String, String>> {
    tokio::task::spawn_blocking(|| {
        let path = rfd::FileDialog::new()
            .set_title("Import a game pack")
            .add_filter("Game pack", &[pack::EXTENSION])
            .pick_file()?;
        Some(std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display())))
    })
    .await
    .unwrap_or_else(|e| Some(Err(e.to_string())))
}

/// Asks where to save a pack and writes it; `None` if the user cancelled.
async fn save_pack(file_name: String, text: String, dir: Option<PathBuf>) -> Option<Result<String, String>> {
    tokio::task::spawn_blocking(move || {
        let mut chooser = rfd::FileDialog::new()
            .set_title("Export game pack")
            .add_filter("Game pack", &[pack::EXTENSION])
            .set_file_name(file_name);
        if let Some(dir) = dir {
            chooser = chooser.set_directory(dir);
        }
        let mut path = chooser.save_file()?;
        if path.extension().is_none_or(|x| x != pack::EXTENSION) {
            path.set_extension(pack::EXTENSION);
        }
        Some(std::fs::write(&path, text).map(|_| path.display().to_string()).map_err(|e| format!("{}: {e}", path.display())))
    })
    .await
    .unwrap_or_else(|e| Some(Err(e.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::tests::*;

    #[test]
    fn pack_file_names_are_tidy() {
        assert_eq!(file_stem("ELDEN RING: Nightreign"), "elden-ring-nightreign.padpack");
        assert_eq!(file_stem("  "), "game.padpack");
    }

    #[test]
    fn importing_a_pack_file_previews_then_adds_the_game() {
        let mut app = with_game();
        app.config.shared.macros.push(Macro { name: "Heal".into(), steps: vec![MacroStep::Wait(10)] });
        app.config.games[0].profiles[0].set_button(Button::North, ButtonAction::Macro { name: "Heal".into(), repeat: false });
        let text = pack::export(&app.config.games[0], &app.config.shared, &pack::draft(&app.config.games[0], false))
            .pack
            .to_toml()
            .unwrap();

        let _ = app.update(Message::PackFileRead(Some(Ok(text))));
        let Some(Dialog::Import { plan, .. }) = &app.dialog else { panic!("no preview") };
        assert!(plan.name_clash && plan.shared_clashes.len() == 1 && plan.rule_clashes.len() == 1);
        assert_eq!(app.config.games.len(), 1, "nothing changes before confirming");
        let _ = app.update(Message::SetKeepMine(0, true));
        let _ = app.update(Message::ConfirmImport);
        assert_eq!(app.page, Page::Game(Some("Doom (2)".into())));
        let imported = app.game();
        assert_eq!(imported.macros[0].name, "Heal (2)");
        assert!(!imported.rules[0].enabled && app.config.games[0].rules[0].enabled, "kept mine");
        assert_eq!(app.validate(), None);

        let _ = app.update(Message::PackFileRead(Some(Ok("format = 9".into()))));
        assert!(app.message.as_ref().is_some_and(|(m, err)| *err && m.contains("newer version")));
    }

    #[test]
    fn export_drafts_fork_and_remember_their_details() {
        let mut app = with_game();
        let _ = app.update(Message::OpenExport);
        let Some(Dialog::Export { info, .. }) = &app.dialog else { panic!("no export dialog") };
        let id = info.id.clone();
        assert_eq!(info.version, "1.0");
        let _ = app.update(Message::SetPackField(PackField::Author, "me".into()));
        let _ = app.update(Message::Exported(Some(Ok("/tmp/doom.padpack".into()))));
        assert!(app.dialog.is_none());
        assert_eq!((app.game().pack.id.as_str(), app.game().pack.author.as_str()), (id.as_str(), "me"));
        let _ = app.update(Message::OpenExport);
        let Some(Dialog::Export { info, .. }) = &app.dialog else { panic!() };
        assert_eq!(info.id, id, "my own pack keeps its ID");

        // General has no Details tab, so nothing to export.
        let _ = app.update(Message::CloseDialog);
        let _ = app.update(Message::SelectPage(Page::Game(None)));
        let _ = app.update(Message::OpenExport);
        assert!(app.dialog.is_none());
    }

    #[test]
    fn items_are_copied_from_other_games() {
        let mut app = with_game();
        let mut quake = Game::new("Quake", vec![Profile::passthrough("P")]);
        quake.info.push(InfoOverlay { name: "Controls".into(), always: true, on_start: None, linger: None, style: OverlayStyle::info(), rows: vec![] });
        app.config.games.push(quake);
        app.config.games[0].info.push(InfoOverlay { name: "Controls".into(), always: false, on_start: None, linger: None, style: OverlayStyle::info(), rows: vec![] });
        let _ = app.update(Message::OpenBrowse(ItemKind::Info));
        let _ = app.update(Message::BrowseFrom(BrowseSource::Game(Some("Quake".into()))));
        let _ = app.update(Message::CopyItem(0));
        assert!(app.dialog.is_none());
        let names: Vec<&str> = app.game().info.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["Controls (2)", "Controls"]);
        assert!(app.game().info[0].always);
        assert!(app.open_infos.contains(&0));
    }

    #[test]
    fn copying_a_menu_brings_the_macros_it_runs() {
        let mut app = with_game();
        let heal = |steps: usize| Macro { name: "Heal".into(), steps: vec![MacroStep::Wait(1); steps] };
        let mut quake = Game::new("Quake", vec![Profile::passthrough("P")]);
        quake.macros = vec![heal(1), Macro { name: "Taunt".into(), steps: vec![] }];
        quake.menus.push(Menu {
            name: "Wheel".into(),
            kind: MenuKind::List,
            items: ["Heal", "Taunt"].map(|n| MenuItem { label: n.into(), action: ButtonAction::Macro { name: n.into(), repeat: false }, button: None }).into(),
            cancel: None,
            style: OverlayStyle::default(),
        });
        app.config.games.push(quake);
        // Doom has its own "Taunt", which the copy can use, but no "Heal".
        app.config.games[0].macros.push(Macro { name: "Taunt".into(), steps: vec![MacroStep::Wait(9)] });
        app.config.games[0].menus.push(Menu { name: "Wheel".into(), kind: MenuKind::List, items: vec![], cancel: None, style: OverlayStyle::default() });
        let _ = app.update(Message::SelectGameTab(GameTab::Menus));
        let _ = app.update(Message::OpenBrowse(ItemKind::Menu));
        let _ = app.update(Message::BrowseFrom(BrowseSource::Game(Some("Quake".into()))));
        let _ = app.update(Message::CopyItem(0));
        let doom = app.game();
        assert_eq!(doom.menus.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Wheel (2)", "Wheel"]);
        assert_eq!(doom.macros.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Heal", "Taunt"]);
        assert!(app.open_menus.contains(&0));
        assert!(app.message.as_ref().is_some_and(|(m, _)| m.contains("with macro “Heal”")), "{:?}", app.message);
        assert_eq!(app.validate(), None);
    }

    #[test]
    fn every_library_game_adds_and_saves_cleanly() {
        let mut app = app();
        assert!(!app.library.is_empty());
        for i in 0..app.library.len() {
            let _ = app.update(Message::PreviewLibrary(i));
            let _ = app.view();
            let _ = app.update(Message::ConfirmImport);
            assert_eq!(app.validate(), None, "{}", app.library[i].file);
            let _ = app.update(Message::SelectGameTab(GameTab::Info));
            let _ = app.view();
        }
    }

    #[test]
    fn the_add_game_picker_and_page_changes_close_dialogs() {
        let mut app = app();
        let _ = app.update(Message::OpenAddGame);
        let _ = app.update(Message::SetLibrarySearch("doo".into()));
        assert!(matches!(&app.dialog, Some(Dialog::AddGame { search, .. }) if search == "doo"));
        let _ = app.update(Message::SelectPage(Page::Settings));
        assert!(app.dialog.is_none() && app.page == Page::Settings);
    }
}

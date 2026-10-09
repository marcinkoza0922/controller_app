//! iced front-end. Talks to the daemon over IPC; falls back to editing the config file directly
//! when the daemon is not running.

use std::{
    borrow::Borrow,
    collections::HashSet,
    fmt,
    mem::Discriminant,
    rc::Rc,
    str::FromStr,
    time::{Duration, Instant},
};

use evdev::KeyCode;
use iced::{
    Alignment, Color, Element, Length, Subscription, Task,
    widget::{
        button, center, checkbox, column, container, mouse_area, opaque, pick_list, pin, row, rule,
        scrollable, slider, space, stack, svg, text, text_input, toggler, tooltip,
    },
};

use crate::{
    config::{
        Analog, Button, ButtonAction, CarouselControls, Cluster, Combo, Config, Feature, Game, GestureKind, GRID_MAX, GyroActivation,
        CurrentInput, GyroConfig, GyroHorizontal, GyroInput, GyroMode, InfoOverlay, ItemKind, LogOverlay, Macro, MacroStep, Menu, MenuItem,
        MenuKind, MenuKindTag, MouseButton, OverlayStyle, Paint, Profile, ProfileRef, Rule, RuleKind, ScopeRef,
        ScreenPosition, Stick, StickAction, StickConfig, Toggled, Trigger, TriggerAction, WheelDirection, Zone, free_name,
    },
    engine::Opener,
    info::PadFamily,
    ipc::{self, InputSnapshot, Request, Response, Status, WindowInfo},
    keyboard::{self, Layout},
    launchers, library,
    menu::MenuSession,
    pack, pad_svg, style,
};

mod actions;
mod checks;
mod games;
mod items;
mod layers;
mod logs;
mod manual;
mod overlays;
mod packs;
mod profile;
mod ring_preview;
mod tracking;
mod widgets;

use actions::*;
use checks::*;
use games::*;
use items::*;
use layers::IndicatorChoice;
use packs::{BrowseSource, Dialog, PackField};
use profile::*;
use tracking::*;
use widgets::*;

pub fn run() -> iced::Result {
    if let Err(e) = crate::daemon::ensure_running() {
        crate::monitor::log!("cannot start the daemon: {e:#}");
    }
    let app = iced::application(App::boot, App::update, App::view)
        .title("Padwight")
        .theme(App::theme)
        .subscription(App::subscription)
        .exit_on_close_request(false)
        .window(iced::window::Settings {
            size: iced::Size::new(1100.0, 900.0),
            icon: crate::icon::window(),
            ..Default::default()
        });
    crate::font::BUNDLED.iter().fold(app, |app, b| app.font(b.bytes)).run()
}

/// How many edits Ctrl+Z can take back.
const UNDO_STEPS: usize = 100;
/// Edits of the same kind this close together (a slider drag, typing) undo as one.
const EDIT_RUN: Duration = Duration::from_secs(1);

const LABEL_WIDTH: f32 = 170.0;
const SIDEBAR_WIDTH: f32 = 210.0;
const ERROR_COLOR: Color = Color::from_rgb(0.9, 0.3, 0.3);
const MUTED_COLOR: Color = Color::from_rgb(0.55, 0.55, 0.6);

struct App {
    /// Working copy being edited.
    config: Config,
    /// Last copy known to be applied; `config != saved` means unsaved edits.
    saved: Config,
    /// `None` while the daemon is unreachable.
    status: Option<Status>,
    page: Page,
    /// The profile being edited, by index in the current game's profiles.
    editing: usize,
    game_tab: GameTab,
    /// The chapter open on the Manual page, by index in `manual::CHAPTERS`.
    manual_chapter: usize,
    /// The manual's chapters, parsed for drawing (see `manual.rs`).
    manual: Vec<iced::widget::markdown::Content>,
    /// Light or dark, from the system; the app's theme follows it.
    theme_mode: iced::theme::Mode,
    /// On General's Macros, Menus, Info and Log overlays tabs: show the shared items (usable by
    /// every game) instead of General's own.
    shared_view: bool,
    /// The read-only list of shared items under a game's own is open.
    shared_open: bool,
    /// Filters the sidebar's game list.
    game_search: String,
    message: Option<(String, bool)>,
    /// Latest physical input streamed from the daemon.
    live: Option<InputSnapshot>,
    picker: Option<KeyPicker>,
    dialog: Option<Dialog>,
    profile_tab: ProfileTab,
    /// The controller picture is folded away on the Profiles tab.
    picture_hidden: bool,
    /// Rows showing their full editor instead of a one-line summary.
    expanded: HashSet<Target>,
    /// Waiting for a controller button press to jump to its row.
    finding: bool,
    found: Option<Button>,
    /// Macros whose cards are open, by index in the shown list.
    open_macros: HashSet<usize>,
    /// Menus (by index) whose card is open.
    open_menus: HashSet<usize>,
    /// Appearance editors that are open: a menu's, or the keyboard's (`None`).
    open_appearance: HashSet<Option<usize>>,
    /// The numpad card's Appearance section is open.
    numpad_appearance: bool,
    /// The media controls' and the in-game menu's Appearance sections are open.
    media_appearance: bool,
    menu_appearance: bool,
    /// Info overlays (by index) whose card, or Appearance section, is open.
    open_infos: HashSet<usize>,
    open_info_appearance: HashSet<usize>,
    /// Log overlays (by index) whose card, or Appearance section, is open.
    open_logs: HashSet<usize>,
    open_log_appearance: HashSet<usize>,
    /// Input groups listing their own inputs, by overlay kind, overlay and group.
    open_input_groups: HashSet<(ItemKind, usize, usize)>,
    /// The built-in game library.
    library: Vec<library::Entry>,
    /// Installed and running games, once looked up for the library picker.
    installed: Option<launchers::Installed>,
    /// Games and profiles renamed since the last save. The daemon still reports the active
    /// profile by its old name until then.
    renames: Vec<Rename>,
    /// The shown game's items changed since it was imported (see `pack::edited_items`),
    /// worked out after each edit rather than every frame.
    edited: Vec<String>,
    /// The layer shown on the Layers tab, by index in the game's layers.
    layer: usize,
    /// The profile a layer is shown over, by index in the game's profiles.
    compare: usize,
    /// What the layer editor edits: the compared profile with the layer on top (see
    /// `layers.rs`).
    layer_view: Option<Profile>,
    /// The layer name label's Appearance section is open.
    indicator_appearance: bool,
    /// The assignment the last failed save is about, to jump to.
    problem_at: Option<checks::Place>,
    /// Copies of `config` from before each edit, newest last, for Ctrl+Z.
    undo: Vec<Config>,
    /// The kind of the last edit recorded in `undo`, and when it was made.
    last_edit: Option<(Discriminant<Message>, Instant)>,
    /// Save & quit: close the window once the save goes through.
    quit_after_save: bool,
}

/// What the sidebar shows on the right.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Page {
    Overview,
    Settings,
    /// The in-app manual.
    Manual,
    /// A game's page; `None` is General.
    Game(Option<String>),
}

/// Sections of a game's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameTab {
    Profiles,
    Layers,
    Macros,
    Menus,
    Info,
    Logs,
    Details,
}

impl GameTab {
    const ALL: [GameTab; 7] =
        [GameTab::Profiles, GameTab::Layers, GameTab::Macros, GameTab::Menus, GameTab::Info, GameTab::Logs, GameTab::Details];
}

impl fmt::Display for GameTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            GameTab::Profiles => "Profiles",
            GameTab::Layers => "Layers",
            GameTab::Macros => "Macros",
            GameTab::Menus => "Menus",
            GameTab::Info => "Info overlays",
            GameTab::Logs => "Log overlays",
            GameTab::Details => "Details",
        })
    }
}

/// A rename not yet saved, for following the daemon's active profile.
#[derive(Debug, Clone, PartialEq)]
enum Rename {
    Game { old: String, new: String },
    Profile { game: Option<String>, old: String, new: String },
}

/// Where `at` is once these renames apply.
fn follow_renames(renames: &[Rename], mut at: ProfileRef) -> ProfileRef {
    for r in renames {
        match r {
            Rename::Game { old, new } if at.game.as_ref() == Some(old) => at.game = Some(new.clone()),
            Rename::Profile { game, old, new } if &at.game == game && &at.profile == old => at.profile = new.clone(),
            _ => {}
        }
    }
    at
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Target {
    Button(Button),
    Trigger(Trigger),
    Combo(usize),
    Gesture(Button, GestureKind),
    Zone(Analog, usize),
    /// Step `.1` of macro `.0` (in the shown list), not part of any profile.
    MacroStep(usize, usize),
    /// Item `.1` of menu `.0` (in the shown list).
    MenuItem(usize, usize),
    /// The advanced response settings of a mouse stick.
    StickResponse(Stick),
    /// Sector `.1` of a stick's button ring.
    RingSector(Stick, usize),
}

/// Open-card indices after item `i` is removed: later items move up one place.
fn shift_removed(set: &HashSet<usize>, i: usize) -> HashSet<usize> {
    set.iter().filter(|j| **j != i).map(|j| if *j > i { j - 1 } else { *j }).collect()
}

/// `base`, or `base 2`, `base 3`, … whichever isn't taken.
fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..)
        .map(|i| if i == 1 { base.to_string() } else { format!("{base} {i}") })
        .find(|n| !taken(n))
        .unwrap()
}

#[derive(Debug, Clone)]
enum Message {
    Poll,
    LiveInput(Option<InputSnapshot>),
    StatusLoaded(Result<Status, String>),
    ConfigLoaded(Box<Config>, Option<String>),
    SetEnabled(bool),
    ActivateProfile(ProfileRef),
    Done(Result<(), String>),
    SelectPage(Page),
    SetGameSearch(String),
    SelectGameTab(GameTab),
    SelectChapter(usize),
    /// A link in the manual; relative links name another chapter's file.
    ManualLink(String),
    SystemMode(iced::theme::Mode),
    SetSharedView(bool),
    ToggleSharedSection,
    RenameGame(String),
    EditProfile(String),
    RenameProfile(String),
    AddProfile(Template),
    DeleteProfile,
    SetAction(Target, ButtonAction),
    SetStick(Stick, StickConfig),
    SetTrigger(Trigger, TriggerAction),
    SetIgnored(String, bool),
    SetAutoSwitch(bool),
    SetDefaultProfile(DefaultChoice),
    AddRule(Option<WindowInfo>),
    RemoveRule(usize),
    SetRuleKind(usize, RuleKind),
    SetRuleValue(usize, String),
    SetRuleProfile(usize, String),
    SetRuleEnabled(usize, bool),
    TestRumble(String),
    TestTurn(i32),
    ToggleOverlay,
    ToggleNumpad,
    ToggleNumpadAppearance,
    SetNumpadStyle(OverlayStyle),
    ToggleMediaAppearance,
    SetMediaStyle(OverlayStyle),
    ToggleInGameMenuAppearance,
    SetInGameMenuStyle(OverlayStyle),
    CalibrateGyro(String),
    CopyMotionRuleCommand,
    SetGyro(GyroConfig),
    SetRequires(Vec<Feature>),
    AddCombo,
    RemoveCombo(usize),
    AddComboButton(usize, Button),
    RemoveComboButton(usize, Button),
    SetComboWindow(f32),
    AddGesture(Button, GestureKind),
    RemoveGesture(Button, GestureKind),
    SetTapWindow(f32),
    SetLongPress(f32),
    AddZone(Analog, ZonePreset),
    RemoveZone(Analog, usize),
    SetZoneRange(Analog, usize, f32, f32),
    SelectProfileTab(ProfileTab),
    ToggleExpanded(Target),
    TogglePicture,
    /// Expand (true) or collapse every row in the current profile section.
    ExpandAll(bool),
    StartFind,
    CancelFind,
    ToggleMacro(usize),
    NewInfo,
    ToggleInfo(usize),
    DeleteInfo(usize),
    RenameInfo(usize, String),
    SetInfoAlways(usize, bool),
    /// Seconds info overlay `.0` is shown for when the game starts, or `None` for not then.
    SetInfoOnStart(usize, Option<f32>),
    /// Seconds info overlay `.0` stays after being let go, or `None` to go at once.
    SetInfoLinger(usize, Option<f32>),
    SetInfoTitle(usize, String),
    SetIndicatorTitle(String),
    SetInfoStyle(usize, OverlayStyle),
    ToggleInfoAppearance(usize),
    AddInfoRow(usize),
    /// Row `.1` of info overlay `.0`, moved up if `.2`.
    MoveInfoRow(usize, usize, bool),
    RemoveInfoRow(usize, usize),
    AddInfoCell(usize, usize),
    /// Cell `.2` of row `.1` of info overlay `.0`.
    SetInfoCell(usize, usize, usize, String),
    InsertInfoToken(usize, usize, usize, TokenChoice),
    RemoveInfoCell(usize, usize, usize),
    SetInfoGlyphs(PadFamily),
    NewLog,
    ToggleLog(usize),
    ToggleLogAppearance(usize),
    DeleteLog(usize),
    RenameLog(usize, String),
    /// Log overlay `.0` replaced by this edited copy (its name is left as it is).
    SetLog(usize, LogOverlay),
    /// What info overlay `.0`'s `{current_input}` cells follow.
    SetInfoInput(usize, crate::config::CurrentInput),
    /// Lists (or hides) the inputs of group `.2` in overlay `.1` of kind `.0`.
    ToggleInputGroup(ItemKind, usize, usize),
    NewMacro,
    DeleteMacro(usize),
    RenameMacro(usize, String),
    AddMacroStep(usize, StepKind),
    /// Step `.1` of macro `.0`.
    SetMacroStep(usize, usize, MacroStep),
    MoveMacroStep(usize, usize, bool),
    RemoveMacroStep(usize, usize),
    InsertMotion(usize, Motion),
    ToggleMenu(usize),
    ToggleAppearance(Option<usize>),
    NewMenu(MenuKindTag),
    DeleteMenu(usize),
    RenameMenu(usize, String),
    SetMenuKind(usize, MenuKind),
    SetMenuStyle(usize, OverlayStyle),
    SetKeyboardStyle(OverlayStyle),
    /// The font of all overlays (`None`: the system's), or the shown game's own.
    SetOverlayFont(Option<String>),
    SetColourblindTones(bool),
    SetGameOverlayFont(Option<String>),
    /// A game's own style for the keyboard or numpad, or back to the global one.
    SetGameOverlayStyle(crate::keyboard::Layout, Option<OverlayStyle>),
    /// A game's own look for the media controls or the in-game menu, or back to the global one.
    SetGameMediaStyle(Option<OverlayStyle>),
    SetGameMenuStyle(Option<OverlayStyle>),
    AddMenuItem(usize),
    RemoveMenuItem(usize, usize),
    MoveMenuItem(usize, usize, bool),
    SetMenuItemLabel(usize, usize, String),
    SetMenuItemButton(usize, usize, QuickChoice),
    OpenKeyPicker(KeyField, Vec<String>, bool),
    PickerKey(&'static str),
    PickerClear,
    PickerClose { apply: bool },
    /// "+ Add game": the library picker.
    OpenAddGame,
    CloseDialog,
    SetLibrarySearch(String),
    /// Library entry to show details of (index in `App::library`).
    SelectLibrary(usize),
    Installed(launchers::Installed),
    AddEmptyGame(Template),
    /// Preview adding the library entry at this index.
    PreviewLibrary(usize),
    /// Preview the library's newer version of this game.
    UpdateFromLibrary(String),
    ImportFile,
    /// A pack file's text, or why it couldn't be read; `None` if the user cancelled.
    PackFileRead(Option<Result<String, String>>),
    SetReplaceGame(bool),
    /// Keep the other game's rule in rule clash `.0`.
    SetKeepMine(usize, bool),
    ConfirmImport,
    OpenExport,
    SetPackField(PackField, String),
    SetLibraryExport(bool),
    SaveExport,
    /// Where the pack was saved, or why not; `None` if the user cancelled.
    Exported(Option<Result<String, String>>),
    AskDeleteGame,
    ConfirmDeleteGame,
    OpenBrowse(ItemKind),
    NewLayer,
    SelectLayer(String),
    RenameLayer(String),
    DeleteLayer,
    /// Which of the game's profiles the layer editor shows the layer over.
    SetCompare(String),
    SetIndicator(IndicatorChoice),
    SetSwallowUnbound(bool),
    SetIndicatorDelay(u64),
    SetIndicatorStyle(OverlayStyle),
    ToggleIndicatorAppearance,
    /// Make the layer set this (a copy of the profile's, to edit).
    OverrideInput(LayerPart),
    /// Stop the layer setting this.
    RevertInput(LayerPart),
    /// Switch a profile combo (by its buttons) off in the layer, or back on.
    ToggleBaseCombo(Vec<Button>),
    BrowseFrom(BrowseSource),
    BrowseOpen(usize),
    CopyItem(usize),
    Save,
    Saved(Result<(), String>),
    Revert,
    Undo,
    /// Jump to the assignment the failed save is about.
    ShowProblem,
    /// The window's close button was pressed.
    CloseRequested,
    /// Save first, then close (from the unsaved-changes prompt).
    SaveAndQuit,
    /// Close without saving (from the unsaved-changes prompt).
    DiscardAndQuit,
}

async fn call(req: Request) -> Result<Response, String> {
    tokio::task::spawn_blocking(move || ipc::request(&req))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

fn call_ok(req: Request) -> Task<Message> {
    Task::perform(call(req), |r| Message::Done(r.map(|_| ())))
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let config = Config::default();
        let app = App {
            saved: config.clone(),
            config,
            status: None,
            page: Page::Overview,
            editing: 0,
            game_tab: GameTab::Profiles,
            manual_chapter: 0,
            manual: manual::load(), theme_mode: iced::theme::Mode::default(),
            shared_view: false,
            shared_open: false,
            game_search: String::new(),
            message: None,
            live: None,
            picker: None,
            dialog: None,
            profile_tab: ProfileTab::Buttons,
            picture_hidden: false,
            expanded: HashSet::new(),
            finding: false,
            found: None,
            open_macros: HashSet::new(),
            open_menus: HashSet::new(),
            open_appearance: HashSet::new(),
            numpad_appearance: false,
            media_appearance: false,
            menu_appearance: false,
            open_infos: HashSet::new(),
            open_info_appearance: HashSet::new(),
            open_logs: HashSet::new(),
            open_log_appearance: HashSet::new(),
            open_input_groups: HashSet::new(),
            library: library::entries(),
            installed: None,
            renames: Vec::new(),
            edited: Vec::new(),
            layer: 0,
            compare: 0,
            layer_view: None,
            indicator_appearance: false,
            problem_at: None,
            undo: Vec::new(),
            last_edit: None,
            quit_after_save: false,
        };
        let load = Task::perform(
            async {
                match call(Request::GetConfig).await {
                    Ok(Response::Config(c)) => (c, None),
                    _ => match tokio::task::spawn_blocking(Config::load).await {
                        Ok(Ok(c)) => (Box::new(c), None),
                        Ok(Err(e)) => (Box::default(), Some(format!("{e:#}"))),
                        Err(e) => (Box::default(), Some(e.to_string())),
                    },
                }
            },
            |(c, err)| Message::ConfigLoaded(c, err),
        );
        (app, Task::batch([load, Task::done(Message::Poll), iced::system::theme().map(Message::SystemMode)]))
    }

    fn theme(&self) -> iced::Theme {
        <iced::Theme as iced::theme::Base>::default(self.theme_mode)
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::time::every(Duration::from_secs(1)).map(|_| Message::Poll),
            Subscription::run(watch_input),
            iced::event::listen_with(undo_shortcut),
            iced::window::close_requests().map(|_| Message::CloseRequested),
            iced::system::theme_changes().map(Message::SystemMode),
        ])
    }

    /// The game whose page is shown (General when it's not a game page).
    fn game_key(&self) -> Option<&str> {
        match &self.page {
            Page::Game(key) => key.as_deref(),
            _ => None,
        }
    }

    fn game(&self) -> &Game {
        self.config.game(self.game_key()).unwrap_or(&self.config.general)
    }

    fn game_mut(&mut self) -> &mut Game {
        let index = self.game_key().and_then(|name| self.config.games.iter().position(|g| g.name == name));
        match index {
            Some(i) => &mut self.config.games[i],
            None => &mut self.config.general,
        }
    }

    /// The Macros, Menus, Info and Log overlays tabs show shared items (on General's page).
    fn on_shared(&self) -> bool {
        let item_tab = matches!(self.game_tab, GameTab::Macros | GameTab::Menus | GameTab::Info | GameTab::Logs);
        self.shared_view && item_tab && self.page == Page::Game(None)
    }

    fn macros(&self) -> &Vec<Macro> {
        if self.on_shared() { &self.config.shared.macros } else { &self.game().macros }
    }

    fn macros_mut(&mut self) -> &mut Vec<Macro> {
        if self.on_shared() { &mut self.config.shared.macros } else { &mut self.game_mut().macros }
    }

    fn menus(&self) -> &Vec<Menu> {
        if self.on_shared() { &self.config.shared.menus } else { &self.game().menus }
    }

    fn menus_mut(&mut self) -> &mut Vec<Menu> {
        if self.on_shared() { &mut self.config.shared.menus } else { &mut self.game_mut().menus }
    }

    fn infos(&self) -> &Vec<InfoOverlay> {
        if self.on_shared() { &self.config.shared.info } else { &self.game().info }
    }

    fn infos_mut(&mut self) -> &mut Vec<InfoOverlay> {
        if self.on_shared() { &mut self.config.shared.info } else { &mut self.game_mut().info }
    }

    /// What the shown profile and items can refer to.
    fn names(&self) -> Names {
        if self.on_shared() { Names::shared(&self.config) } else { Names::for_game(&self.config, self.game()) }
    }

    /// The profile the editor shows: the edited profile, or on the Layers tab, the layer over
    /// the profile it's compared with.
    fn profile(&self) -> Option<&Profile> {
        if self.editing_layer() {
            return self.layer_view.as_ref();
        }
        self.game().profiles.get(self.editing)
    }

    fn profile_mut(&mut self) -> Option<&mut Profile> {
        if self.editing_layer() {
            return self.layer_view.as_mut();
        }
        let editing = self.editing;
        self.game_mut().profiles.get_mut(editing)
    }

    /// The edited profile, as the daemon names it.
    fn profile_ref(&self) -> Option<ProfileRef> {
        self.profile().map(|p| ProfileRef::new(self.game_key(), &p.name))
    }

    /// Forgets per-page state when another page or list is shown.
    fn reset_page_state(&mut self) {
        self.open_macros.clear();
        self.open_menus.clear();
        self.open_infos.clear();
        self.open_info_appearance.clear();
        self.open_logs.clear();
        self.open_log_appearance.clear();
        self.open_input_groups.clear();
        self.open_appearance.retain(Option::is_none);
        self.expanded.clear();
        self.found = None;
        self.finding = false;
    }

    /// Shows a game's page, editing its active profile if it has it.
    fn show_game(&mut self, key: Option<String>) {
        let has_active = self.config.active_ref().game == key;
        if key.is_some() {
            self.shared_view = false;
        }
        self.page = Page::Game(key);
        self.editing = if has_active {
            let active = self.config.active_ref();
            self.game().profiles.iter().position(|p| p.name == active.profile).unwrap_or(0)
        } else {
            0
        };
        self.layer = 0;
        self.compare = self.editing;
        self.reset_page_state();
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        // Polling and live input don't change the config, and arrive many times a second.
        let edits = !matches!(message, Message::Poll | Message::LiveInput(_) | Message::StatusLoaded(_));
        let message = self.new_layer_for(message);
        // Loading the config and undoing aren't edits to take back.
        let undoable = edits && !matches!(message, Message::ConfigLoaded(..) | Message::Undo);
        let kind = std::mem::discriminant(&message);
        let snapshot = undoable.then(|| self.config.clone());
        // Edits in the layer editor change a working copy; they're kept as the layer's
        // overrides.
        let before = self.layer_view.clone().filter(|_| self.editing_layer());
        let task = self.handle(message);
        if let Some(before) = before
            && self.editing_layer()
        {
            self.write_back(&before);
        }
        if let Some(snapshot) = snapshot {
            self.record_edit(kind, snapshot);
        }
        if edits {
            self.refresh_layer_view();
            self.edited = if self.page == Page::Game(None) { Vec::new() } else { pack::edited_items(self.game()) };
        }
        task
    }

    /// Keeps `before` for Ctrl+Z if the config changed. A run of edits of one kind close
    /// together is a single step.
    fn record_edit(&mut self, kind: Discriminant<Message>, before: Config) {
        if self.config == before {
            return;
        }
        let now = Instant::now();
        let continues = self.last_edit.is_some_and(|(k, at)| k == kind && now.duration_since(at) < EDIT_RUN);
        if !continues {
            self.undo.push(before);
            if self.undo.len() > UNDO_STEPS {
                self.undo.remove(0);
            }
        }
        self.last_edit = Some((kind, now));
    }

    /// After the config was replaced wholesale: leaves a game that's gone, and keeps the
    /// profile being edited in range.
    fn settle_config(&mut self) {
        let exists = self.game_key().is_none_or(|k| self.config.games.iter().any(|g| g.name == k));
        if !exists {
            self.show_game(None);
        }
        self.editing = self.editing.min(self.game().profiles.len().saturating_sub(1));
    }

    /// The app-wide messages: polling, status, saving. Each module handles its own and passes
    /// the rest on: games → items → profile → actions → layers → packs.
    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Poll => {
                return Task::perform(call(Request::Status), |r| {
                    Message::StatusLoaded(r.and_then(|r| match r {
                        Response::Status(s) => Ok(s),
                        other => Err(format!("The daemon sent an unexpected reply: {other:?}")),
                    }))
                });
            }
            Message::LiveInput(snapshot) => {
                if self.finding
                    && let Some(b) = snapshot.as_ref().and_then(|s| newly_pressed(self.live.as_ref(), s))
                {
                    return self.jump_to(b);
                }
                self.live = snapshot;
            }
            Message::StatusLoaded(Ok(status)) => {
                // Mirror daemon-owned fields so both copies stay comparable.
                let active = ProfileRef { game: status.active_game.clone(), profile: status.active_profile.clone() };
                self.saved.enabled = status.enabled;
                self.saved.active = active.clone();
                self.config.enabled = status.enabled;
                self.config.active = follow_renames(&self.renames, active);
                self.status = Some(status);
            }
            Message::StatusLoaded(Err(_)) => {
                // The daemon has quit (from its tray, say), so close like the window's close button:
                // unsaved edits get asked about first.
                if self.status.take().is_some() {
                    return Task::done(Message::CloseRequested);
                }
            }
            Message::ConfigLoaded(config, err) => {
                let config = *config;
                self.saved = config.clone();
                self.config = config;
                if let Page::Game(key) = self.page.clone() {
                    let exists = key.as_deref().is_none_or(|k| self.config.games.iter().any(|g| g.name == k));
                    self.show_game(if exists { key } else { None });
                }
                if let Some(e) = err {
                    self.message = Some((format!("Couldn't load the config file: {e}"), true));
                }
            }
            Message::SetEnabled(on) => return call_ok(Request::SetEnabled(on)),
            Message::ActivateProfile(at) => {
                if self.saved.profile(&at).is_some() {
                    return Task::batch([call_ok(Request::Activate(at)), Task::done(Message::Poll)]);
                }
                self.message = Some(("Save the new profile, then make it active.".into(), true));
            }
            Message::Done(Ok(())) => return Task::done(Message::Poll),
            Message::Done(Err(e)) => self.message = Some((e, true)),
            Message::Save => {
                if let Some(err) = self.validate() {
                    self.message = Some((err, true));
                    self.problem_at = self.problem_place();
                    self.quit_after_save = false;
                    return Task::none();
                }
                return self.push(self.config.clone());
            }
            Message::Saved(Ok(())) => {
                self.saved = self.config.clone();
                self.problem_at = None;
                self.renames.clear();
                self.message = Some(("Saved and applied.".into(), false));
                if std::mem::take(&mut self.quit_after_save) {
                    return iced::exit();
                }
                return Task::done(Message::Poll);
            }
            Message::Saved(Err(e)) => {
                self.quit_after_save = false;
                self.message = Some((e, true));
            }
            Message::Revert => {
                self.problem_at = None;
                self.config = self.saved.clone();
                self.renames.clear();
                self.settle_config();
                self.message = None;
            }
            Message::Undo
            | Message::ShowProblem
            | Message::CloseRequested
            | Message::SaveAndQuit
            | Message::DiscardAndQuit => {
                return self.update_session(&message);
            }
            other => return self.update_games(other),
        }
        Task::none()
    }

    /// Shows the profile and assignment in `place`, open for editing.
    fn go_to(&mut self, place: checks::Place) -> Task<Message> {
        self.dialog = None;
        self.picker = None;
        self.show_game(place.game);
        self.editing = place.profile;
        self.game_tab = GameTab::Profiles;
        self.expanded.insert(place.target);
        match place.target {
            Target::Button(b) | Target::Gesture(b, _) => self.jump_to(b),
            Target::Combo(_) => {
                self.profile_tab = ProfileTab::Combos;
                Task::none()
            }
            Target::Trigger(_) | Target::Zone(..) | Target::RingSector(..) => {
                self.profile_tab = ProfileTab::Sticks;
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Undo and closing the window.
    fn update_session(&mut self, message: &Message) -> Task<Message> {
        match message {
            Message::ShowProblem => {
                if let Some(place) = self.problem_at.clone() {
                    return self.go_to(place);
                }
            }
            Message::Undo => {
                if self.picker.is_none()
                    && self.dialog.is_none()
                    && let Some(previous) = self.undo.pop()
                {
                    self.config = previous;
                    self.last_edit = None;
                    self.settle_config();
                }
            }
            Message::CloseRequested => {
                if self.config == self.saved {
                    return iced::exit();
                }
                if self.dialog.is_none() {
                    self.dialog = Some(Dialog::ConfirmQuit);
                }
            }
            Message::SaveAndQuit => {
                self.quit_after_save = true;
                return Task::done(Message::Save);
            }
            Message::DiscardAndQuit => return iced::exit(),
            _ => {}
        }
        Task::none()
    }

    /// Sends a config to the daemon, or writes the file if the daemon is not running.
    fn push(&self, config: Config) -> Task<Message> {
        let daemon_up = self.status.is_some();
        Task::perform(
            async move {
                if daemon_up {
                    call(Request::SetConfig(Box::new(config))).await.map(|_| ())
                } else {
                    tokio::task::spawn_blocking(move || config.save())
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| format!("{e:#}"))
                }
            },
            Message::Saved,
        )
    }

    fn view(&self) -> Element<'_, Message> {
        let page: Element<'_, Message> = match &self.page {
            Page::Overview => column![
                self.view_live(self.config.active().map(|p| self.with_active_layers(p)).as_ref(), false),
                rule::horizontal(1),
                self.view_devices()
            ]
                .spacing(16)
                .into(),
            Page::Settings => self.view_settings(),
            Page::Manual => self.view_manual(),
            Page::Game(_) => self.view_game(),
        };
        let content = column![self.view_header(), rule::horizontal(1), page].spacing(16).padding(20);
        let main = column![scrollable(content).id("main").height(Length::Fill), self.view_footer()].width(Length::Fill);
        // Always a stack with the page first, popups or not: iced keeps widget state (such as
        // the page's scroll position) by place in the tree, so the shape must not change when
        // a popup opens or closes.
        let base = row![self.view_sidebar(), rule::vertical(1), main];
        let popup = match (&self.picker, &self.dialog) {
            (Some(picker), _) => Some(view_picker(picker)),
            (None, Some(dialog)) => Some(self.view_dialog(dialog)),
            (None, None) => None,
        };
        let mut layers = stack![base];
        if let Some(popup) = popup {
            layers = layers.push(popup);
        }
        layers.into()
    }

    fn view_header(&self) -> Element<'_, Message> {
        let profiles: Vec<ProfileRef> = self
            .saved
            .all_games()
            .flat_map(|(key, g)| g.profiles.iter().map(move |p| ProfileRef::new(key, &p.name)))
            .collect();
        let running = self.status.is_some();

        let mut title = row![text("Padwight").size(26), space::horizontal()].align_y(Alignment::Center);
        if !running {
            title = title.push(text("○ Daemon not running").color(ERROR_COLOR));
        }

        let mut header = column![
            title,
            row![
                toggler(self.config.enabled)
                    .label("Remapping enabled")
                    .on_toggle_maybe(running.then_some(Message::SetEnabled)),
                space::horizontal(),
                text("Active profile"),
                dropdown(profiles, Some(self.saved.active.clone()), Message::ActivateProfile).width(280),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ]
        .spacing(12);

        if !running {
            header = header.push(
                text("The background service isn't running. Start it with `systemctl --user start padwight` (or `padwight daemon`). You can still edit: your changes are saved and take effect once it starts.")
                    .size(13)
                    .color(MUTED_COLOR),
            );
        }
        header.into()
    }

    fn view_footer(&self) -> Element<'_, Message> {
        let dirty = self.config != self.saved;
        let msg: Element<'_, Message> = match &self.message {
            Some((m, is_err)) => text(m).color(if *is_err { ERROR_COLOR } else { MUTED_COLOR }).into(),
            None if dirty => text("Unsaved changes").color(MUTED_COLOR).into(),
            None => space().into(),
        };
        let mut actions = row![msg, space::horizontal()].spacing(8).align_y(Alignment::Center);
        if self.problem_at.is_some() {
            actions = actions.push(button(text("Go to problem")).style(style::secondary).on_press(Message::ShowProblem));
        }
        actions = actions
            .push(button(text("Revert")).style(style::secondary).on_press_maybe(dirty.then_some(Message::Revert)))
            .push(button(text("Save & apply")).on_press_maybe(dirty.then_some(Message::Save)));
        // Only a rule on top: a box would add a second line beside the sidebar's divider.
        column![
            rule::horizontal(1),
            container(actions).padding(12),
        ]
        .into()
    }
}

/// Ctrl+Z, without Shift, undoes the last edit.
fn undo_shortcut(event: iced::Event, _: iced::event::Status, _: iced::window::Id) -> Option<Message> {
    match event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Character(c),
            modifiers,
            ..
        }) if modifiers.command() && !modifiers.shift() && c.eq_ignore_ascii_case("z") => Some(Message::Undo),
        _ => None,
    }
}

/// Streams live input from the daemon, reconnecting every second while it is unavailable.
fn watch_input() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::{SinkExt, channel::mpsc};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    iced::stream::channel(16, async |mut output: mpsc::Sender<Message>| {
        let mut request = serde_json::to_string(&Request::WatchInput).unwrap();
        request.push('\n');
        loop {
            if let Ok(mut stream) = tokio::net::UnixStream::connect(ipc::socket_path()).await
                && stream.write_all(request.as_bytes()).await.is_ok()
            {
                let mut lines = BufReader::new(stream).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Ok(snapshot) = serde_json::from_str(&line) {
                        let _ = output.send(Message::LiveInput(snapshot)).await;
                    }
                }
            }
            let _ = output.send(Message::LiveInput(None)).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The app on General's page, editing its "Gamepad" profile.
    pub(super) fn app() -> App {
        let mut app = App::boot().0;
        let _ = app.update(Message::SelectPage(Page::Game(None)));
        app.editing = 0;
        app
    }

    /// The app with a game "Doom" (profiles "Play" and "Menus") shown.
    pub(super) fn with_game() -> App {
        let mut app = app();
        // Guide plain, so the game starts without the Guide layer; tests add their own layers.
        let mut profiles = vec![Profile::pc_action("Play"), Profile::desktop("Menus")];
        profiles.iter_mut().for_each(|p| p.set_button(Button::Guide, ButtonAction::Gamepad(Button::Guide)));
        let mut game = Game::new("Doom", profiles);
        game.rules.push(Rule::new(RuleKind::Executable, "doom.exe", "Play"));
        app.config.games.push(game);
        let _ = app.update(Message::SelectPage(Page::Game(Some("Doom".into()))));
        app
    }

    pub(super) fn pick(app: &mut App, field: KeyField, single: bool, keys: &[&'static str]) {
        let _ = app.update(Message::OpenKeyPicker(field, Vec::new(), single));
        for k in keys {
            let _ = app.update(Message::PickerKey(k));
        }
        if !single {
            let _ = app.update(Message::PickerClose { apply: true });
        }
    }

    pub(super) fn device(name: &str, analog_triggers: bool, ignored: bool) -> ipc::DeviceInfo {
        ipc::DeviceInfo { name: name.into(), path: String::new(), managed: !ignored, ignored, analog_triggers, rumble: true, gyro: false }
    }

    pub(super) fn snapshot(buttons: &[Button], right_stick: (f32, f32)) -> InputSnapshot {
        InputSnapshot {
            device: "Pad".into(),
            buttons: buttons.to_vec(),
            left_stick: (0.0, 0.0),
            right_stick,
            left_trigger: 0.0,
            right_trigger: 0.0,
            gyro: None,
        }
    }

    #[test]
    fn every_page_tab_and_dialog_builds() {
        let mut app = with_game();
        app.config.shared.menus.push(Menu { name: "Wheel".into(), kind: MenuKind::List, items: vec![], cancel: None, style: OverlayStyle::default() });
        app.config.games[0].info.push(InfoOverlay { name: "Controls".into(), always: true, on_start: None, linger: None, title: None, current_input: Default::default(), style: OverlayStyle::info(), rows: vec![vec!["{south}".into()]] });
        app.config.games[0].macros.push(Macro { name: "Dodge".into(), steps: vec![MacroStep::Wait(5)] });
        app.config.games[0].layers.push(crate::config::Layer::new("Hotkeys"));
        app.status = Some(Status { devices: vec![device("Pad", true, false)], active_layers: vec!["Hotkeys".into()], ..Default::default() });
        let _ = app.view();
        for page in [Page::Overview, Page::Settings, Page::Manual, Page::Game(None), Page::Game(Some("Doom".into()))] {
            let _ = app.update(Message::SelectPage(page));
            for tab in GameTab::ALL {
                let _ = app.update(Message::SelectGameTab(tab));
                let _ = app.update(Message::ToggleMacro(0));
                let _ = app.update(Message::ToggleMenu(0));
                let _ = app.update(Message::ToggleInfo(0));
                let _ = app.update(Message::ToggleSharedSection);
                let _ = app.view();
            }
        }
        let _ = app.update(Message::SetSharedView(true));
        let _ = app.view();
        let _ = app.update(Message::SelectPage(Page::Game(Some("Doom".into()))));
        let text = pack::export(app.game(), &app.config.shared, &pack::draft(app.game(), false)).pack.to_toml().unwrap();
        let dialogs = [
            Message::OpenAddGame,
            Message::Installed(launchers::Installed::default()),
            Message::PackFileRead(Some(Ok(text))),
            Message::OpenExport,
            Message::AskDeleteGame,
            Message::OpenBrowse(ItemKind::Menu),
            Message::BrowseFrom(BrowseSource::Game(None)),
        ];
        for m in dialogs {
            let _ = app.update(m);
            let _ = app.view();
        }
        let _ = app.update(Message::BrowseOpen(0));
        let _ = app.view();
    }

    #[test]
    fn ctrl_z_takes_back_the_last_edit() {
        let mut app = app();
        let before = app.config.clone();
        let _ = app.update(Message::SetAction(Target::Button(Button::South), ButtonAction::Keys(vec!["KEY_A".into()])));
        assert_ne!(app.config, before);
        let _ = app.update(Message::Undo);
        assert_eq!(app.config, before);
        // Nothing left to undo: a no-op.
        let _ = app.update(Message::Undo);
        assert_eq!(app.config, before);
    }

    #[test]
    fn quick_edits_of_one_kind_undo_together() {
        let mut app = app();
        let before = app.config.clone();
        let _ = app.update(Message::SetAction(Target::Button(Button::South), ButtonAction::Keys(vec!["KEY_A".into()])));
        let _ = app.update(Message::SetAction(Target::Button(Button::East), ButtonAction::Keys(vec!["KEY_B".into()])));
        let _ = app.update(Message::Undo);
        assert_eq!(app.config, before);
    }

    #[test]
    fn closing_with_unsaved_edits_asks_first() {
        let mut app = app();
        let _ = app.update(Message::CloseRequested);
        assert!(app.dialog.is_none(), "nothing unsaved: closes without asking");
        let _ = app.update(Message::SetAction(Target::Button(Button::South), ButtonAction::Keys(vec!["KEY_A".into()])));
        let _ = app.update(Message::CloseRequested);
        assert!(matches!(app.dialog, Some(Dialog::ConfirmQuit)));
        let _ = app.update(Message::CloseDialog);
        assert!(app.dialog.is_none());
    }

    #[test]
    fn a_blocked_save_can_jump_to_the_turbo_at_fault() {
        let mut app = app();
        app.config.general.macros.push(Macro { name: "Long".into(), steps: vec![MacroStep::Wait(300)] });
        let turbo = ButtonAction::Turbo { action: Box::new(ButtonAction::Macro { name: "Long".into(), repeat: false }), rate: 10.0, every_ms: 50 };
        app.config.general.profiles[0].set_button(Button::West, turbo);
        let _ = app.update(Message::Save);
        assert!(app.message.as_ref().is_some_and(|(m, err)| *err && m.contains("raise the turbo's gap")));
        assert_eq!(app.problem_at.as_ref().map(|p| (p.game.clone(), p.profile, p.target)), Some((None, 0, Target::Button(Button::West))));

        // Somewhere else first, then the jump brings the turbo's row up for editing.
        app.show_game(None);
        app.editing = 0;
        app.expanded.clear();
        let _ = app.update(Message::ShowProblem);
        assert!(app.expanded.contains(&Target::Button(Button::West)));
        assert_eq!(app.profile_tab, ProfileTab::Buttons);
    }
}

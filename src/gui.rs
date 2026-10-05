//! iced front-end. Talks to the daemon over IPC; falls back to editing the config file directly
//! when the daemon is not running.

use std::{fmt, rc::Rc, str::FromStr, time::Duration};

use evdev::KeyCode;
use iced::{
    Alignment, Color, Element, Length, Subscription, Task,
    widget::{
        button, center, checkbox, column, container, mouse_area, opaque, pick_list, row, rule,
        scrollable, slider, space, stack, svg, text, text_input, toggler,
    },
};

use crate::{
    config::{
        Analog, Button, ButtonAction, Combo, Config, GestureKind, MouseButton, Zone, Profile, Stick, StickAction, StickConfig,
        Trigger, TriggerAction,
    },
    ipc::{self, InputSnapshot, Request, Response, Status},
    keyboard, pad_svg,
};

pub fn run() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Controller App")
        .subscription(App::subscription)
        .window_size((860.0, 900.0))
        .run()
}

const LABEL_WIDTH: f32 = 170.0;
const ERROR_COLOR: Color = Color::from_rgb(0.9, 0.3, 0.3);
const MUTED_COLOR: Color = Color::from_rgb(0.55, 0.55, 0.6);

struct App {
    /// Working copy being edited.
    config: Config,
    /// Last copy known to be applied; `config != saved` means unsaved edits.
    saved: Config,
    /// `None` while the daemon is unreachable.
    status: Option<Status>,
    editing: usize,
    message: Option<(String, bool)>,
    /// Latest physical input streamed from the daemon, and its rendered drawing.
    live: Option<InputSnapshot>,
    pad: svg::Handle,
    picker: Option<KeyPicker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Button(Button),
    Trigger(Trigger),
    Combo(usize),
    Gesture(Button, GestureKind),
    Zone(Analog, usize),
}

/// Address of a key field, so the on-screen keyboard can write its result back.
#[derive(Debug, Clone, PartialEq)]
enum KeyField {
    /// A `Keys` action at `target`, reached through these `Multi` list indices.
    Action { target: Target, path: Vec<usize> },
    /// One direction (0 up, 1 down, 2 left, 3 right) of a stick in direction-keys mode.
    StickDir { stick: Stick, dir: usize },
}

impl KeyField {
    fn root(target: Target) -> Self {
        KeyField::Action { target, path: Vec::new() }
    }

    fn child(&self, index: usize) -> Self {
        match self {
            KeyField::Action { target, path } => {
                let mut path = path.clone();
                path.push(index);
                KeyField::Action { target: *target, path }
            }
            other => other.clone(),
        }
    }
}

/// Open on-screen keyboard. `single` fields hold one key and close on the first click.
struct KeyPicker {
    field: KeyField,
    keys: Vec<String>,
    single: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Poll,
    LiveInput(Option<InputSnapshot>),
    StatusLoaded(Result<Status, String>),
    ConfigLoaded(Config, Option<String>),
    SetEnabled(bool),
    ActivateProfile(String),
    Done(Result<(), String>),
    EditProfile(String),
    RenameProfile(String),
    AddProfile(Template),
    DeleteProfile,
    SetAction(Target, ButtonAction),
    SetStick(Stick, StickConfig),
    SetTrigger(Trigger, TriggerAction),
    SetIgnored(String, bool),
    TestRumble(String),
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
    OpenKeyPicker(KeyField, Vec<String>, bool),
    PickerKey(&'static str),
    PickerClear,
    PickerClose { apply: bool },
    Save,
    Saved(Result<(), String>),
    Revert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZonePreset {
    Empty,
    /// Left Shift while a stick is only partly pushed (walk in WASD games).
    Walk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Template {
    Gamepad,
    Desktop,
    Duplicate,
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
            editing: 0,
            message: None,
            live: None,
            pad: pad_handle(None),
            picker: None,
        };
        let load = Task::perform(
            async {
                match call(Request::GetConfig).await {
                    Ok(Response::Config(c)) => (c, None),
                    _ => match tokio::task::spawn_blocking(Config::load).await {
                        Ok(Ok(c)) => (c, None),
                        Ok(Err(e)) => (Config::default(), Some(format!("{e:#}"))),
                        Err(e) => (Config::default(), Some(e.to_string())),
                    },
                }
            },
            |(c, err)| Message::ConfigLoaded(c, err),
        );
        (app, Task::batch([load, Task::done(Message::Poll)]))
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::time::every(Duration::from_secs(1)).map(|_| Message::Poll),
            Subscription::run(watch_input),
        ])
    }

    fn profile(&self) -> Option<&Profile> {
        self.config.profiles.get(self.editing)
    }

    fn profile_mut(&mut self) -> Option<&mut Profile> {
        self.config.profiles.get_mut(self.editing)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Poll => {
                return Task::perform(call(Request::Status), |r| {
                    Message::StatusLoaded(r.and_then(|r| match r {
                        Response::Status(s) => Ok(s),
                        other => Err(format!("unexpected reply: {other:?}")),
                    }))
                });
            }
            Message::LiveInput(snapshot) => {
                if snapshot != self.live {
                    self.pad = pad_handle(snapshot.as_ref());
                    self.live = snapshot;
                }
            }
            Message::StatusLoaded(Ok(status)) => {
                // Mirror daemon-owned fields so both copies stay comparable.
                for c in [&mut self.config, &mut self.saved] {
                    c.enabled = status.enabled;
                    c.active_profile = status.active_profile.clone();
                }
                self.status = Some(status);
            }
            Message::StatusLoaded(Err(_)) => self.status = None,
            Message::ConfigLoaded(config, err) => {
                self.editing = config
                    .profiles
                    .iter()
                    .position(|p| p.name == config.active_profile)
                    .unwrap_or(0);
                self.saved = config.clone();
                self.config = config;
                if let Some(e) = err {
                    self.message = Some((format!("Could not load config: {e}"), true));
                }
            }
            Message::SetEnabled(on) => return call_ok(Request::SetEnabled(on)),
            Message::TestRumble(path) => return call_ok(Request::TestRumble(path)),
            Message::ActivateProfile(name) => {
                if self.saved.profiles.iter().any(|p| p.name == name) {
                    return Task::batch([call_ok(Request::SetProfile(name)), Task::done(Message::Poll)]);
                }
                self.message = Some(("Save the new profile before activating it.".into(), true));
            }
            Message::Done(Ok(())) => return Task::done(Message::Poll),
            Message::Done(Err(e)) => self.message = Some((e, true)),
            Message::EditProfile(name) => {
                if let Some(i) = self.config.profiles.iter().position(|p| p.name == name) {
                    self.editing = i;
                }
            }
            Message::RenameProfile(name) => {
                let taken = self
                    .config
                    .profiles
                    .iter()
                    .enumerate()
                    .any(|(i, p)| i != self.editing && p.name == name);
                if !taken && let Some(p) = self.profile_mut() {
                    p.name = name;
                }
            }
            Message::AddProfile(template) => {
                let name = self.unique_name(match template {
                    Template::Gamepad => "Gamepad",
                    Template::Desktop => "Desktop",
                    Template::Duplicate => "Copy",
                });
                let profile = match template {
                    Template::Gamepad => Profile::passthrough(&name),
                    Template::Desktop => Profile::desktop(&name),
                    Template::Duplicate => {
                        let mut p = self.profile().cloned().unwrap_or_else(|| Profile::passthrough(""));
                        p.name = name;
                        p
                    }
                };
                self.config.profiles.push(profile);
                self.editing = self.config.profiles.len() - 1;
            }
            Message::DeleteProfile => {
                if self.config.profiles.len() > 1 {
                    self.config.profiles.remove(self.editing);
                    self.editing = self.editing.min(self.config.profiles.len() - 1);
                }
            }
            Message::SetAction(target, action) => {
                if let Some(p) = self.profile_mut() {
                    match target {
                        Target::Button(b) => p.set_button(b, action),
                        Target::Trigger(t) => {
                            if let TriggerAction::Button { action: a, .. } = p.trigger_mut(t) {
                                *a = action;
                            }
                        }
                        Target::Combo(i) => {
                            if let Some(c) = p.combos.get_mut(i) {
                                c.action = action;
                            }
                        }
                        Target::Gesture(b, kind) => {
                            *p.gestures.entry(b).or_default().slot(kind) = Some(action);
                        }
                        Target::Zone(a, i) => {
                            if let Some(z) = p.zones_mut(a).get_mut(i) {
                                z.action = action;
                            }
                        }
                    }
                }
            }
            Message::AddGesture(b, kind) => {
                if let Some(p) = self.profile_mut() {
                    let slot = p.gestures.entry(b).or_default().slot(kind);
                    if slot.is_none() {
                        *slot = Some(ButtonAction::Keys(Vec::new()));
                    }
                }
            }
            Message::RemoveGesture(b, kind) => {
                if let Some(p) = self.profile_mut()
                    && let Some(g) = p.gestures.get_mut(&b)
                {
                    *g.slot(kind) = None;
                    if g.is_empty() {
                        p.gestures.remove(&b);
                    }
                }
            }
            Message::AddZone(a, preset) => {
                if let Some(p) = self.profile_mut() {
                    p.zones_mut(a).push(match preset {
                        ZonePreset::Empty => Zone { min: 0.0, max: 0.5, action: ButtonAction::Keys(Vec::new()) },
                        ZonePreset::Walk => Zone {
                            min: 0.0,
                            max: 0.75,
                            action: ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into()]),
                        },
                    });
                }
            }
            Message::RemoveZone(a, i) => {
                if let Some(p) = self.profile_mut()
                    && i < p.zones(a).len()
                {
                    p.zones_mut(a).remove(i);
                }
            }
            Message::SetZoneRange(a, i, min, max) => {
                if let Some(z) = self.profile_mut().and_then(|p| p.zones_mut(a).get_mut(i)) {
                    z.min = min;
                    z.max = max;
                }
            }
            Message::OpenKeyPicker(field, keys, single) => {
                let keys = keys.into_iter().filter(|k| KeyCode::from_str(k).is_ok()).collect();
                self.picker = Some(KeyPicker { field, keys, single });
            }
            Message::PickerKey(code) => {
                if let Some(picker) = &mut self.picker {
                    if picker.single {
                        picker.keys = vec![code.to_string()];
                        return Task::done(Message::PickerClose { apply: true });
                    }
                    match picker.keys.iter().position(|k| k == code) {
                        Some(i) => {
                            picker.keys.remove(i);
                        }
                        None => picker.keys.push(code.to_string()),
                    }
                }
            }
            Message::PickerClear => {
                if let Some(picker) = &mut self.picker {
                    picker.keys.clear();
                }
            }
            Message::PickerClose { apply } => {
                if let Some(picker) = self.picker.take()
                    && apply
                {
                    self.apply_keys(picker.field, picker.keys);
                }
            }
            Message::SetTapWindow(ms) => {
                if let Some(p) = self.profile_mut() {
                    p.tap_window_ms = ms.round() as u64;
                }
            }
            Message::SetLongPress(ms) => {
                if let Some(p) = self.profile_mut() {
                    p.long_press_ms = ms.round() as u64;
                }
            }
            Message::AddCombo => {
                if let Some(p) = self.profile_mut() {
                    p.combos.push(Combo {
                        buttons: vec![Button::LeftBumper, Button::RightBumper],
                        action: ButtonAction::Keys(Vec::new()),
                    });
                }
            }
            Message::RemoveCombo(i) => {
                if let Some(p) = self.profile_mut()
                    && i < p.combos.len()
                {
                    p.combos.remove(i);
                }
            }
            Message::AddComboButton(i, b) => {
                if let Some(c) = self.profile_mut().and_then(|p| p.combos.get_mut(i))
                    && !c.buttons.contains(&b)
                {
                    c.buttons.push(b);
                }
            }
            Message::RemoveComboButton(i, b) => {
                if let Some(c) = self.profile_mut().and_then(|p| p.combos.get_mut(i)) {
                    c.buttons.retain(|x| *x != b);
                }
            }
            Message::SetComboWindow(ms) => {
                if let Some(p) = self.profile_mut() {
                    p.combo_window_ms = ms.round() as u64;
                }
            }
            Message::SetStick(s, cfg) => {
                if let Some(p) = self.profile_mut() {
                    *p.stick_mut(s) = cfg;
                }
            }
            Message::SetTrigger(t, action) => {
                if let Some(p) = self.profile_mut() {
                    *p.trigger_mut(t) = action;
                }
            }
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
            Message::Save => {
                if let Some(err) = self.validate() {
                    self.message = Some((err, true));
                    return Task::none();
                }
                return self.push(self.config.clone());
            }
            Message::Saved(Ok(())) => {
                self.saved = self.config.clone();
                self.message = Some(("Saved and applied.".into(), false));
                return Task::done(Message::Poll);
            }
            Message::Saved(Err(e)) => self.message = Some((e, true)),
            Message::Revert => {
                self.config = self.saved.clone();
                self.editing = self.editing.min(self.config.profiles.len().saturating_sub(1));
                self.message = None;
            }
        }
        Task::none()
    }

    /// Sends a config to the daemon, or writes the file if the daemon is not running.
    fn push(&self, config: Config) -> Task<Message> {
        let daemon_up = self.status.is_some();
        Task::perform(
            async move {
                if daemon_up {
                    call(Request::SetConfig(config)).await.map(|_| ())
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

    /// Writes keys chosen in the on-screen keyboard into the field it was opened for.
    fn apply_keys(&mut self, field: KeyField, keys: Vec<String>) {
        let Some(p) = self.profile_mut() else { return };
        match field {
            KeyField::StickDir { stick, dir } => {
                if let StickAction::Keys { up, down, left, right } = &mut p.stick_mut(stick).action {
                    let slot = match dir {
                        0 => up,
                        1 => down,
                        2 => left,
                        _ => right,
                    };
                    *slot = keys.into_iter().next().unwrap_or_default();
                }
            }
            KeyField::Action { target, path } => {
                let root = match target {
                    Target::Button(b) => Some(p.buttons.entry(b).or_insert(ButtonAction::Disabled)),
                    Target::Trigger(t) => match p.trigger_mut(t) {
                        TriggerAction::Button { action, .. } => Some(action),
                        _ => None,
                    },
                    Target::Combo(i) => p.combos.get_mut(i).map(|c| &mut c.action),
                    Target::Gesture(b, kind) => p.gestures.get_mut(&b).and_then(|g| g.slot(kind).as_mut()),
                    Target::Zone(a, i) => p.zones_mut(a).get_mut(i).map(|z| &mut z.action),
                };
                if let Some(action) = root.and_then(|a| action_at(a, &path)) {
                    *action = ButtonAction::Keys(keys);
                }
            }
        }
    }

    /// False only when every controller in use has on/off triggers (e.g. Switch pads), in
    /// which case pull-depth settings are hidden. With nothing detected, assume analog.
    fn analog_triggers(&self) -> bool {
        let Some(status) = &self.status else { return true };
        let mut in_use = status.devices.iter().filter(|d| !d.ignored).peekable();
        in_use.peek().is_none() || in_use.any(|d| d.analog_triggers)
    }

    fn unique_name(&self, base: &str) -> String {
        (1..)
            .map(|i| if i == 1 { base.to_string() } else { format!("{base} {i}") })
            .find(|n| !self.config.profiles.iter().any(|p| &p.name == n))
            .unwrap()
    }

    fn validate(&self) -> Option<String> {
        for p in &self.config.profiles {
            if p.name.trim().is_empty() {
                return Some("Profile names cannot be empty.".into());
            }
            if let Some(c) = p.combos.iter().find(|c| c.buttons.len() < 2) {
                return Some(format!(
                    "Profile {:?}: a combo needs at least two buttons (has {}).",
                    p.name,
                    c.buttons.len()
                ));
            }
            let mut actions: Vec<&ButtonAction> = p.buttons.values().collect();
            actions.extend(p.combos.iter().map(|c| &c.action));
            let analogs = [
                Analog::Stick(Stick::Left),
                Analog::Stick(Stick::Right),
                Analog::Trigger(Trigger::Left),
                Analog::Trigger(Trigger::Right),
            ];
            for a in analogs {
                if p.zones(a).iter().any(|z| z.min >= z.max) {
                    return Some(format!("Profile {:?}: a zone's range must start below where it ends.", p.name));
                }
                actions.extend(p.zones(a).iter().map(|z| &z.action));
            }
            actions.extend(
                p.gestures
                    .values()
                    .flat_map(|g| GestureKind::ALL.into_iter().filter_map(|k| g.get(k))),
            );
            for t in [Trigger::Left, Trigger::Right] {
                if let TriggerAction::Button { action, .. } = p.trigger(t) {
                    actions.push(action);
                }
            }
            let mut keys: Vec<&String> = actions.into_iter().flat_map(|a| a.key_names()).collect();
            for s in [Stick::Left, Stick::Right] {
                if let StickAction::Keys { up, down, left, right } = &p.stick(s).action {
                    keys.extend([up, down, left, right]);
                }
            }
            if let Some(bad) = keys.iter().find(|k| KeyCode::from_str(k).is_err()) {
                return Some(format!("Profile {:?}: unknown key {:?}", p.name, short_key(bad)));
            }
        }
        None
    }

    fn view(&self) -> Element<'_, Message> {
        let content = column![
            self.view_header(),
            rule::horizontal(1),
            self.view_live(),
            rule::horizontal(1),
            self.view_devices(),
            rule::horizontal(1),
            self.view_profile_bar(),
        ]
        .spacing(16)
        .padding(20);

        let content = match self.profile() {
            Some(p) => content.push(view_profile(p, self.analog_triggers())),
            None => content,
        };

        let base: Element<'_, Message> =
            column![scrollable(content).height(Length::Fill), self.view_footer()].into();
        match &self.picker {
            Some(picker) => stack![base, view_picker(picker)].into(),
            None => base,
        }
    }

    fn view_header(&self) -> Element<'_, Message> {
        let (status_text, color) = match &self.status {
            Some(_) => ("● Daemon running".to_string(), Color::from_rgb(0.3, 0.8, 0.4)),
            None => ("○ Daemon not running".to_string(), ERROR_COLOR),
        };
        let names: Vec<String> = self.saved.profiles.iter().map(|p| p.name.clone()).collect();
        let running = self.status.is_some();

        let mut header = column![
            row![
                text("Controller App").size(26),
                space::horizontal(),
                text(status_text).color(color),
            ]
            .align_y(Alignment::Center),
            row![
                toggler(self.config.enabled)
                    .label("Remapping enabled")
                    .on_toggle_maybe(running.then_some(Message::SetEnabled)),
                space::horizontal(),
                text("Active profile"),
                pick_list(names, Some(self.config.active_profile.clone()), Message::ActivateProfile)
                    .width(200),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ]
        .spacing(12);

        if !running {
            header = header.push(
                text("Start it with `systemctl --user start controller_app` or `controller_app daemon`. Edits are saved to the config file.")
                    .size(13)
                    .color(MUTED_COLOR),
            );
        }
        header.into()
    }

    fn view_live(&self) -> Element<'_, Message> {
        let caption = match (&self.live, &self.status) {
            (Some(live), _) => live.device.clone(),
            (None, Some(_)) => "Press a button on a managed controller".into(),
            (None, None) => "Live input needs the daemon".into(),
        };
        column![
            text("Live input").size(20),
            container(svg(self.pad.clone()).width(420).height(275)).center_x(Length::Fill),
            container(text(caption).size(13).color(MUTED_COLOR)).center_x(Length::Fill),
        ]
        .spacing(8)
        .into()
    }

    fn view_devices(&self) -> Element<'_, Message> {
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
                    let name = d.name.clone();
                    let rumble: Element<'_, Message> = if d.rumble {
                        button(text("Test rumble").size(13))
                            .style(button::secondary)
                            .on_press(Message::TestRumble(d.path.clone()))
                            .into()
                    } else {
                        space().into()
                    };
                    list = list.push(
                        row![
                            column![text(&d.name), text(format!("{} · {state}", d.path)).size(12).color(MUTED_COLOR)]
                                .width(Length::Fill),
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
        list.into()
    }

    fn view_profile_bar(&self) -> Element<'_, Message> {
        let names: Vec<String> = self.config.profiles.iter().map(|p| p.name.clone()).collect();
        let current = self.profile().map(|p| p.name.clone());
        let can_delete = self.config.profiles.len() > 1;
        column![
            text("Edit profile").size(20),
            row![
                pick_list(names, current.clone(), Message::EditProfile).width(200),
                text_input("Profile name", current.as_deref().unwrap_or(""))
                    .on_input(Message::RenameProfile)
                    .width(200),
                space::horizontal(),
                button(text("+ Gamepad")).style(button::secondary).on_press(Message::AddProfile(Template::Gamepad)),
                button(text("+ Desktop")).style(button::secondary).on_press(Message::AddProfile(Template::Desktop)),
                button(text("Duplicate")).style(button::secondary).on_press(Message::AddProfile(Template::Duplicate)),
                button(text("Delete"))
                    .style(button::danger)
                    .on_press_maybe(can_delete.then_some(Message::DeleteProfile)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .into()
    }

    fn view_footer(&self) -> Element<'_, Message> {
        let dirty = self.config != self.saved;
        let msg: Element<'_, Message> = match &self.message {
            Some((m, is_err)) => text(m).color(if *is_err { ERROR_COLOR } else { MUTED_COLOR }).into(),
            None if dirty => text("Unsaved changes").color(MUTED_COLOR).into(),
            None => space().into(),
        };
        container(
            row![
                msg,
                space::horizontal(),
                button(text("Revert")).style(button::secondary).on_press_maybe(dirty.then_some(Message::Revert)),
                button(text("Save & apply")).on_press_maybe(dirty.then_some(Message::Save)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(12)
        .style(container::bordered_box)
        .into()
    }
}

/// Follows `Multi` list indices down from `action`.
fn action_at<'a>(action: &'a mut ButtonAction, path: &[usize]) -> Option<&'a mut ButtonAction> {
    match path.split_first() {
        None => Some(action),
        Some((i, rest)) => match action {
            ButtonAction::Multi(list) => action_at(list.get_mut(*i)?, rest),
            _ => None,
        },
    }
}

/// Modal on-screen keyboard over a dimmed backdrop; clicking the backdrop cancels.
fn view_picker(picker: &KeyPicker) -> Element<'_, Message> {
    let selection = if picker.keys.is_empty() {
        "Nothing selected".to_string()
    } else {
        picker.keys.iter().map(|k| keyboard::label(k)).collect::<Vec<_>>().join(" + ")
    };
    let (title, hint) = if picker.single {
        ("Pick a key", "Click a key to choose it.")
    } else {
        ("Pick keys", "Click keys to toggle them. Selected keys are pressed together, in the order chosen.")
    };
    let mut actions = row![
        button(text("Clear")).style(button::secondary).on_press(Message::PickerClear),
        space::horizontal(),
        button(text("Cancel")).style(button::secondary).on_press(Message::PickerClose { apply: false }),
    ]
    .spacing(8);
    if !picker.single {
        actions = actions.push(button(text("Done")).on_press(Message::PickerClose { apply: true }));
    }
    let dialog = container(
        column![
            text(title).size(20),
            text(hint).size(13).color(MUTED_COLOR),
            text(selection).size(16),
            keyboard::view(&picker.keys, Message::PickerKey),
            actions,
        ]
        .spacing(12),
    )
    .padding(20)
    .style(container::bordered_box);

    let backdrop = container(center(opaque(dialog)))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Color { a: 0.6, ..Color::BLACK }.into()),
            ..container::Style::default()
        });
    opaque(mouse_area(backdrop).on_press(Message::PickerClose { apply: false }))
}

fn pad_handle(input: Option<&InputSnapshot>) -> svg::Handle {
    svg::Handle::from_memory(pad_svg::render(input).into_bytes())
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

fn section<'a>(title: &'a str, rows: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    container(column![text(title).size(18)].extend(rows).spacing(10))
        .padding(14)
        .width(Length::Fill)
        .style(container::rounded_box)
        .into()
}

fn labeled<'a>(label: impl text::IntoFragment<'a>, editor: Element<'a, Message>) -> Element<'a, Message> {
    row![text(label).width(LABEL_WIDTH), editor]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}

fn view_profile(p: &Profile, analog_triggers: bool) -> Element<'_, Message> {
    let buttons = button_rows(p);
    let sticks = [Stick::Left, Stick::Right]
        .into_iter()
        .map(|s| stick_editor(s, p.stick(s)))
        .collect();
    let triggers = [Trigger::Left, Trigger::Right]
        .into_iter()
        .map(|t| trigger_editor(t, p.trigger(t), p.zones(Analog::Trigger(t)), analog_triggers))
        .collect();
    column![
        section("Buttons", buttons),
        section("Combos", combo_rows(p)),
        section("Sticks", sticks),
        section("Triggers", triggers),
    ]
    .spacing(16)
    .into()
}

fn button_rows(p: &Profile) -> Vec<Element<'_, Message>> {
    let mut rows: Vec<Element<'_, Message>> = vec![
        text(
            "Add a double tap, triple tap or long press to any button. Buttons with gestures act \
             once the gesture is decided: a single press fires after the tap window (or on \
             release if only a long press is set).",
        )
        .size(13)
        .color(MUTED_COLOR)
        .into(),
        value_slider("Tap window", 100.0..=600.0, p.tap_window_ms as f32, 10.0, "ms", Message::SetTapWindow),
        value_slider("Long press after", 200.0..=1500.0, p.long_press_ms as f32, 50.0, "ms", Message::SetLongPress),
    ];
    for b in Button::ALL {
        let gestures = p.gestures.get(&b);
        let missing: Vec<GestureKind> = GestureKind::ALL
            .into_iter()
            .filter(|k| gestures.and_then(|g| g.get(*k)).is_none())
            .collect();
        let mut line = row![labeled(b.to_string(), action_editor(p.button(b), b, false, set_action(Target::Button(b)), KeyField::root(Target::Button(b))))]
            .align_y(Alignment::Center);
        if !missing.is_empty() {
            line = line.push(space::horizontal()).push(
                pick_list(missing, None::<GestureKind>, move |k| Message::AddGesture(b, k))
                    .placeholder("+ Gesture")
                    .width(130),
            );
        }
        rows.push(line.into());

        for kind in GestureKind::ALL {
            if let Some(action) = gestures.and_then(|g| g.get(kind)) {
                rows.push(labeled(
                    format!("    {kind}"),
                    row![
                        action_editor(action, b, false, set_action(Target::Gesture(b, kind)), KeyField::root(Target::Gesture(b, kind))),
                        button(text("✕").size(13))
                            .style(button::secondary)
                            .on_press(Message::RemoveGesture(b, kind)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into(),
                ));
            }
        }
    }
    rows
}

fn combo_rows(p: &Profile) -> Vec<Element<'_, Message>> {
    let mut rows: Vec<Element<'_, Message>> = vec![
        text(format!(
            "Buttons pressed within the window act as one input. Combo buttons wait up to {} ms \
             before acting alone; a combo button set to Disabled works as a modifier with no time limit.",
            p.combo_window_ms
        ))
        .size(13)
        .color(MUTED_COLOR)
        .into(),
        value_slider("Combo window", 20.0..=300.0, p.combo_window_ms as f32, 5.0, "ms", Message::SetComboWindow),
    ];
    for (i, combo) in p.combos.iter().enumerate() {
        let mut members = row![].spacing(6).align_y(Alignment::Center);
        for (n, &b) in combo.buttons.iter().enumerate() {
            if n > 0 {
                members = members.push(text("+"));
            }
            members = members.push(
                button(text(format!("{} ✕", short_button(b))).size(13))
                    .style(button::secondary)
                    .on_press(Message::RemoveComboButton(i, b)),
            );
        }
        let remaining: Vec<Button> =
            Button::ALL.into_iter().filter(|b| !combo.buttons.contains(b)).collect();
        members = members.push(
            pick_list(remaining, None::<Button>, move |b| Message::AddComboButton(i, b))
                .placeholder("Add button…")
                .width(170),
        );
        rows.push(
            container(
                column![
                    row![
                        members,
                        space::horizontal(),
                        button(text("Remove combo").size(13))
                            .style(button::danger)
                            .on_press(Message::RemoveCombo(i)),
                    ]
                    .align_y(Alignment::Center),
                    labeled("    Action", action_editor(&combo.action, Button::South, false, set_action(Target::Combo(i)), KeyField::root(Target::Combo(i)))),
                ]
                .spacing(8),
            )
            .padding(10)
            .style(container::bordered_box)
            .into(),
        );
    }
    rows.push(button(text("+ Add combo")).style(button::secondary).on_press(Message::AddCombo).into());
    rows
}

fn short_button(b: Button) -> &'static str {
    match b {
        Button::South => "A",
        Button::East => "B",
        Button::North => "Y",
        Button::West => "X",
        Button::LeftBumper => "LB",
        Button::RightBumper => "RB",
        Button::Select => "Select",
        Button::Start => "Start",
        Button::Guide => "Guide",
        Button::LeftStick => "LS",
        Button::RightStick => "RS",
        Button::DpadUp => "Up",
        Button::DpadDown => "Down",
        Button::DpadLeft => "Left",
        Button::DpadRight => "Right",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionKind {
    Disabled,
    Gamepad,
    Keys,
    Mouse,
    NextProfile,
    Multiple,
}

impl fmt::Display for ActionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ActionKind::Disabled => "Disabled",
            ActionKind::Gamepad => "Gamepad button",
            ActionKind::Keys => "Keyboard",
            ActionKind::Mouse => "Mouse button",
            ActionKind::NextProfile => "Next profile",
            ActionKind::Multiple => "Multiple…",
        })
    }
}

const ACTION_KINDS: [ActionKind; 6] = [
    ActionKind::Disabled,
    ActionKind::Gamepad,
    ActionKind::Keys,
    ActionKind::Mouse,
    ActionKind::NextProfile,
    ActionKind::Multiple,
];

/// Callback that turns an edited action into a message; lets editors nest inside `Multi`.
type OnAction<'a> = Rc<dyn Fn(ButtonAction) -> Message + 'a>;

fn set_action<'a>(target: Target) -> OnAction<'a> {
    Rc::new(move |a| Message::SetAction(target, a))
}

/// Editor for one action. `nested` editors (entries of a Multiple list) can't be Multiple.
fn action_editor<'a>(
    action: &'a ButtonAction,
    default_button: Button,
    nested: bool,
    on_change: OnAction<'a>,
    field: KeyField,
) -> Element<'a, Message> {
    let kind = match action {
        ButtonAction::Disabled => ActionKind::Disabled,
        ButtonAction::Gamepad(_) => ActionKind::Gamepad,
        ButtonAction::Keys(_) => ActionKind::Keys,
        ButtonAction::Mouse(_) => ActionKind::Mouse,
        ButtonAction::NextProfile => ActionKind::NextProfile,
        ButtonAction::Multi(_) => ActionKind::Multiple,
    };
    let kinds: &'static [ActionKind] = if nested { &ACTION_KINDS[..5] } else { &ACTION_KINDS };
    let kind_picker = {
        let on_change = on_change.clone();
        pick_list(kinds, Some(kind), move |k| {
            on_change(match k {
                ActionKind::Disabled => ButtonAction::Disabled,
                ActionKind::Gamepad => ButtonAction::Gamepad(default_button),
                ActionKind::Keys => ButtonAction::Keys(Vec::new()),
                ActionKind::Mouse => ButtonAction::Mouse(MouseButton::Left),
                ActionKind::NextProfile => ButtonAction::NextProfile,
                // Keep what was there as the first entry.
                ActionKind::Multiple => ButtonAction::Multi(match action {
                    ButtonAction::Disabled | ButtonAction::Multi(_) => Vec::new(),
                    other => vec![other.clone()],
                }),
            })
        })
        .width(170)
    };

    let value: Element<'a, Message> = match action {
        ButtonAction::Gamepad(b) => pick_list(Button::ALL, Some(*b), move |b| {
            on_change(ButtonAction::Gamepad(b))
        })
        .width(220)
        .into(),
        ButtonAction::Mouse(m) => pick_list(MouseButton::ALL, Some(*m), move |m| {
            on_change(ButtonAction::Mouse(m))
        })
        .width(220)
        .into(),
        ButtonAction::Keys(keys) => key_input(
            &keys_to_text(keys),
            "e.g. LEFTCTRL+C",
            move |s| on_change(ButtonAction::Keys(text_to_keys(&s))),
            Message::OpenKeyPicker(field, keys.clone(), false),
        ),
        ButtonAction::Multi(list) => multi_editor(list, default_button, on_change, field),
        ButtonAction::Disabled | ButtonAction::NextProfile => space().into(),
    };
    row![kind_picker, value].spacing(8).align_y(Alignment::Start).into()
}

/// List of simultaneous actions, each with its own editor and a remove button.
fn multi_editor<'a>(
    list: &'a [ButtonAction],
    default_button: Button,
    on_change: OnAction<'a>,
    field: KeyField,
) -> Element<'a, Message> {
    let with = |f: &dyn Fn(&mut Vec<ButtonAction>)| {
        let mut v = list.to_vec();
        f(&mut v);
        on_change(ButtonAction::Multi(v))
    };
    let mut col = column![].spacing(6);
    for (i, sub) in list.iter().enumerate() {
        let parent = on_change.clone();
        let entry: OnAction<'a> = Rc::new(move |a| {
            let mut v = list.to_vec();
            v[i] = a;
            parent(ButtonAction::Multi(v))
        });
        col = col.push(
            row![
                action_editor(sub, default_button, true, entry, field.child(i)),
                button(text("✕").size(13)).style(button::secondary).on_press(with(&|v| {
                    v.remove(i);
                })),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    col.push(
        button(text("+ Add output").size(13))
            .style(button::secondary)
            .on_press(with(&|v| v.push(ButtonAction::Keys(Vec::new())))),
    )
    .into()
}

/// Text input for evdev key names (without the `KEY_` prefix) that turns red when invalid.
/// Also offers a button that opens the on-screen keyboard (`open_picker`).
fn key_input<'a>(
    value: &str,
    placeholder: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    open_picker: Message,
) -> Element<'a, Message> {
    let valid = value.is_empty()
        || value.split('+').all(|k| KeyCode::from_str(&format!("KEY_{k}")).is_ok());
    let input = text_input(placeholder, value).on_input(on_input).width(180);
    let pick = button(text("⌨").size(14)).style(button::secondary).on_press(open_picker);
    let mut r = row![input, pick].spacing(6).align_y(Alignment::Center);
    if !valid {
        r = r.push(text("unknown key").size(12).color(ERROR_COLOR));
    }
    r.into()
}

fn short_key(k: &str) -> &str {
    k.strip_prefix("KEY_").unwrap_or(k)
}

fn keys_to_text(keys: &[String]) -> String {
    keys.iter().map(|k| short_key(k)).collect::<Vec<_>>().join("+")
}

fn text_to_keys(s: &str) -> Vec<String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();
    if s.is_empty() {
        return Vec::new();
    }
    s.split('+').map(|k| format!("KEY_{k}")).collect()
}

fn single_key(s: &str) -> String {
    text_to_keys(s).into_iter().next().unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StickKind {
    Disabled,
    Gamepad,
    Mouse,
    Scroll,
    Keys,
}

impl fmt::Display for StickKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StickKind::Disabled => "Disabled",
            StickKind::Gamepad => "Gamepad stick",
            StickKind::Mouse => "Mouse pointer",
            StickKind::Scroll => "Scroll wheel",
            StickKind::Keys => "Direction keys",
        })
    }
}

fn stick_editor(s: Stick, cfg: &StickConfig) -> Element<'_, Message> {
    let kind = match cfg.action {
        StickAction::Disabled => StickKind::Disabled,
        StickAction::Gamepad { .. } => StickKind::Gamepad,
        StickAction::Mouse { .. } => StickKind::Mouse,
        StickAction::Scroll { .. } => StickKind::Scroll,
        StickAction::Keys { .. } => StickKind::Keys,
    };
    let with = move |action: StickAction| {
        let mut c = cfg.clone();
        c.action = action;
        Message::SetStick(s, c)
    };
    let kinds = [StickKind::Disabled, StickKind::Gamepad, StickKind::Mouse, StickKind::Scroll, StickKind::Keys];
    let picker = pick_list(kinds, Some(kind), move |k| {
        with(match k {
            StickKind::Disabled => StickAction::Disabled,
            StickKind::Gamepad => StickAction::Gamepad { stick: s, invert_y: false },
            StickKind::Mouse => StickAction::Mouse { speed: 1200.0 },
            StickKind::Scroll => StickAction::Scroll { speed: 15.0 },
            StickKind::Keys => wasd(),
        })
    })
    .width(170);

    let mut rows = column![labeled(s.to_string(), picker.into())].spacing(8);

    match &cfg.action {
        StickAction::Gamepad { stick, invert_y } => {
            let invert_y = *invert_y;
            let stick = *stick;
            rows = rows.push(labeled(
                "    Output",
                row![
                    pick_list([Stick::Left, Stick::Right], Some(stick), move |t| {
                        with(StickAction::Gamepad { stick: t, invert_y })
                    })
                    .width(170),
                    checkbox(invert_y)
                        .label("Invert Y")
                        .on_toggle(move |inv| with(StickAction::Gamepad { stick, invert_y: inv })),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
            ));
        }
        StickAction::Mouse { speed } => {
            rows = rows.push(value_slider("    Speed", 100.0..=4000.0, *speed, 50.0, "px/s", move |v| {
                with(StickAction::Mouse { speed: v })
            }));
        }
        StickAction::Scroll { speed } => {
            rows = rows.push(value_slider("    Speed", 1.0..=60.0, *speed, 1.0, "notches/s", move |v| {
                with(StickAction::Scroll { speed: v })
            }));
        }
        StickAction::Keys { up, down, left, right } => {
            let full = [up, down, left, right].map(|k| k.clone());
            let keys = full.clone().map(|k| short_key(&k).to_string());
            let current = keys.clone();
            let set = move |i: usize, v: String| {
                let mut k = current.clone().map(|k| format!("KEY_{k}"));
                k[i] = single_key(&v);
                let [up, down, left, right] = k;
                with(StickAction::Keys { up, down, left, right })
            };
            let field = |i: usize, label: &'static str| {
                let set = set.clone();
                let open = Message::OpenKeyPicker(KeyField::StickDir { stick: s, dir: i }, vec![full[i].clone()], true);
                column![text(label).size(12).color(MUTED_COLOR), key_input(&keys[i], label, move |v| set(i, v), open)]
                    .spacing(2)
            };
            rows = rows.push(labeled(
                "    Keys",
                column![
                    row![field(0, "Up"), field(1, "Down")].spacing(8),
                    row![field(2, "Left"), field(3, "Right")].spacing(8),
                    row![
                        button(text("WASD").size(13)).style(button::secondary).on_press(with(wasd())),
                        button(text("Arrows").size(13)).style(button::secondary).on_press(with(arrows())),
                    ]
                    .spacing(8),
                ]
                .spacing(6)
                .into(),
            ));
        }
        StickAction::Disabled => {}
    }

    if !matches!(cfg.action, StickAction::Disabled) {
        rows = rows.push(value_slider("    Deadzone", 0.0..=0.5, cfg.deadzone, 0.01, "", move |v| {
            let mut c = cfg.clone();
            c.deadzone = v;
            Message::SetStick(s, c)
        }));
    }
    if matches!(cfg.action, StickAction::Mouse { .. } | StickAction::Scroll { .. }) {
        rows = rows.push(value_slider("    Curve", 1.0..=3.0, cfg.curve, 0.1, "(1 = linear)", move |v| {
            let mut c = cfg.clone();
            c.curve = v;
            Message::SetStick(s, c)
        }));
    }
    if matches!(cfg.action, StickAction::Keys { .. }) {
        rows = rows.push(value_slider("    Press keys at", 0.05..=0.95, cfg.key_threshold, 0.05, "", move |v| {
            let mut c = cfg.clone();
            c.key_threshold = v;
            Message::SetStick(s, c)
        }));
    }
    rows.push(zone_editor(Analog::Stick(s), &cfg.zones)).into()
}

/// Extra actions held while the stick/trigger is within a range of travel.
fn zone_editor(analog: Analog, zones: &[Zone]) -> Element<'_, Message> {
    let what = match analog {
        Analog::Stick(_) => "pushed",
        Analog::Trigger(_) => "pulled",
    };
    let mut col = column![text(format!(
        "Zones: extra outputs held while it is {what} within a range (0 = just past rest, 1 = all the way)."
    ))
    .size(12)
    .color(MUTED_COLOR)]
    .spacing(8);

    for (i, zone) in zones.iter().enumerate() {
        let (min, max) = (zone.min, zone.max);
        let range = row![
            text("From").size(13),
            slider(0.0..=1.0, min, move |v| Message::SetZoneRange(analog, i, v, max)).step(0.05_f32).width(140),
            text(format!("{min:.2}")).size(13),
            text("to").size(13),
            slider(0.0..=1.0, max, move |v| Message::SetZoneRange(analog, i, min, v)).step(0.05_f32).width(140),
            text(format!("{max:.2}")).size(13),
            space::horizontal(),
            button(text("✕").size(13)).style(button::secondary).on_press(Message::RemoveZone(analog, i)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let mut body = column![
            range,
            action_editor(&zone.action, Button::South, false, set_action(Target::Zone(analog, i)), KeyField::root(Target::Zone(analog, i))),
        ]
        .spacing(8);
        if min >= max {
            body = body.push(text("The range must start below where it ends.").size(12).color(ERROR_COLOR));
        }
        col = col.push(container(body).padding(10).style(container::bordered_box));
    }

    let mut buttons = row![
        button(text("+ Add zone").size(13)).style(button::secondary).on_press(Message::AddZone(analog, ZonePreset::Empty)),
    ]
    .spacing(8);
    if matches!(analog, Analog::Stick(_)) {
        buttons = buttons.push(
            button(text("+ Walk modifier (Shift on partial push)").size(13))
                .style(button::secondary)
                .on_press(Message::AddZone(analog, ZonePreset::Walk)),
        );
    }
    labeled("    Zones", col.push(buttons).into())
}

fn wasd() -> StickAction {
    StickAction::Keys {
        up: "KEY_W".into(),
        down: "KEY_S".into(),
        left: "KEY_A".into(),
        right: "KEY_D".into(),
    }
}

fn arrows() -> StickAction {
    StickAction::Keys {
        up: "KEY_UP".into(),
        down: "KEY_DOWN".into(),
        left: "KEY_LEFT".into(),
        right: "KEY_RIGHT".into(),
    }
}

fn value_slider<'a>(
    label: &'a str,
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    step: f32,
    unit: &'a str,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    let shown = if step >= 1.0 { format!("{value:.0} {unit}") } else { format!("{value:.2} {unit}") };
    labeled(
        label,
        row![slider(range, value, on_change).step(step).width(300), text(shown).size(13)]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerKind {
    Disabled,
    Gamepad,
    Button,
}

impl fmt::Display for TriggerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TriggerKind::Disabled => "Disabled",
            TriggerKind::Gamepad => "Gamepad trigger",
            TriggerKind::Button => "Button action",
        })
    }
}

fn trigger_editor<'a>(
    t: Trigger,
    action: &'a TriggerAction,
    zones: &'a [Zone],
    analog: bool,
) -> Element<'a, Message> {
    let kind = match action {
        TriggerAction::Disabled => TriggerKind::Disabled,
        TriggerAction::Gamepad(_) => TriggerKind::Gamepad,
        TriggerAction::Button { .. } => TriggerKind::Button,
    };
    let kinds = [TriggerKind::Disabled, TriggerKind::Gamepad, TriggerKind::Button];
    let picker = pick_list(kinds, Some(kind), move |k| {
        Message::SetTrigger(
            t,
            match k {
                TriggerKind::Disabled => TriggerAction::Disabled,
                TriggerKind::Gamepad => TriggerAction::Gamepad(t),
                TriggerKind::Button => TriggerAction::Button {
                    action: ButtonAction::Mouse(MouseButton::Left),
                    threshold: 0.5,
                },
            },
        )
    })
    .width(170);

    let mut rows = column![labeled(t.to_string(), picker.into())].spacing(8);
    match action {
        TriggerAction::Gamepad(target) => {
            rows = rows.push(labeled(
                "    Output",
                pick_list([Trigger::Left, Trigger::Right], Some(*target), move |o| {
                    Message::SetTrigger(t, TriggerAction::Gamepad(o))
                })
                .width(170)
                .into(),
            ));
        }
        TriggerAction::Button { action: inner, threshold } => {
            rows = rows.push(labeled(
                "    Action",
                action_editor(inner, Button::South, false, set_action(Target::Trigger(t)), KeyField::root(Target::Trigger(t))),
            ));
            if analog {
                let inner = inner.clone();
                rows = rows.push(value_slider("    Threshold", 0.05..=0.95, *threshold, 0.05, "", move |v| {
                    Message::SetTrigger(t, TriggerAction::Button { action: inner.clone(), threshold: v })
                }));
            }
        }
        TriggerAction::Disabled => {}
    }
    if analog {
        return rows.push(zone_editor(Analog::Trigger(t), zones)).into();
    }
    rows = rows.push(labeled(
        "",
        text("Your controller's triggers are on/off only, so pull thresholds and zones don't apply.")
            .size(12)
            .color(MUTED_COLOR)
            .into(),
    ));
    // Keep existing zones reachable so they can be removed rather than silently lingering.
    if !zones.is_empty() {
        rows = rows
            .push(labeled(
                "",
                text("This profile still has zones for this trigger; a full pull acts as the top zone.")
                    .size(12)
                    .color(ERROR_COLOR)
                    .into(),
            ))
            .push(zone_editor(Analog::Trigger(t), zones));
    }
    rows.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::boot().0;
        app.editing = 0; // default "Gamepad" profile
        app
    }

    fn pick(app: &mut App, field: KeyField, single: bool, keys: &[&'static str]) {
        let _ = app.update(Message::OpenKeyPicker(field, Vec::new(), single));
        for k in keys {
            let _ = app.update(Message::PickerKey(k));
        }
        if !single {
            let _ = app.update(Message::PickerClose { apply: true });
        }
    }

    #[test]
    fn picker_writes_chord_into_nested_multi_entry() {
        let mut app = app();
        let multi = ButtonAction::Multi(vec![ButtonAction::Gamepad(Button::South), ButtonAction::Keys(vec![])]);
        app.config.profiles[0].set_button(Button::South, multi);
        let field = KeyField::root(Target::Button(Button::South)).child(1);
        // Toggling a key twice removes it again.
        pick(&mut app, field, false, &["KEY_LEFTSHIFT", "KEY_A", "KEY_B", "KEY_B"]);
        assert_eq!(
            app.config.profiles[0].button(Button::South),
            &ButtonAction::Multi(vec![
                ButtonAction::Gamepad(Button::South),
                ButtonAction::Keys(vec!["KEY_LEFTSHIFT".into(), "KEY_A".into()]),
            ])
        );
        assert!(app.picker.is_none());
    }

    #[test]
    fn single_picker_sets_stick_direction() {
        let mut app = app();
        app.config.profiles[0].left_stick.action = wasd();
        let _ = app.update(Message::OpenKeyPicker(KeyField::StickDir { stick: Stick::Left, dir: 2 }, vec![], true));
        // Single mode closes itself by scheduling PickerClose; run it like the runtime would.
        let _ = app.update(Message::PickerKey("KEY_LEFT"));
        let _ = app.update(Message::PickerClose { apply: true });
        let StickAction::Keys { left, up, .. } = &app.config.profiles[0].left_stick.action else { panic!() };
        assert_eq!((left.as_str(), up.as_str()), ("KEY_LEFT", "KEY_W"));
    }

    fn device(name: &str, analog_triggers: bool, ignored: bool) -> ipc::DeviceInfo {
        ipc::DeviceInfo { name: name.into(), path: String::new(), managed: !ignored, ignored, analog_triggers, rumble: true }
    }

    #[test]
    fn trigger_depth_settings_hide_only_when_all_pads_are_digital() {
        let mut app = app();
        let status = |devices| Status { enabled: true, active_profile: "Gamepad".into(), devices };
        assert!(app.analog_triggers(), "no daemon: assume analog");

        app.status = Some(status(vec![device("Pro Controller", false, false)]));
        assert!(!app.analog_triggers());

        app.status = Some(status(vec![device("Pro Controller", false, false), device("Xbox", true, false)]));
        assert!(app.analog_triggers());

        // An ignored Xbox pad doesn't count.
        app.status = Some(status(vec![device("Pro Controller", false, false), device("Xbox", true, true)]));
        assert!(!app.analog_triggers());

        app.status = Some(status(vec![]));
        assert!(app.analog_triggers());
    }

    #[test]
    fn cancel_leaves_config_untouched() {
        let mut app = app();
        let before = app.config.clone();
        let _ = app.update(Message::OpenKeyPicker(KeyField::root(Target::Button(Button::North)), vec![], false));
        let _ = app.update(Message::PickerKey("KEY_Q"));
        let _ = app.update(Message::PickerClose { apply: false });
        assert_eq!(app.config, before);
    }

    #[test]
    fn key_text_roundtrip() {
        for s in ["", "LEFTCTRL+C", "LEFTCTRL+", "A"] {
            assert_eq!(keys_to_text(&text_to_keys(s)), s);
        }
        assert_eq!(text_to_keys("leftctrl + c"), vec!["KEY_LEFTCTRL", "KEY_C"]);
    }
}

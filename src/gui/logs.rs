//! The Log overlays tab: on-screen logs of the buttons pressed lately and what each did.

use iced::widget::{checkbox, column, row};

use super::*;
use crate::{config::{InputLogSettings, LogEnd, LogSource}, info::Glyphs};

impl fmt::Display for LogEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LogEnd::Top => "Newest on top",
            LogEnd::Bottom => "Newest at the bottom",
        })
    }
}

/// A copy of `o` with `f` applied, as the message that saves it.
type Edit<'a> = Rc<dyn Fn(&dyn Fn(&mut LogOverlay)) -> Message + 'a>;

fn settings(o: &mut LogOverlay) -> &mut InputLogSettings {
    let LogSource::Input(s) = &mut o.source;
    s
}

fn view_new_log_card<'a>() -> Element<'a, Message> {
    container(
        row![
            button(text("+ New log overlay")).style(style::secondary).on_press(Message::NewLog),
            text("The buttons just pressed, and what each did.").size(13).color(MUTED_COLOR),
            space::horizontal(),
            help(
                "A log overlay lists recent input on screen, one line per burst of presses: a fireball \
                 motion reads ↓ ↘ → X. Under each button is what it did when it's mapped to something \
                 else, and how long it was held. Show it always while its game is active, or with the \
                 \"Show log overlay…\" action. For just the latest burst inside an info overlay, use the \
                 {current_input} token there."
                    .into(),
            ),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(14)
    .width(Length::Fill)
    .style(style::card)
    .into()
}

impl App {
    pub(super) fn logs(&self) -> &Vec<LogOverlay> {
        if self.on_shared() { &self.config.shared.logs } else { &self.game().logs }
    }

    pub(super) fn logs_mut(&mut self) -> &mut Vec<LogOverlay> {
        if self.on_shared() { &mut self.config.shared.logs } else { &mut self.game_mut().logs }
    }

    /// The names of the shown log overlays, for name checks.
    pub(super) fn log_names(&self) -> Vec<&String> {
        self.logs().iter().map(|o| &o.name).collect()
    }

    pub(super) fn logs_have_problem(&self) -> bool {
        logs_problem(self.logs(), None).is_some()
    }

    pub(super) fn view_logs(&self) -> Element<'_, Message> {
        let mut col = column![view_new_log_card()].spacing(16);
        if self.logs().is_empty() {
            col = col.push(text("This setup has no log overlays yet.").color(MUTED_COLOR));
        }
        for (i, o) in self.logs().iter().enumerate() {
            col = col.push(self.view_log_card(i, o));
        }
        col.into()
    }

    fn view_log_card<'a>(&'a self, i: usize, o: &'a LogOverlay) -> Element<'a, Message> {
        let open = self.open_logs.contains(&i);
        let problem = self.name_problem(ItemKind::Log, i, &o.name);
        let chevron = if open { "▾" } else { "▸" };
        let title = text(format!("{chevron}  {}", if o.name.is_empty() { "(unnamed)" } else { &o.name })).size(18);
        let title = if problem.is_some() { title.color(ERROR_COLOR) } else { title };
        let LogSource::Input(s) = &o.source;
        let shown = if o.always { "always shown" } else { "shown by an action" };
        let summary = format!("Input · {} lines · {} · {shown}", s.lines, DeviceChoice(s.tracking.device).to_string().to_lowercase());
        let mut header = row![
            button(title).style(button::text).padding(0).on_press(Message::ToggleLog(i)),
            text(summary).size(13).color(MUTED_COLOR),
            space::horizontal(),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        if let Some(problem) = problem {
            header = header.push(text(format!("⚠ {problem}")).size(12).color(ERROR_COLOR));
        }
        header = header.push(button(text("✕").size(14)).style(style::quiet_danger).on_press(Message::DeleteLog(i)));
        let mut col = column![header].spacing(12);
        if open {
            col = col.push(self.view_log_editor(i, o));
        }
        container(col).padding(14).width(Length::Fill).style(style::card).into()
    }

    fn view_log_editor<'a>(&'a self, i: usize, o: &'a LogOverlay) -> Element<'a, Message> {
        let edit: Edit<'a> = Rc::new(move |f| {
            let mut o = o.clone();
            f(&mut o);
            Message::SetLog(i, o)
        });
        let LogSource::Input(s) = &o.source;
        let mut rows: Vec<Element<'a, Message>> = vec![
            labeled("Name", field("Log overlay name", &o.name).on_input(move |n| Message::RenameLog(i, n)).width(240).into()),
            self.view_log_shown(o, &edit),
        ];
        if !o.always {
            let e = edit.clone();
            rows.push(labeled(
                "Lingers",
                seconds_option("After it's let go (or toggled off), stays for", o.linger, DEFAULT_LINGER_SECONDS, move |v| {
                    e(&|o| o.linger = v)
                }),
            ));
        }
        let e = edit.clone();
        let on_tracking: OnTracking<'a> = Rc::new(move |t| e(&|o| settings(o).tracking = t.clone()));
        rows.extend(self.tracking_editor((ItemKind::Log, i), &s.tracking, "line", &on_tracking));
        rows.extend(input_settings(s, &edit));

        let appearance_open = self.open_log_appearance.contains(&i);
        rows.push(labeled("", disclosure("Appearance", appearance_open, Message::ToggleLogAppearance(i))));
        if appearance_open {
            let e = edit.clone();
            rows.push(style_editor(&o.style, Rc::new(move |style| e(&|o| o.style = style.clone()))));
        }
        let sample = crate::inputlog::Inputs::sample();
        let now = sample.now.unwrap_or_else(std::time::Instant::now);
        let glyphs = Glyphs { family: self.config.info_glyphs, nintendo_layout: self.nintendo_layout() };
        let view = crate::inputlog::log_view(&sample.entries(None), s, &preview_style(&o.style), glyphs, now);
        rows.push(preview(crate::overlay::draw::log_panel(&view, self.preview_font(), &crate::motion::Anim::still())));
        column(rows).spacing(12).into()
    }

    fn view_log_shown<'a>(&self, o: &LogOverlay, edit: &Edit<'a>) -> Element<'a, Message> {
        let e = edit.clone();
        labeled(
            "Shown",
            column![
                checkbox(o.always)
                    .label(if self.on_shared() { "Always, in every setup" } else { "Always, while this setup is active" })
                    .on_toggle(move |a| e(&|o| o.always = a)),
                text(if o.always {
                    "Stays on screen whenever the setup is active, so actions can't show it."
                } else {
                    "Map \"Show log overlay…\" to a button: it shows while held, or until pressed again if \
                     wrapped in Toggle."
                })
                .size(12)
                .color(MUTED_COLOR),
            ]
            .spacing(4)
            .into(),
        )
    }

    pub(super) fn update_logs(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NewLog => {
                let name = self.new_item_name(ItemKind::Log, "Input log");
                let overlay = LogOverlay { always: true, ..LogOverlay::new(&name) };
                // New ones go first, right under the button that made them, already open.
                self.logs_mut().insert(0, overlay);
                self.open_logs = self.open_logs.iter().map(|j| j + 1).collect();
                self.open_logs.insert(0);
                self.open_log_appearance = self.open_log_appearance.iter().map(|j| j + 1).collect();
            }
            Message::ToggleLog(i) => {
                if !self.open_logs.remove(&i) {
                    self.open_logs.insert(i);
                }
            }
            Message::ToggleLogAppearance(i) => {
                if !self.open_log_appearance.remove(&i) {
                    self.open_log_appearance.insert(i);
                }
            }
            Message::DeleteLog(i) => {
                if i < self.logs().len() {
                    self.logs_mut().remove(i);
                    self.open_logs = shift_removed(&self.open_logs, i);
                    self.open_log_appearance = shift_removed(&self.open_log_appearance, i);
                }
            }
            Message::RenameLog(i, name) => {
                if self.item_name_free(ItemKind::Log, Some(i), &name)
                    && let Some(o) = self.logs_mut().get_mut(i)
                {
                    let old = std::mem::replace(&mut o.name, name.clone());
                    // Keep every "Show log overlay" pointing at it.
                    self.follow_item_rename(ItemKind::Log, &old, &name);
                }
            }
            Message::ToggleInputGroup(kind, i, g) => {
                if !self.open_input_groups.remove(&(kind, i, g)) {
                    self.open_input_groups.insert((kind, i, g));
                }
            }
            Message::SetLog(i, overlay) => {
                // The name only changes through RenameLog, which follows references.
                if let Some(o) = self.logs_mut().get_mut(i) {
                    *o = LogOverlay { name: o.name.clone(), ..overlay };
                }
            }
            other => return self.update_profile(other),
        }
        Task::none()
    }
}

/// The log's own settings: lines, order, fading and what's shown.
fn input_settings<'a>(s: &InputLogSettings, edit: &Edit<'a>) -> Vec<Element<'a, Message>> {
    let e = edit.clone();
    let lines = value_slider("Lines", 1.0..=20.0, f32::from(s.lines), 1.0, "", move |v| e(&|o| settings(o).lines = v as u8));
    let e = edit.clone();
    let newest = dropdown([LogEnd::Top, LogEnd::Bottom], Some(s.newest), move |end| e(&|o| settings(o).newest = end)).width(200);
    let e = edit.clone();
    let fade = (s.fade_after > 0.0).then_some(s.fade_after);
    let fade = seconds_option("Each line fades away after", fade, 5.0, move |v| e(&|o| settings(o).fade_after = v.unwrap_or(0.0)));
    let check = |label: &'static str, on: bool, set: fn(&mut InputLogSettings, bool)| -> Element<'a, Message> {
        let e = edit.clone();
        checkbox(on).label(label).on_toggle(move |v| e(&|o| set(settings(o), v))).into()
    };
    let show = column![
        check("Merge repeats (X X X shows as X×3)", s.merge_repeats, |s, v| s.merge_repeats = v),
        check("What each press did, when remapped", s.show_labels, |s, v| s.show_labels = v),
        check("How long each press was held", s.show_holds, |s, v| s.show_holds = v),
    ]
    .spacing(6);
    vec![
        lines,
        labeled("Order", newest.into()),
        labeled("Fading", fade),
        labeled("Show", show.into()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::tests::*;

    #[test]
    fn log_overlay_editing_renames_and_checks() {
        let mut app = app();
        let _ = app.update(Message::SelectGameTab(GameTab::Logs));
        let _ = app.update(Message::NewLog);
        assert!(app.open_logs.contains(&0));
        let _ = app.view();
        let mut edited = app.config.general.logs[0].clone();
        settings(&mut edited).lines = 3;
        edited.always = false;
        let _ = app.update(Message::SetLog(0, edited));
        assert_eq!(app.config.general.logs[0].source, LogSource::Input(InputLogSettings { lines: 3, ..InputLogSettings::default() }));

        // "Show log overlay" mappings follow a rename; a missing one blocks saving.
        app.config.general.profiles[0].set_button(Button::Select, ButtonAction::ShowLog("Input log".into()));
        let _ = app.update(Message::RenameLog(0, "Inputs".into()));
        assert_eq!(app.config.general.profiles[0].button(Button::Select), &ButtonAction::ShowLog("Inputs".into()));
        assert_eq!(app.validate(), None);
        let _ = app.update(Message::SetLog(0, LogOverlay { always: true, ..app.config.general.logs[0].clone() }));
        assert!(app.validate().unwrap().contains("always shown"), "{:?}", app.validate());
        let _ = app.update(Message::DeleteLog(0));
        assert!(app.validate().unwrap().contains("missing log overlay"));
        assert!(app.open_logs.is_empty());
    }

    #[test]
    fn input_settings_show_once_an_info_overlay_has_current_input() {
        let mut app = app();
        let _ = app.update(Message::SelectGameTab(GameTab::Info));
        let _ = app.update(Message::NewInfo);
        let _ = app.update(Message::SetInfoCell(0, 0, 0, "{current_input}".into()));
        let _ = app.update(Message::ToggleInputGroup(ItemKind::Info, 0, 0));
        let _ = app.view();
        let mut input = app.config.general.info[0].current_input.clone();
        input.tracking.set_tracked(crate::config::TrackedInput::Dpad, false);
        input.tracking.max_inputs = 4;
        let _ = app.update(Message::SetInfoInput(0, input.clone()));
        assert_eq!(app.config.general.info[0].current_input, input);
    }
}

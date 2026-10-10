//! The Guide tab of the profile editor: the notes for the Notes overlay, and the list behind the
//! Mapping guide (the Mappings box on the Guide overlay): each row's text, showing or hiding it,
//! and sharing a row with other inputs.

use iced::{
    Length,
    widget::{button, checkbox, row, text, text_editor},
};

use super::{App, Message, widgets::{dropdown, field, section}};
use crate::config::{GuideInput, Profile, RowId, RowKey, Stick, Trigger, default_text, editable_rows};

/// The Guide tab's notes editor, which the app keeps.
#[derive(Clone, Copy)]
pub(super) struct GuideView<'a> {
    pub(super) notes: &'a text_editor::Content,
}

#[derive(Debug, Clone)]
pub(super) enum GuideMsg {
    Notes(text_editor::Action),
    Text(RowKey, String),
    Hidden(RowKey, bool),
    /// Starts a merged row with the row's input and another, or adds that other to the row's
    /// merged row if the input is on one.
    Share(RowKey, GuideInput),
    Split(usize),
}

/// An input offered in a "share with" choice: its label, and the input.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Choice(GuideInput, String);

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}

impl App {
    pub(super) fn update_guide(&mut self, msg: GuideMsg) {
        match msg {
            GuideMsg::Notes(action) => {
                self.guide_notes.perform(action);
                let text = self.guide_notes.text();
                if let Some(p) = self.profile_mut() {
                    p.guide.notes = text.strip_suffix('\n').unwrap_or(&text).to_string();
                }
            }
            GuideMsg::Text(key, text) => {
                if let Some(p) = self.profile_mut() {
                    p.guide.set_text(key, text);
                }
            }
            GuideMsg::Hidden(key, hidden) => {
                if let Some(p) = self.profile_mut() {
                    p.guide.set_hidden(key, hidden);
                }
            }
            GuideMsg::Share(key, other) => {
                if let Some(p) = self.profile_mut() {
                    match key {
                        RowKey::Merged(i) => p.guide.add_to_merge(i, other),
                        RowKey::Row(RowId::Input(input)) => {
                            let text = default_text(p, input);
                            p.guide.merge(vec![input, other], text);
                        }
                        RowKey::Row(_) => {}
                    }
                }
            }
            GuideMsg::Split(i) => {
                if let Some(p) = self.profile_mut() {
                    p.guide.split(i);
                }
            }
        }
    }

    /// Loads the notes editor from the profile, when the Guide tab is shown or another profile is opened.
    pub(super) fn reload_guide_notes(&mut self) {
        let notes = self.profile().map(|p| p.guide.notes.clone()).unwrap_or_default();
        self.guide_notes = text_editor::Content::with_text(&notes);
    }
}

/// The Guide tab's sections for `p`. Inside a layer the tab is shown but not editable, since the
/// Guide settings belong to the profile.
pub(super) fn sections<'a>(p: &'a Profile, guide: GuideView<'a>, in_layer: bool) -> Vec<iced::Element<'a, Message>> {
    if in_layer {
        return vec![section(
            "Guide",
            Some("The Guide overlay belongs to the profile. Open this profile to edit its notes and the mapping guide.".into()),
            Vec::new(),
        )];
    }
    let notes = text_editor(guide.notes)
        .placeholder("Tips for the player: what the controls don't cover, when to switch layers, and so on.")
        .on_action(|action| Message::Guide(GuideMsg::Notes(action)))
        .height(Length::Fixed(140.0));
    let rows = editable_rows(p);
    // Inputs with a row of their own, which another row can share.
    let free: Vec<Choice> = rows
        .iter()
        .filter_map(|r| match r.key {
            RowKey::Row(RowId::Input(input)) => Some(Choice(input, format!("{} {}", input_label(input), r.default_text))),
            RowKey::Row(_) | RowKey::Merged(_) => None,
        })
        .collect();
    let list: Vec<iced::Element<'a, Message>> = rows
        .iter()
        .map(|r| {
            let others: Vec<Choice> = free.iter().filter(|c| !r.inputs.contains(&c.0)).cloned().collect();
            mapping_row(r, others)
        })
        .collect();
    vec![
        section(
            "Notes",
            Some("Shown on the left of the Guide overlay. Plain text; {buttons} show as glyphs, like info overlays.".into()),
            vec![notes.into()],
        ),
        section(
            "Mapping guide",
            Some("The list behind the Mappings box on the right of the Guide overlay. Each row says what an input, gesture or combo does: change its text, or show or hide it (the mapping still works). Inputs can also share a row with another input, such as both D-pad sides for \"Lean\". Sharing with a shared row adds to it. Split takes a shared row apart.".into()),
            list,
        ),
    ]
}

/// The short name an input's row starts with: a button's label, or LT, RT, LS and RS.
fn input_label(input: GuideInput) -> &'static str {
    match input {
        GuideInput::Button(b) => super::profile::short_button(b),
        GuideInput::Trigger(Trigger::Left) => "LT",
        GuideInput::Trigger(Trigger::Right) => "RT",
        GuideInput::Stick(Stick::Left) => "LS",
        GuideInput::Stick(Stick::Right) => "RS",
    }
}

/// One row of the list: its glyphs, its text, and its controls.
fn mapping_row<'a>(r: &crate::config::EditableRow, others: Vec<Choice>) -> iced::Element<'a, Message> {
    let labels: Vec<String> = r.inputs.iter().map(|i| input_label(*i).to_string()).collect();
    let glyphs = r.shape.join(&labels);
    let key = r.key.clone();
    let placeholder = if r.default_text.is_empty() { "Text" } else { r.default_text.as_str() };
    let edit = field(placeholder, &r.text).on_input({
        let key = key.clone();
        move |t| Message::Guide(GuideMsg::Text(key.clone(), t))
    })
    .width(Length::Fill);
    let shown = checkbox(!r.hidden).label("Show").on_toggle({
        let key = key.clone();
        move |on| Message::Guide(GuideMsg::Hidden(key.clone(), !on))
    });
    let share: iced::Element<'a, Message> = match r.key.clone() {
        RowKey::Merged(i) => {
            let split = button(text("Split").size(13)).on_press(Message::Guide(GuideMsg::Split(i)));
            if others.is_empty() {
                split.into()
            } else {
                row![
                    dropdown(others, None::<Choice>, move |c: Choice| Message::Guide(GuideMsg::Share(key.clone(), c.0)))
                        .placeholder("Add an input"),
                    split
                ]
                .spacing(8)
                .into()
            }
        }
        RowKey::Row(RowId::Input(_)) if !others.is_empty() => {
            dropdown(others, None::<Choice>, move |c: Choice| Message::Guide(GuideMsg::Share(key.clone(), c.0)))
                .placeholder("Share with")
                .into()
        }
        RowKey::Row(_) => text("").into(),
    };
    row![text(glyphs).size(14).width(Length::Fixed(140.0)), edit, shown, share].spacing(10).into()
}

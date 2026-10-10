//! The Guide tab of the profile editor: the notes for the Notes overlay, and the list behind the
//! Mapping guide (the Mappings box on the Guide overlay): each row's text, showing or hiding it,
//! sharing a row with other inputs, and rows the author writes.

use iced::{
    Alignment, Length,
    widget::{button, checkbox, container, row, space, text, text_editor},
};

use super::{App, Message, style, pieces::{self, Swap}, widgets::{dropdown, field, section}};
use crate::config::{Button, GuideInput, Profile, RowId, RowKey, Stick, Trigger, default_text, editable_rows};
use crate::info::PadFamily;

/// The Guide tab's notes editor, which the app keeps, and how the glyphs are drawn.
#[derive(Clone, Copy)]
pub(super) struct GuideView<'a> {
    pub(super) notes: &'a text_editor::Content,
    /// Whose glyphs the rows are drawn with, and how the Nintendo layout shows on the face buttons.
    pub(super) family: PadFamily,
    pub(super) swap: Swap,
}

#[derive(Debug, Clone)]
pub(super) enum GuideMsg {
    Notes(text_editor::Action),
    Text(RowKey, String),
    Hidden(RowKey, bool),
    /// Starts a merged row with the row's input and another, adds that other to the row's merged
    /// row, or adds it to a custom row.
    Share(RowKey, GuideInput),
    Split(usize),
    AddCustom,
    RemoveCustom(usize),
}

/// An input offered in a choice: its label, and the input.
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
                        RowKey::Custom(i) => p.guide.add_custom_input(i, other),
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
            GuideMsg::AddCustom => {
                if let Some(p) = self.profile_mut() {
                    p.guide.add_custom(String::new());
                }
            }
            GuideMsg::RemoveCustom(i) => {
                if let Some(p) = self.profile_mut() {
                    p.guide.remove_custom(i);
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

/// Every input a row can name: the buttons (Guide excepted), the triggers and the sticks.
fn all_inputs() -> Vec<GuideInput> {
    let mut inputs: Vec<GuideInput> = Button::ALL.into_iter().filter(|b| *b != Button::Guide).map(GuideInput::Button).collect();
    inputs.extend([Trigger::Left, Trigger::Right].map(GuideInput::Trigger));
    inputs.extend([Stick::Left, Stick::Right].map(GuideInput::Stick));
    inputs
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
    let label = |input: GuideInput| pieces::input_text(input, guide.family, guide.swap);
    // Inputs with a row of their own, which another row can share.
    let free: Vec<Choice> = rows
        .iter()
        .filter_map(|r| match r.key {
            RowKey::Row(RowId::Input(input)) => Some(Choice(input, format!("{} {}", label(input), r.default_text))),
            RowKey::Row(_) | RowKey::Merged(_) | RowKey::Custom(_) => None,
        })
        .collect();
    let every: Vec<Choice> = all_inputs().into_iter().map(|input| Choice(input, label(input))).collect();
    let mut list: Vec<iced::Element<'a, Message>> = rows
        .iter()
        .map(|r| {
            let others: Vec<Choice> = match r.key {
                RowKey::Custom(_) => every.iter().filter(|c| !r.inputs.contains(&c.0)).cloned().collect(),
                _ => free.iter().filter(|c| !r.inputs.contains(&c.0)).cloned().collect(),
            };
            mapping_row(r, &others, guide)
        })
        .collect();
    list.push(button(text("Add a row").size(13)).on_press(Message::Guide(GuideMsg::AddCustom)).into());
    vec![
        section(
            "Notes",
            Some("Shown on the left of the Guide overlay. Plain text; {buttons} show as glyphs, like info overlays.".into()),
            vec![notes.into()],
        ),
        section(
            "Mapping guide",
            Some("The list behind the Mappings box on the right of the Guide overlay. Each row says what an input, gesture or combo does: change its text, or show or hide it (the mapping still works). Inputs can share a row with another input, such as both D-pad sides for \"Lean\". \"Add a row\" writes a row of your own, for things the mappings don't show, such as what a layer's chord does: choose its inputs, then its text.".into()),
            list,
        ),
    ]
}

/// Widths of the columns after a row's text, the same on every row so the columns line up.
const SHOW_WIDTH: f32 = 70.0;
const CONTROLS_WIDTH: f32 = 280.0;
/// Width of the Split and Remove buttons, so dropdowns beside them line up too.
const SMALL_WIDTH: f32 = 70.0;

/// One row of the list: its glyphs, its text, and its controls.
fn mapping_row<'a>(r: &crate::config::EditableRow, others: &[Choice], guide: GuideView<'_>) -> iced::Element<'a, Message> {
    let glyphs = pieces::row_glyphs(&r.inputs, r.shape, guide.family, guide.swap);
    let key = r.key.clone();
    let placeholder = if r.default_text.is_empty() { "Text" } else { r.default_text.as_str() };
    let edit = field(placeholder, &r.text).on_input({
        let key = key.clone();
        move |t| Message::Guide(GuideMsg::Text(key.clone(), t))
    })
    .width(Length::Fill);
    // Custom rows aren't hidden: they're removed instead.
    let shown: iced::Element<'a, Message> = match r.key {
        RowKey::Custom(_) => space().into(),
        _ => checkbox(!r.hidden)
            .label("Show")
            .on_toggle({
                let key = key.clone();
                move |on| Message::Guide(GuideMsg::Hidden(key.clone(), !on))
            })
            .into(),
    };
    let share = |placeholder: &'static str, key: RowKey| -> iced::Element<'a, Message> {
        if others.is_empty() {
            return space().width(Length::Fill).into();
        }
        dropdown(others.to_vec(), None::<Choice>, move |c: Choice| Message::Guide(GuideMsg::Share(key.clone(), c.0)))
            .placeholder(placeholder)
            .width(Length::Fill)
            .into()
    };
    let small = |label: &'static str, msg: GuideMsg| {
        button(text(label).size(13).width(Length::Fill).align_x(Alignment::Center))
            .style(style::secondary)
            .width(Length::Fixed(SMALL_WIDTH))
            .on_press(Message::Guide(msg))
    };
    // Every row keeps the same columns, so the text fields line up whatever controls a row has.
    let controls: iced::Element<'a, Message> = match r.key.clone() {
        RowKey::Merged(i) => row![share("Add an input", key.clone()), small("Split", GuideMsg::Split(i))].spacing(8).align_y(Alignment::Center).into(),
        RowKey::Custom(i) => row![share("Add an input", key.clone()), small("Remove", GuideMsg::RemoveCustom(i))].spacing(8).align_y(Alignment::Center).into(),
        RowKey::Row(RowId::Input(_)) => share("Share with", key.clone()),
        RowKey::Row(_) => space().into(),
    };
    row![
        container(glyphs).width(Length::Fixed(140.0)),
        edit,
        container(shown).width(Length::Fixed(SHOW_WIDTH)),
        container(controls).width(Length::Fixed(CONTROLS_WIDTH)),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

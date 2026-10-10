//! A profile's Guide settings: the notes the author wrote for the player, and how the
//! Mappings overlay lists the profile's inputs, gestures and combos (label overrides, hidden
//! rows, and merged input rows).

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{Button, GestureKind, Stick, Trigger};

/// What the Guide tab adds to a profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GuideSettings {
    /// Free text for the Notes overlay, one line per line of text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Changes to the rows of the Mappings overlay, by what each row is about.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub edits: BTreeMap<RowId, RowEdit>,
    /// Inputs that share one row on the Mappings overlay, such as both D-pad sides for "Lean".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merged: Vec<MergedRow>,
    /// Rows the author wrote, for what the profile's mappings don't show, such as a layer's chord.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<CustomRow>,
}

impl GuideSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// An input that can have a row on the Mappings overlay. Its key is `button:South`,
/// `trigger:Left` or `stick:Right`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GuideInput {
    Button(Button),
    Trigger(Trigger),
    Stick(Stick),
}

impl GuideInput {
    pub fn key(self) -> String {
        match self {
            GuideInput::Button(b) => format!("button:{b:?}"),
            GuideInput::Trigger(t) => format!("trigger:{t:?}"),
            GuideInput::Stick(s) => format!("stick:{s:?}"),
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        let (kind, name) = key.split_once(':')?;
        match kind {
            "button" => Button::ALL.into_iter().find(|b| format!("{b:?}") == name).map(GuideInput::Button),
            "trigger" => [Trigger::Left, Trigger::Right].into_iter().find(|t| format!("{t:?}") == name).map(GuideInput::Trigger),
            "stick" => [Stick::Left, Stick::Right].into_iter().find(|s| format!("{s:?}") == name).map(GuideInput::Stick),
            _ => None,
        }
    }
}

impl Serialize for GuideInput {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.key())
    }
}

impl<'de> Deserialize<'de> for GuideInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        GuideInput::from_key(&key).ok_or_else(|| serde::de::Error::custom(format!("unknown input {key:?}")))
    }
}

/// What one row of the Mappings overlay is about, which is also what its edits are keyed by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowId {
    /// An input's own row (its action).
    Input(GuideInput),
    /// A gesture a button has, such as its double tap.
    Gesture(GuideInput, GestureKind),
    /// A combo, by its buttons in a fixed order.
    Combo(Vec<GuideInput>),
}

impl RowId {
    fn key(&self) -> String {
        match self {
            RowId::Input(input) => input.key(),
            RowId::Gesture(input, kind) => format!("gesture:{}:{}", input.key(), gesture_name(*kind)),
            RowId::Combo(inputs) => {
                let parts: Vec<String> = inputs.iter().map(|i| i.key()).collect();
                format!("combo:{}", parts.join("+"))
            }
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        if let Some(rest) = key.strip_prefix("gesture:") {
            let (input, kind) = rest.rsplit_once(':')?;
            let kind = [GestureKind::DoubleTap, GestureKind::TripleTap, GestureKind::LongPress].into_iter().find(|k| gesture_name(*k) == kind)?;
            return GuideInput::from_key(input).map(|i| RowId::Gesture(i, kind));
        }
        if let Some(rest) = key.strip_prefix("combo:") {
            let inputs: Option<Vec<GuideInput>> = rest.split('+').map(GuideInput::from_key).collect();
            return inputs.map(RowId::Combo);
        }
        GuideInput::from_key(key).map(RowId::Input)
    }
}

fn gesture_name(kind: GestureKind) -> &'static str {
    match kind {
        GestureKind::DoubleTap => "double_tap",
        GestureKind::TripleTap => "triple_tap",
        GestureKind::LongPress => "long_press",
    }
}

impl Serialize for RowId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.key())
    }
}

impl<'de> Deserialize<'de> for RowId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        RowId::from_key(&key).ok_or_else(|| serde::de::Error::custom(format!("unknown row {key:?}")))
    }
}

/// How one row of the Mappings overlay differs from the default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowEdit {
    /// The text to show instead of the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Leaves the row off the overlay. The mapping still works.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

/// Several inputs on one row of the Mappings overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergedRow {
    pub inputs: Vec<GuideInput>,
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

/// What a row's glyphs stand for, which decides how they're joined and marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowShape {
    /// One input.
    Input,
    /// Several inputs sharing a row: "A, B".
    Merged,
    /// A combo's buttons pressed together: "A + B".
    Combo,
    /// A gesture on one button: "A ×2", "A ×3" or "A hold".
    Gesture(GestureKind),
}

impl RowShape {
    /// What goes between the glyphs of a row.
    pub fn separator(self) -> &'static str {
        match self {
            RowShape::Input | RowShape::Gesture(_) => " ",
            RowShape::Merged => ", ",
            RowShape::Combo => " + ",
        }
    }

    /// What follows the glyphs of a row: the mark of a gesture, which says how it's done.
    pub fn mark(self) -> &'static str {
        match self {
            RowShape::Input | RowShape::Merged | RowShape::Combo => "",
            RowShape::Gesture(GestureKind::DoubleTap) => " ×2",
            RowShape::Gesture(GestureKind::TripleTap) => " ×3",
            RowShape::Gesture(GestureKind::LongPress) => " hold",
        }
    }

    /// The glyphs of a row, given each input's glyph (`parts`, in the row's order), joined and marked.
    pub fn join(self, parts: &[String]) -> String {
        format!("{}{}", parts.join(self.separator()), self.mark())
    }
}

/// A row the author wrote: the inputs it names, and the text after them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomRow {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<GuideInput>,
    pub text: String,
}

mod rows;

pub use rows::*;

impl GuideSettings {
    /// Sets the text a row shows. Empty text goes back to the default.
    pub fn set_text(&mut self, key: RowKey, text: String) {
        match key {
            RowKey::Custom(i) => {
                if let Some(custom) = self.custom.get_mut(i) {
                    custom.text = text;
                }
            }
            RowKey::Row(id) => {
                let edit = self.edits.entry(id.clone()).or_default();
                edit.text = (!text.is_empty()).then_some(text);
                self.prune(&id);
            }
            RowKey::Merged(i) => {
                if let Some(merge) = self.merged.get_mut(i) {
                    merge.text = text;
                }
            }
        }
    }

    /// Shows or hides a row on the Mappings overlay.
    pub fn set_hidden(&mut self, key: RowKey, hidden: bool) {
        match key {
            // Custom rows aren't hidden: they're removed.
            RowKey::Custom(_) => {}
            RowKey::Row(id) => {
                self.edits.entry(id.clone()).or_default().hidden = hidden;
                self.prune(&id);
            }
            RowKey::Merged(i) => {
                if let Some(merge) = self.merged.get_mut(i) {
                    merge.hidden = hidden;
                }
            }
        }
    }

    /// Puts `inputs` on one row with `text`. They leave whatever merged rows they were on, and a
    /// row left with a single input is dropped (that input is listed on its own again).
    pub fn merge(&mut self, inputs: Vec<GuideInput>, text: String) {
        if inputs.len() < 2 {
            return;
        }
        for merge in &mut self.merged {
            merge.inputs.retain(|i| !inputs.contains(i));
        }
        self.merged.retain(|m| m.inputs.len() >= 2);
        self.merged.push(MergedRow { inputs, text, hidden: false });
    }

    /// Adds `input` to the merged row at `index`, taking it off whatever row it was on.
    pub fn add_to_merge(&mut self, index: usize, input: GuideInput) {
        for (i, merge) in self.merged.iter_mut().enumerate() {
            if i != index {
                merge.inputs.retain(|b| *b != input);
            }
        }
        if let Some(merge) = self.merged.get_mut(index)
            && !merge.inputs.contains(&input)
        {
            merge.inputs.push(input);
        }
        self.merged.retain(|m| m.inputs.len() >= 2);
    }

    /// Adds a row the author wrote, with no inputs yet.
    pub fn add_custom(&mut self, text: String) {
        self.custom.push(CustomRow { inputs: Vec::new(), text });
    }

    /// Adds `input` to the custom row at `index`.
    pub fn add_custom_input(&mut self, index: usize, input: GuideInput) {
        if let Some(custom) = self.custom.get_mut(index)
            && !custom.inputs.contains(&input)
        {
            custom.inputs.push(input);
        }
    }

    /// Removes the custom row at `index`.
    pub fn remove_custom(&mut self, index: usize) {
        if index < self.custom.len() {
            self.custom.remove(index);
        }
    }

    /// Takes a merged row apart, so its inputs are listed on their own again.
    pub fn split(&mut self, index: usize) {
        if index < self.merged.len() {
            self.merged.remove(index);
        }
    }

    /// Drops an edit that no longer changes anything, so the file only holds real edits.
    fn prune(&mut self, id: &RowId) {
        if self.edits.get(id).is_some_and(|e| e.text.is_none() && !e.hidden) {
            self.edits.remove(id);
        }
    }
}

#[cfg(test)]
mod tests;

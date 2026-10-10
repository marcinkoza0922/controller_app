//! A profile's Guide settings: the notes the author wrote for the player, and how the
//! Mappings overlay lists the profile's inputs, gestures and combos (label overrides, hidden
//! rows, and merged input rows).

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{Button, ButtonAction, GestureKind, Profile, Stick, StickAction, Trigger, TriggerAction, combo_key};
use super::summary::guide_text;

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

/// One line of the Mappings overlay: the inputs it names, and the text after them.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingRow {
    pub inputs: Vec<GuideInput>,
    pub text: String,
    pub shape: RowShape,
}

/// Which row of the Guide tab something refers to: a row of the list, or a merged input row by
/// its index in [`GuideSettings::merged`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKey {
    Row(RowId),
    Merged(usize),
    Custom(usize),
}

/// A row of the Guide tab: the mapping it stands for, the text it shows, and whether it's shown.
#[derive(Debug, Clone, PartialEq)]
pub struct EditableRow {
    pub key: RowKey,
    pub inputs: Vec<GuideInput>,
    /// The text the overlay shows: the override, or the default.
    pub text: String,
    /// The default text, which the text is replaced from.
    pub default_text: String,
    pub shape: RowShape,
    pub overridden: bool,
    pub hidden: bool,
}

/// Every input the profile maps, with its default text, in order: buttons, then triggers, then
/// sticks. Guide itself, disabled inputs and plain pass-through triggers and sticks aren't listed.
fn mapped_inputs(profile: &Profile) -> Vec<(GuideInput, String)> {
    let mut inputs = Vec::new();
    for (b, action) in &profile.buttons {
        if *b != Button::Guide && *action != ButtonAction::Disabled {
            inputs.push((GuideInput::Button(*b), guide_text(&action.pieces())));
        }
    }
    for t in [Trigger::Left, Trigger::Right] {
        if let TriggerAction::Button { action, .. } = profile.trigger(t) {
            inputs.push((GuideInput::Trigger(t), guide_text(&action.pieces())));
        }
    }
    for s in [Stick::Left, Stick::Right] {
        let text = match &profile.stick(s).action {
            StickAction::Disabled | StickAction::Gamepad { .. } => continue,
            StickAction::Mouse { .. } => "Move the mouse".to_string(),
            StickAction::Scroll { .. } => "Scroll".to_string(),
            _ => "Remapped".to_string(),
        };
        inputs.push((GuideInput::Stick(s), text));
    }
    inputs
}

/// Every row the profile's Mappings overlay can show, hidden ones included: the inputs (merged
/// inputs share one row, at the first of them), then the gestures, then the combos.
pub fn editable_rows(profile: &Profile) -> Vec<EditableRow> {
    let settings = &profile.guide;
    let mapped = mapped_inputs(profile);
    let mut rows = Vec::new();
    for (input, default_text) in &mapped {
        if let Some((index, merge)) = settings.merged.iter().enumerate().find(|(_, m)| m.inputs.contains(input)) {
            let first = mapped.iter().find(|(i, _)| merge.inputs.contains(i)).map(|(i, _)| i) == Some(input);
            if first {
                let inputs = mapped.iter().map(|(i, _)| *i).filter(|i| merge.inputs.contains(i)).collect();
                rows.push(EditableRow {
                    key: RowKey::Merged(index),
                    inputs,
                    text: merge.text.clone(),
                    default_text: String::new(),
                    shape: RowShape::Merged,
                    overridden: true,
                    hidden: merge.hidden,
                });
            }
            continue;
        }
        rows.push(row(settings, RowId::Input(*input), vec![*input], default_text.clone(), RowShape::Input));
    }
    for (button, gestures) in &profile.gestures {
        if *button == Button::Guide {
            continue;
        }
        for kind in GestureKind::ALL {
            if let Some(action) = gestures.get(kind).filter(|a| **a != ButtonAction::Disabled) {
                let input = GuideInput::Button(*button);
                rows.push(row(settings, RowId::Gesture(input, kind), vec![input], guide_text(&action.pieces()), RowShape::Gesture(kind)));
            }
        }
    }
    for combo in &profile.combos {
        if combo.buttons.len() < 2 || combo.buttons.contains(&Button::Guide) || combo.action == ButtonAction::Disabled {
            continue;
        }
        let inputs: Vec<GuideInput> = combo_key(&combo.buttons).into_iter().map(GuideInput::Button).collect();
        rows.push(row(settings, RowId::Combo(inputs.clone()), inputs, guide_text(&combo.action.pieces()), RowShape::Combo));
    }
    for (index, custom) in settings.custom.iter().enumerate() {
        let shape = if custom.inputs.len() > 1 { RowShape::Combo } else { RowShape::Input };
        rows.push(EditableRow {
            key: RowKey::Custom(index),
            inputs: custom.inputs.clone(),
            text: custom.text.clone(),
            default_text: String::new(),
            shape,
            overridden: true,
            hidden: false,
        });
    }
    rows
}

/// A row for `id`, with its edit applied.
fn row(settings: &GuideSettings, id: RowId, inputs: Vec<GuideInput>, default_text: String, shape: RowShape) -> EditableRow {
    let edit = settings.edits.get(&id).cloned().unwrap_or_default();
    EditableRow {
        key: RowKey::Row(id),
        inputs,
        text: edit.text.clone().unwrap_or_else(|| default_text.clone()),
        default_text,
        shape,
        overridden: edit.text.is_some(),
        hidden: edit.hidden,
    }
}

/// The Mappings overlay's rows for `profile`: the editable rows that aren't hidden. They follow
/// the mappings, so a change to an action shows up here unless its text is overridden.
pub fn mapping_rows(profile: &Profile) -> Vec<MappingRow> {
    editable_rows(profile)
        .into_iter()
        .filter(|r| !r.hidden)
        .map(|r| MappingRow { inputs: r.inputs, text: r.text, shape: r.shape })
        .collect()
}

/// The text an input's row shows by default: its action's description, or what its stick or
/// trigger does.
pub fn default_text(profile: &Profile, input: GuideInput) -> String {
    mapped_inputs(profile).into_iter().find(|(i, _)| *i == input).map(|(_, text)| text).unwrap_or_default()
}

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
mod tests {
    use super::*;
    use crate::config::{Combo, Gestures, MouseButton};

    fn profile() -> Profile {
        let mut p = Profile::passthrough("p");
        p.set_button(Button::South, ButtonAction::Keys(vec!["KEY_E".into()]));
        p.set_button(Button::DpadLeft, ButtonAction::Keys(vec!["KEY_Q".into()]));
        p.set_button(Button::DpadRight, ButtonAction::Keys(vec!["KEY_E".into()]));
        p.set_button(Button::Select, ButtonAction::Disabled);
        p
    }

    fn btn(b: Button) -> GuideInput {
        GuideInput::Button(b)
    }

    /// A combo's inputs in the order its row is keyed by.
    fn combo_inputs(buttons: &[Button]) -> Vec<GuideInput> {
        combo_key(buttons).into_iter().map(GuideInput::Button).collect()
    }

    fn input_key(i: GuideInput) -> RowKey {
        RowKey::Row(RowId::Input(i))
    }

    fn row_of(rows: &[MappingRow], inputs: &[GuideInput]) -> Option<MappingRow> {
        rows.iter().find(|r| r.inputs == inputs).cloned()
    }

    #[test]
    fn rows_follow_the_mappings_and_skip_guide_and_disabled_buttons() {
        let rows = mapping_rows(&profile());
        assert!(rows.iter().all(|r| r.inputs.iter().all(|i| *i != btn(Button::Guide) && *i != btn(Button::Select))));
        assert_eq!(row_of(&rows, &[btn(Button::South)]).unwrap().text, "{keyboard:e}", "default text is the action's description");
    }

    #[test]
    fn default_text_shows_pad_buttons_and_mouse_clicks_as_glyph_tokens() {
        let mut p = profile();
        p.set_button(Button::East, ButtonAction::Gamepad(Button::South));
        p.set_button(Button::West, ButtonAction::Mouse(MouseButton::Left));
        let rows = mapping_rows(&p);
        assert_eq!(row_of(&rows, &[btn(Button::East)]).unwrap().text, "{pad:south}");
        assert_eq!(row_of(&rows, &[btn(Button::West)]).unwrap().text, "{mouse:leftclick}");
    }

    #[test]
    fn triggers_and_sticks_get_rows_when_they_do_something() {
        let mut p = profile();
        p.left_stick.action = StickAction::mouse(1000.0);
        p.left_trigger.action = TriggerAction::Button { action: ButtonAction::Keys(vec!["KEY_Z".into()]), threshold: 0.5 };
        let rows = mapping_rows(&p);
        assert_eq!(row_of(&rows, &[GuideInput::Stick(Stick::Left)]).unwrap().text, "Move the mouse");
        assert_eq!(row_of(&rows, &[GuideInput::Trigger(Trigger::Left)]).unwrap().text, "{keyboard:z}");
        assert!(row_of(&rows, &[GuideInput::Stick(Stick::Right)]).is_none(), "a pass-through stick isn't listed");
    }

    #[test]
    fn a_label_override_replaces_the_action_text() {
        let mut p = profile();
        p.guide.set_text(input_key(btn(Button::South)), "Use".into());
        assert_eq!(row_of(&mapping_rows(&p), &[btn(Button::South)]).unwrap().text, "Use");
    }

    #[test]
    fn a_hidden_row_leaves_the_overlay_but_keeps_the_mapping() {
        let mut p = profile();
        p.guide.set_hidden(input_key(btn(Button::South)), true);
        assert!(row_of(&mapping_rows(&p), &[btn(Button::South)]).is_none());
        assert_eq!(p.button(Button::South), &ButtonAction::Keys(vec!["KEY_E".into()]), "the mapping itself is untouched");
    }

    #[test]
    fn merged_inputs_share_one_row_at_the_first_of_them() {
        let mut p = profile();
        p.guide.merge(vec![btn(Button::DpadRight), btn(Button::DpadLeft)], "Lean".into());
        let rows = mapping_rows(&p);
        let merged: Vec<&MappingRow> = rows.iter().filter(|r| r.text == "Lean").collect();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].inputs, [btn(Button::DpadLeft), btn(Button::DpadRight)], "in mapping order");
        assert!(row_of(&rows, &[btn(Button::DpadLeft)]).is_none());
    }

    #[test]
    fn a_hidden_merged_row_is_left_off() {
        let mut p = profile();
        p.guide.merge(vec![btn(Button::DpadLeft), btn(Button::DpadRight)], "Lean".into());
        p.guide.set_hidden(RowKey::Merged(0), true);
        assert!(mapping_rows(&p).iter().all(|r| r.text != "Lean"));
    }

    #[test]
    fn rows_join_their_glyphs_by_shape() {
        let parts = vec!["A".to_string(), "B".to_string()];
        assert_eq!(RowShape::Merged.join(&parts), "A, B");
        assert_eq!(RowShape::Combo.join(&parts), "A + B");
        assert_eq!(RowShape::Input.join(&parts[..1]), "A");
        assert_eq!(RowShape::Gesture(GestureKind::DoubleTap).join(&parts[..1]), "A ×2");
        assert_eq!(RowShape::Gesture(GestureKind::TripleTap).join(&parts[..1]), "A ×3");
        assert_eq!(RowShape::Gesture(GestureKind::LongPress).join(&parts[..1]), "A hold");
    }

    #[test]
    fn gestures_and_combos_are_listed_after_the_inputs() {
        let mut p = profile();
        p.gestures.insert(Button::South, Gestures { double_tap: Some(ButtonAction::Keys(vec!["KEY_F".into()])), ..Default::default() });
        p.combos.push(Combo { buttons: vec![Button::North, Button::West], action: ButtonAction::Keys(vec!["KEY_R".into()]) });
        p.combos.push(Combo { buttons: vec![Button::North, Button::Guide], action: ButtonAction::Keys(vec!["KEY_X".into()]) });
        let rows = mapping_rows(&p);
        assert_eq!(row_of(&rows, &[btn(Button::South)]).map(|r| r.text), Some("{keyboard:e}".into()));
        assert!(rows.iter().any(|r| r.inputs == [btn(Button::South)] && r.text == "{keyboard:f}" && r.shape == RowShape::Gesture(GestureKind::DoubleTap)));
        assert_eq!(row_of(&rows, &[btn(Button::North), btn(Button::West)]).unwrap().text, "{keyboard:r}");
        assert_eq!(rows.iter().filter(|r| r.inputs.contains(&btn(Button::Guide))).count(), 0, "Guide is never listed");
        assert_eq!(rows.last().unwrap().inputs.len(), 2, "combos come last");
    }

    #[test]
    fn a_gesture_or_combo_row_can_be_renamed_and_hidden() {
        let mut p = profile();
        p.gestures.insert(Button::South, Gestures { double_tap: Some(ButtonAction::Keys(vec!["KEY_F".into()])), ..Default::default() });
        p.combos.push(Combo { buttons: vec![Button::North, Button::West], action: ButtonAction::Keys(vec!["KEY_R".into()]) });
        let gesture = RowKey::Row(RowId::Gesture(btn(Button::South), GestureKind::DoubleTap));
        let combo = RowKey::Row(RowId::Combo(combo_inputs(&[Button::North, Button::West])));
        p.guide.set_text(gesture.clone(), "Reload".into());
        p.guide.set_hidden(combo.clone(), true);
        let rows = mapping_rows(&p);
        assert!(rows.iter().any(|r| r.inputs == [btn(Button::South)] && r.text == "Reload"));
        assert!(rows.iter().all(|r| r.inputs != combo_inputs(&[Button::North, Button::West])), "hidden combo is off");
        let shown = editable_rows(&p).into_iter().find(|r| r.key == combo).unwrap();
        assert!(shown.hidden, "the tab still lists it, hidden");
    }

    #[test]
    fn edits_to_gestures_and_combos_roundtrip_under_readable_keys() {
        let mut p = profile();
        p.gestures.insert(Button::South, Gestures { double_tap: Some(ButtonAction::Keys(vec!["KEY_F".into()])), ..Default::default() });
        p.combos.push(Combo { buttons: vec![Button::North, Button::West], action: ButtonAction::Keys(vec!["KEY_R".into()]) });
        p.guide.set_text(RowKey::Row(RowId::Gesture(btn(Button::South), GestureKind::DoubleTap)), "Reload".into());
        p.guide.set_hidden(RowKey::Row(RowId::Combo(combo_inputs(&[Button::North, Button::West]))), true);
        let text = toml::to_string_pretty(&p.guide).unwrap();
        assert!(text.contains("gesture:button:South:double_tap"), "{text}");
        assert!(text.contains("combo:button:") && text.contains("+button:"), "{text}");
        let back: GuideSettings = toml::from_str(&text).unwrap();
        assert_eq!(back, p.guide);
    }

    #[test]
    fn inputs_roundtrip_through_the_file_under_readable_keys() {
        let mut p = profile();
        p.guide.set_text(RowKey::Row(RowId::Input(GuideInput::Trigger(Trigger::Left))), "Aim".into());
        p.left_trigger.action = TriggerAction::Button { action: ButtonAction::Keys(vec!["KEY_Z".into()]), threshold: 0.5 };
        let text = toml::to_string_pretty(&p.guide).unwrap();
        assert!(text.contains("trigger:Left"), "{text}");
        let back: GuideSettings = toml::from_str(&text).unwrap();
        assert_eq!(back, p.guide);
    }

    #[test]
    fn empty_text_goes_back_to_the_default() {
        let mut p = profile();
        let key = input_key(btn(Button::DpadLeft));
        p.guide.set_text(key.clone(), "Lean".into());
        let row = editable_rows(&p).into_iter().find(|r| r.key == key).unwrap();
        assert_eq!((row.text.as_str(), row.overridden), ("Lean", true));
        p.guide.set_text(key, String::new());
        assert!(p.guide.is_default());
    }

    #[test]
    fn showing_a_row_again_leaves_no_edit_behind() {
        let mut p = profile();
        let key = input_key(btn(Button::DpadLeft));
        p.guide.set_hidden(key.clone(), true);
        assert!(editable_rows(&p).iter().any(|r| r.key == key && r.hidden), "the tab still lists it");
        p.guide.set_hidden(key, false);
        assert!(p.guide.is_default());
    }

    #[test]
    fn merging_again_moves_an_input_to_the_new_row() {
        let mut p = profile();
        let (left, right, south) = (btn(Button::DpadLeft), btn(Button::DpadRight), btn(Button::South));
        p.guide.merge(vec![left, right], "Lean".into());
        p.guide.merge(vec![left, south], "Other".into());
        assert_eq!(p.guide.merged.len(), 1, "the first row is left with one input, so it goes");
        assert_eq!(p.guide.merged[0].inputs, [left, south]);
    }

    #[test]
    fn separate_merges_stay_separate() {
        let mut p = profile();
        let (left, right, south) = (btn(Button::DpadLeft), btn(Button::DpadRight), btn(Button::South));
        p.guide.merge(vec![left, right], "Lean".into());
        p.guide.merge(vec![south, btn(Button::West)], "Use".into());
        assert_eq!(p.guide.merged.len(), 2);
        let rows = mapping_rows(&p);
        assert!(rows.iter().any(|r| r.inputs == [left, right]));
        assert!(rows.iter().any(|r| r.inputs.contains(&south) && r.inputs.len() == 2));
    }

    #[test]
    fn adding_an_input_to_a_merged_row_takes_it_off_its_old_row() {
        let mut p = profile();
        let (left, right, south) = (btn(Button::DpadLeft), btn(Button::DpadRight), btn(Button::South));
        p.guide.merge(vec![left, right], "Lean".into());
        p.guide.merge(vec![south, btn(Button::West)], "Use".into());
        p.guide.add_to_merge(0, south);
        assert_eq!(p.guide.merged.len(), 1, "the other row is left with one input, so it goes");
        assert_eq!(p.guide.merged[0].inputs, [left, right, south]);
    }

    #[test]
    fn custom_rows_list_after_everything_else_and_take_inputs() {
        let mut p = profile();
        p.guide.add_custom("Assign group instead".into());
        p.guide.add_custom_input(0, btn(Button::LeftBumper));
        p.guide.add_custom_input(0, btn(Button::RightBumper));
        p.guide.add_custom_input(0, btn(Button::RightBumper));
        let rows = mapping_rows(&p);
        let custom = rows.last().unwrap();
        assert_eq!(custom.text, "Assign group instead");
        assert_eq!(custom.inputs, [btn(Button::LeftBumper), btn(Button::RightBumper)], "each input once");
        assert_eq!(editable_rows(&p).last().unwrap().shape, RowShape::Combo);
        p.guide.set_text(RowKey::Custom(0), "Groups".into());
        assert_eq!(mapping_rows(&p).last().unwrap().text, "Groups");
        p.guide.set_hidden(RowKey::Custom(0), true);
        assert!(p.guide.custom.len() == 1, "a custom row isn't hidden, only removed");
        p.guide.remove_custom(0);
        assert!(p.guide.custom.is_empty());
    }

    #[test]
    fn custom_rows_roundtrip_through_the_file() {
        let mut p = profile();
        p.guide.add_custom("Assign group instead".into());
        p.guide.add_custom_input(0, btn(Button::LeftBumper));
        let text = toml::to_string_pretty(&p.guide).unwrap();
        let back: GuideSettings = toml::from_str(&text).unwrap();
        assert_eq!(back, p.guide);
    }

    #[test]
    fn split_gives_the_inputs_back_their_own_rows() {
        let mut p = profile();
        p.guide.merge(vec![btn(Button::DpadLeft), btn(Button::DpadRight)], "Lean".into());
        p.guide.split(0);
        assert!(editable_rows(&p).iter().any(|r| r.key == input_key(btn(Button::DpadLeft))));
    }
}

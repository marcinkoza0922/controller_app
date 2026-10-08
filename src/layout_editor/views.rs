//! The read-only parts of Edit Controls' pages: the labels and the rows of the list pages, and
//! what the active profile and game hold for them.

use crate::{
    config::{Button, ButtonAction, Combo, Config, Layer, Profile, Stick, Trigger},
    menu::{ItemView, Tone},
};

use super::{Top, values};
use super::buttons::Slot;

pub(super) fn combo_page(config: &Config, i: usize) -> (Vec<ItemView>, &'static str) {
    let combo = combos(config).get(i);
    let mut rows: Vec<ItemView> = Button::ALL
        .iter()
        .map(|&b| {
            let member = combo.is_some_and(|c| c.buttons.contains(&b));
            button_item(b, if member { "included" } else { "" }.into())
        })
        .collect();
    let action = combo.map_or("Disabled".into(), |c| c.action.summary());
    rows.push(item(format!("Action: {action}")));
    rows.push(remove_item("Delete combo".into()));
    (rows, "A toggle or change · B back")
}
pub(super) fn layer_page(config: &Config, i: usize) -> (Vec<ItemView>, &'static str) {
    let layer = layers(config).get(i);
    let rows = Button::ALL
        .iter()
        .map(|&b| {
            let bound = layer.and_then(|l| l.buttons.get(&b)).map_or("not set".into(), ButtonAction::summary);
            button_item(b, bound)
        })
        .collect();
    (rows, "A change · B back")
}
pub(super) fn layer_name(config: &Config, slot: Slot) -> Option<&str> {
    match slot {
        Slot::Layer(i, _) => layers(config).get(i).map(|l| l.name.as_str()),
        _ => None,
    }
}
pub(super) fn layers(config: &Config) -> &[Layer] {
    config.active_game().layers.as_slice()
}
pub(super) fn combos(config: &Config) -> &[Combo] {
    config.active().map_or(&[], |p: &Profile| p.combos.as_slice())
}
pub(super) fn top_item(row: Top, config: &Config) -> ItemView {
    let profile = config.active();
    let macro_count = config.active_game().macros.len();
    match row {
        Top::Button(b) => {
            let action = profile.map_or(ButtonAction::Disabled, |p| p.button(b).clone());
            button_item(b, action.summary())
        }
        Top::Stick(s) => badged(if s == Stick::Left { "LS" } else { "RS" }, format!("{} stick", values::side(s == Stick::Left))),
        Top::Trigger(t) => badged(if t == Trigger::Left { "LT" } else { "RT" }, format!("{} trigger", values::side(t == Trigger::Left))),
        Top::Gyro => item("Gyro".into()),
        Top::Combos => item(format!("Combos ({})", profile.map_or(0, |p| p.combos.len()))),
        Top::Layers => item("Layers".into()),
        Top::Macros => item(format!("Macros ({macro_count})")),
    }
}

/// A row with a button's chip in front of its label.
pub(super) fn badged(badge: &str, label: String) -> ItemView {
    ItemView { label, button: Some(badge.into()), submenu: false, buttons: Vec::new(), tone: Tone::Normal }
}

pub(super) fn item(label: String) -> ItemView {
    ItemView { label, button: None, submenu: false, buttons: Vec::new(), tone: Tone::Normal }
}

/// A row for a button: its glyph in front of the label.
pub(super) fn button_item(b: Button, label: String) -> ItemView {
    ItemView { buttons: vec![b], ..item(label) }
}

/// A combo's row: its buttons' glyphs, then what it does.
pub(super) fn combo_item(buttons: &[Button], action: &ButtonAction) -> ItemView {
    let label = action.summary();
    if buttons.is_empty() {
        return item(format!("(no buttons yet): {label}"));
    }
    ItemView { buttons: buttons.to_vec(), ..item(label) }
}

/// A row that adds something.
pub(super) fn add_item(label: String) -> ItemView {
    ItemView { tone: Tone::Add, ..item(label) }
}

/// A row that removes or deletes something.
pub(super) fn remove_item(label: String) -> ItemView {
    ItemView { tone: Tone::Remove, ..item(label) }
}

//! The Edit Controls page of the Guide + Start menu: what each button, gesture, combo, stick,
//! trigger and the gyro of the active profile does, changed with the controller. It works on the
//! active profile only, so it is the pack of the game being played (the daemon makes one first
//! when there isn't one).

mod buttons;
mod macros;
mod values;
mod views;
mod zones;

use crate::{
    config::{Button, ButtonAction, Combo, Config, Game, GestureKind, MenuKind, Stick, Trigger},
    menu::{ItemView, MenuView},
    system_menu::active_profile_mut,
};

use buttons::{ActionRow, Picker, Slot, action_row_keyword, action_row_label, action_title, choice_action, page_rows, tuned, wrapped};
use crate::config::Analog;
use zones::{ZONE_ACTION, ZONE_DELETE, ZONE_FROM, ZONE_ROWS, ZONE_TO};
use macros::{StepKind, add_macro, choose_macro_row, macro_page, macro_rows};
use crate::config::Layer;
use views::{add_item, button_item, combo_item, combo_page, combos, item, layer_name, layer_page, layers, top_item};

/// One row of the top page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Top {
    Button(Button),
    Stick(Stick),
    Trigger(Trigger),
    Gyro,
    Combos,
    Layers,
    Macros,
}

fn top_rows() -> Vec<Top> {
    Button::ALL
        .iter()
        .map(|&b| Top::Button(b))
        .chain([
            Top::Stick(Stick::Left),
            Top::Stick(Stick::Right),
            Top::Trigger(Trigger::Left),
            Top::Trigger(Trigger::Right),
            Top::Gyro,
            Top::Combos,
            Top::Layers,
            Top::Macros,
        ])
        .collect()
}

/// Where the editor is. Each page's parent is the one it was opened from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Top,
    Action(Slot),
    Pad(Slot),
    Gestures(Button),
    Combos,
    Combo(usize),
    Layers,
    Layer(usize),
    /// The key picker, for the slot it was opened from.
    Keys(Slot),
    /// Picking the button a macro step presses, releases or taps.
    MacroPick(usize, StepKind),
    /// A stick's or trigger's zones, and one zone.
    Zones(Analog),
    Zone(Analog, usize),
    /// The layer picker, for the slot it was opened from.
    Pick(Slot, Picker),
    /// A stick step of a macro: which stick and where it points.
    MacroStick(usize),
    Macros,
    Macro(usize),
    Stick(Stick),
    Trigger(Trigger),
    Gyro,
}

/// What the editor asks of the menu after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditStep {
    /// Stay on the editor.
    Stay,
    /// Left the top page: back to the menu's main page.
    Leave,
    /// A setting changed in the config.
    Changed,
}

impl EditStep {
    fn changed(changed: bool) -> Self {
        if changed { EditStep::Changed } else { EditStep::Stay }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutEditor {
    page: Page,
    cursor: usize,
    /// The page and cursor each open page was entered from, innermost last.
    history: Vec<(Page, usize)>,
}

impl Default for LayoutEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEditor {
    pub fn new() -> Self {
        LayoutEditor { page: Page::Top, cursor: 0, history: Vec::new() }
    }

    /// Moves the cursor on the current page: (x, y), y positive down. Pages in two columns move
    /// between them with x.
    pub fn move_cursor(&mut self, step: (i32, i32), config: &Config) {
        self.cursor = crate::menu::list_step(self.cursor, self.rows(config), step);
    }

    /// Whether the current page is in two columns, so left and right move between them rather
    /// than changing a value.
    pub fn two_columns(&self, config: &Config) -> bool {
        crate::menu::list_columns(self.rows(config)) == 2
    }

    /// How many rows the current page has.
    pub fn rows(&self, config: &Config) -> usize {
        match self.page {
            Page::Top => top_rows().len(),
            Page::Action(slot) => page_rows(slot, &current_action(config, slot)).len(),
            Page::Pad(_) => Button::ALL.len(),
            Page::Gestures(_) => GestureKind::ALL.len(),
            Page::Combos => combos(config).len() + 1,
            Page::Combo(_) => Button::ALL.len() + 2,
            Page::Layers => layers(config).len() + 1,
            Page::Keys(_) => buttons::KEY_PICKS.len() + 1,
            Page::MacroPick(..) => Button::ALL.len(),
            Page::Pick(_, p) => picker_names(config, p).len(),
            Page::MacroStick(_) => macros::STICK_STEPS,
            Page::Layer(_) => Button::ALL.len(),
            Page::Macros => config.active_game().macros.len() + 1,
            Page::Macro(i) => macro_rows(config, i),
            Page::Stick(_) => values::STICK_ROWS + 1,
            Page::Trigger(_) => values::TRIGGER_ROWS + 1,
            Page::Zones(a) => zones::zones_rows(config, a).len(),
            Page::Zone(..) => ZONE_ROWS,
            Page::Gyro => values::GYRO_ROWS,
        }
    }

    /// Chooses the highlighted row.
    pub fn choose(&mut self, config: &mut Config) -> EditStep {
        match self.page {
            Page::Top => self.open_top_row(),
            Page::Action(slot) => self.choose_action(slot, config),
            Page::Pad(slot) => {
                set_slot(config, slot, Some(ButtonAction::Gamepad(Button::ALL[self.cursor])));
                self.pop();
                self.pop();
                EditStep::Changed
            }
            Page::Keys(slot) => self.choose_key(slot, config),
            Page::MacroPick(i, kind) => self.choose_macro_pick(i, kind, config),
            Page::Gestures(b) => {
                self.push(Page::Action(Slot::Gesture(b, GestureKind::ALL[self.cursor])));
                EditStep::Stay
            }
            Page::Combos => self.choose_combo_row(config),
            Page::Combo(i) => self.choose_combo_member(i, config),
            Page::Layers => self.choose_layers_row(config),
            Page::Pick(slot, p) => {
                let Some(name) = picker_names(config, p).get(self.cursor).cloned() else { return EditStep::Stay };
                set_slot(config, slot, Some(p.action(&name)));
                self.pop();
                self.pop();
                EditStep::Changed
            }
            Page::Layer(i) => {
                self.push(Page::Action(Slot::Layer(i, Button::ALL[self.cursor])));
                EditStep::Stay
            }
            Page::Macros => self.choose_macros_row(config),
            Page::Macro(i) if macros::is_delete_row(config, i, self.cursor) => {
                if macros::delete_macro(config, i) {
                    self.pop();
                    return EditStep::Changed;
                }
                EditStep::Stay
            }
            Page::MacroStick(i) => {
                macros::push_step(config, i, macros::stick_step(self.cursor));
                self.pop();
                EditStep::Changed
            }
            Page::Macro(i) => self.choose_macro_row(i, config),
            Page::Stick(stick) if self.cursor == values::STICK_ROWS => {
                self.push(Page::Zones(Analog::Stick(stick)));
                EditStep::Stay
            }
            Page::Stick(stick) => values::stick_choose(stick, self.cursor, config),
            Page::Trigger(trigger) if self.cursor == values::TRIGGER_ROWS => {
                self.push(Page::Zones(Analog::Trigger(trigger)));
                EditStep::Stay
            }
            Page::Trigger(trigger) => values::trigger_choose(trigger, self.cursor, config),
            Page::Zones(a) => self.choose_zones(a, config),
            Page::Zone(a, i) => self.choose_zone(a, i, config),
            Page::Gyro => values::gyro_choose(self.cursor, config),
        }
    }

    /// Changes the highlighted number by one step (`dir` is -1 or 1). Pages without numbers do
    /// nothing.
    pub fn adjust(&mut self, dir: i32, config: &mut Config) -> EditStep {
        match self.page {
            Page::Stick(stick) => values::stick_adjust(stick, self.cursor, dir, config),
            Page::Trigger(trigger) => values::trigger_adjust(trigger, self.cursor, dir, config),
            Page::Gyro => values::gyro_adjust(self.cursor, dir, config),
            Page::Zone(a, i) => zones::adjust_zone(config, a, i, self.cursor, dir),
            _ => EditStep::Stay,
        }
    }

    /// Goes back one page. From the top page it leaves the editor.
    pub fn back(&mut self) -> EditStep {
        if self.page == Page::Top || !self.pop() {
            self.page = Page::Top;
            self.cursor = 0;
            return EditStep::Leave;
        }
        EditStep::Stay
    }

    fn push(&mut self, page: Page) {
        self.history.push((self.page, self.cursor));
        self.page = page;
        self.cursor = 0;
    }

    /// Returns to the page this one was opened from, with its cursor. False at the top.
    fn pop(&mut self) -> bool {
        match self.history.pop() {
            Some((page, cursor)) => {
                self.page = page;
                self.cursor = cursor;
                true
            }
            None => false,
        }
    }

    fn open_top_row(&mut self) -> EditStep {
        let page = match top_rows()[self.cursor] {
            Top::Button(b) => Page::Action(Slot::Button(b)),
            Top::Stick(s) => Page::Stick(s),
            Top::Trigger(t) => Page::Trigger(t),
            Top::Gyro => Page::Gyro,
            Top::Combos => Page::Combos,
            Top::Layers => Page::Layers,
            Top::Macros => Page::Macros,
        };
        self.push(page);
        EditStep::Stay
    }

    fn choose_action(&mut self, slot: Slot, config: &mut Config) -> EditStep {
        let current = current_action(config, slot);
        let Some(&row) = page_rows(slot, &current).get(self.cursor) else { return EditStep::Stay };
        match row {
            ActionRow::Tuning(tuning) => {
                set_slot(config, slot, Some(tuned(&current, tuning)));
                EditStep::Changed
            }
            ActionRow::Pad => {
                self.push(Page::Pad(slot));
                EditStep::Stay
            }
            ActionRow::Gestures => {
                if let Slot::Button(b) = slot {
                    self.push(Page::Gestures(b));
                }
                EditStep::Stay
            }
            ActionRow::Keys => {
                self.push(Page::Keys(slot));
                EditStep::Stay
            }
            ActionRow::Pick(p) => {
                self.push(Page::Pick(slot, p));
                EditStep::Stay
            }
            // Stays on the page, so the wrapper's own settings can be set straight away.
            ActionRow::Wrap(wrap) => {
                let next = wrapped(&current_action(config, slot), wrap);
                set_slot(config, slot, Some(next));
                EditStep::Changed
            }
            _ => {
                let action = choice_action(row).flatten();
                set_slot(config, slot, action);
                self.pop();
                EditStep::Changed
            }
        }
    }

    fn choose_combo_row(&mut self, config: &mut Config) -> EditStep {
        if self.cursor < combos(config).len() {
            self.push(Page::Combo(self.cursor));
            return EditStep::Stay;
        }
        let Some(profile) = active_profile_mut(config) else { return EditStep::Stay };
        profile.combos.push(Combo { buttons: Vec::new(), action: ButtonAction::Disabled });
        let new = profile.combos.len() - 1;
        self.push(Page::Combo(new));
        EditStep::Changed
    }

    fn choose_layers_row(&mut self, config: &mut Config) -> EditStep {
        if self.cursor < layers(config).len() {
            self.push(Page::Layer(self.cursor));
            return EditStep::Stay;
        }
        let Some(profile_game) = active_game_mut(config) else { return EditStep::Stay };
        let taken = |n: &str| profile_game.layers.iter().any(|l| l.name == n);
        let name = (1..).map(|n| format!("Layer {n}")).find(|n| !taken(n)).unwrap_or_default();
        profile_game.layers.push(Layer::new(&name));
        let new = profile_game.layers.len() - 1;
        self.push(Page::Layer(new));
        EditStep::Changed
    }

    fn choose_key(&mut self, slot: Slot, config: &mut Config) -> EditStep {
        match buttons::KEY_PICKS.get(self.cursor) {
            Some(&code) => {
                toggle_key(config, slot, code);
                EditStep::Changed
            }
            // Done: back to the action page it was opened from, and then out of it.
            None => {
                self.pop();
                self.pop();
                EditStep::Stay
            }
        }
    }

    fn choose_macro_pick(&mut self, i: usize, kind: StepKind, config: &mut Config) -> EditStep {
        let Some(&b) = Button::ALL.get(self.cursor) else { return EditStep::Stay };
        macros::push_step(config, i, kind.step(b));
        self.pop();
        EditStep::Changed
    }

    /// A macro's row: an add row picks what to add, the others change the steps.
    fn choose_macro_row(&mut self, i: usize, config: &mut Config) -> EditStep {
        match macros::add_offset(config, i, self.cursor) {
            Some(offset) if offset < 3 => {
                self.push(Page::MacroPick(i, StepKind::from_offset(offset)));
                EditStep::Stay
            }
            Some(3) => {
                self.push(Page::MacroStick(i));
                EditStep::Stay
            }
            _ => choose_macro_row(config, i, self.cursor),
        }
    }

    fn choose_zones(&mut self, a: Analog, config: &mut Config) -> EditStep {
        if self.cursor < zones::zones(config, a).len() {
            self.push(Page::Zone(a, self.cursor));
            return EditStep::Stay;
        }
        let Some(new) = zones::add_zone(config, a) else { return EditStep::Stay };
        self.push(Page::Zone(a, new));
        EditStep::Changed
    }

    fn choose_zone(&mut self, a: Analog, i: usize, config: &mut Config) -> EditStep {
        match self.cursor {
            ZONE_FROM | ZONE_TO => zones::adjust_zone(config, a, i, self.cursor, 1),
            ZONE_ACTION => {
                self.push(Page::Action(Slot::Zone(a, i)));
                EditStep::Stay
            }
            ZONE_DELETE if zones::delete_zone(config, a, i) => {
                self.pop();
                EditStep::Changed
            }
            _ => EditStep::Stay,
        }
    }

    fn choose_macros_row(&mut self, config: &mut Config) -> EditStep {
        if self.cursor < config.active_game().macros.len() {
            self.push(Page::Macro(self.cursor));
            return EditStep::Stay;
        }
        let Some(new) = add_macro(config) else { return EditStep::Stay };
        self.push(Page::Macro(new));
        EditStep::Changed
    }

    fn choose_combo_member(&mut self, i: usize, config: &mut Config) -> EditStep {
        let members = Button::ALL.len();
        match self.cursor {
            row if row < members => {
                let b = Button::ALL[row];
                let Some(profile) = active_profile_mut(config) else { return EditStep::Stay };
                let Some(combo) = profile.combos.get_mut(i) else { return EditStep::Stay };
                if let Some(at) = combo.buttons.iter().position(|&x| x == b) {
                    combo.buttons.remove(at);
                } else {
                    combo.buttons.push(b);
                    combo.buttons.sort_by_key(|x| Button::ALL.iter().position(|y| y == x));
                }
                EditStep::Changed
            }
            row if row == members => {
                self.push(Page::Action(Slot::Combo(i)));
                EditStep::Stay
            }
            _ => {
                let Some(profile) = active_profile_mut(config) else { return EditStep::Stay };
                if i >= profile.combos.len() {
                    return EditStep::Stay;
                }
                profile.combos.remove(i);
                self.pop();
                self.cursor = self.cursor.min(combos(config).len().saturating_sub(1));
                EditStep::Changed
            }
        }
    }

    /// The page as the overlay draws it. `parents` are the menus the editor was opened from,
    /// outermost first; the editor's own pages before this one follow them as breadcrumbs.
    pub fn view(&self, config: &Config, parents: &[String]) -> MenuView {
        let style = config.active_menu_style().clone();
        let profile = config.active();
        let (items, hint) = match self.page {
            Page::Top => (top_rows().into_iter().map(|row| top_item(row, config)).collect(), "A change · B back"),
            Page::Action(slot) => {
                let current = current_action(config, slot);
                (page_rows(slot, &current).into_iter().map(|row| ItemView { keyword: action_row_keyword(row), ..item(action_row_label(row, &current)) }).collect(), "A choose · B back")
            }
            Page::Pad(_) => (Button::ALL.iter().map(|&p| button_item(p, String::new())).collect(), "A choose · B back"),
            Page::Gestures(b) => (
                GestureKind::ALL.iter().map(|&k| button_item(b, buttons::gesture_row_label(profile, b, k))).collect(),
                "A choose · B back",
            ),
            Page::Combos => {
                let mut rows: Vec<ItemView> = combos(config).iter().map(|c| combo_item(&c.buttons, &c.action)).collect();
                rows.push(add_item("Add combo".into()));
                (rows, "A open · B back")
            }
            Page::Combo(i) => combo_page(config, i),
            Page::Layers => {
                let mut rows: Vec<ItemView> = layers(config).iter().map(|l| item(format!("{} ({} bound)", l.name, l.buttons.len()))).collect();
                rows.push(add_item("Add layer".into()));
                (rows, "A open · B back")
            }
            Page::Pick(_, p) => {
                let rows = picker_names(config, p).into_iter().map(item).collect();
                (rows, "A choose · B back")
            }
            Page::Layer(i) => layer_page(config, i),
            Page::Keys(slot) => keys_page(config, slot),
            Page::MacroPick(..) => (Button::ALL.iter().map(|&b| button_item(b, String::new())).collect(), "A choose · B back"),
            Page::Macros => {
                let mut rows: Vec<ItemView> = config.active_game().macros.iter().map(|m| item(format!("{} ({} steps)", m.name, m.steps.len()))).collect();
                rows.push(add_item("Add macro".into()));
                (rows, "A open · B back")
            }
            Page::Macro(i) => macro_page(config, i),
            Page::MacroStick(_) => ((0..macros::STICK_STEPS).map(|r| item(macros::stick_row_label(r))).collect(), "A choose · B back"),
            Page::Stick(stick) => with_zones_row(values::stick_page(stick, profile), config, Analog::Stick(stick)),
            Page::Trigger(trigger) => with_zones_row(values::trigger_page(trigger, profile), config, Analog::Trigger(trigger)),
            Page::Zones(a) => (zones::zones_rows(config, a), "A open or add · B back"),
            Page::Zone(a, i) => zones::zone_page(config, a, i),
            Page::Gyro => values::gyro_page(profile),
        };
        let crumbs = parents.iter().cloned().chain(self.history.iter().map(|&(page, _)| page_title(page, config))).collect();
        MenuView {
            title: page_title(self.page, config),
            kind: MenuKind::List,
            items,
            selected: Some(self.cursor),
            crumbs,
            hint: hint.into(),
            style,
        }
    }
}

/// The key picker's rows: each key, ticked when the slot sends it, and Done.
fn keys_page(config: &Config, slot: Slot) -> (Vec<ItemView>, &'static str) {
    let pressed = match current_action(config, slot) {
        ButtonAction::Keys(keys) => keys,
        _ => Vec::new(),
    };
    let mut rows: Vec<ItemView> = buttons::KEY_PICKS
        .iter()
        .map(|code| item(format!("[{}] {}", if pressed.iter().any(|k| k == code) { "x" } else { " " }, buttons::key_label(code))))
        .collect();
    rows.push(item("Done".into()));
    (rows, "A toggle a key · B back")
}

/// A stick's or trigger's page with its zones row added after its own rows.
fn with_zones_row((mut rows, hint): (Vec<ItemView>, &'static str), config: &Config, a: Analog) -> (Vec<ItemView>, &'static str) {
    rows.push(item(format!("Zones ({})", zones::zones(config, a).len())));
    (rows, hint)
}

/// The action a slot holds now.
fn current_action(config: &Config, slot: Slot) -> ButtonAction {
    let Some(profile) = config.active() else { return ButtonAction::Disabled };
    match slot {
        Slot::Button(b) => profile.button(b).clone(),
        Slot::Gesture(b, kind) => profile.gestures.get(&b).and_then(|g| g.get(kind)).cloned().unwrap_or(ButtonAction::Disabled),
        Slot::Combo(i) => profile.combos.get(i).map_or(ButtonAction::Disabled, |c| c.action.clone()),
        Slot::Layer(i, b) => layers(config).get(i).and_then(|l| l.buttons.get(&b)).cloned().unwrap_or(ButtonAction::Disabled),
        Slot::Zone(a, i) => zones::zone_action(config, a, i),
    }
}

/// The title of a page, which the overlay also shows as a breadcrumb.
fn page_title(page: Page, config: &Config) -> String {
    match page {
        Page::Top => "Edit Controls".into(),
        Page::Action(slot) => action_title(slot, layer_name(config, slot)),
        Page::Pad(slot) => format!("{} sends", action_title(slot, layer_name(config, slot))),
        Page::Keys(slot) => format!("{} sends a key", action_title(slot, layer_name(config, slot))),
        Page::MacroPick(i, _) => config.active_game().macros.get(i).map_or_else(|| "Macro".into(), |m| format!("{} · pick a button", m.name)),
        Page::Pick(slot, p) => format!("{} · pick a {}", action_title(slot, layer_name(config, slot)), p.noun()),
        Page::MacroStick(i) => format!("{} · stick step", config.active_game().macros.get(i).map_or("Macro", |m| m.name.as_str())),
        Page::Gestures(b) => format!("{} gestures", b.short_name()),
        Page::Combos => "Combos".into(),
        Page::Combo(i) => format!("Combo {}", i + 1),
        Page::Layers => "Layers".into(),
        Page::Layer(i) => layers(config).get(i).map_or_else(|| "Layer".into(), |l| l.name.clone()),
        Page::Macros => "Macros".into(),
        Page::Macro(i) => config.active_game().macros.get(i).map_or_else(|| "Macro".into(), |m| m.name.clone()),
        Page::Stick(stick) => format!("{} stick", values::side(stick == Stick::Left)),
        Page::Trigger(trigger) => format!("{} trigger", values::side(trigger == Trigger::Left)),
        Page::Gyro => "Gyro".into(),
        Page::Zones(a) => format!("{} zones", analog_name(a)),
        Page::Zone(_, i) => format!("Zone {}", i + 1),
    }
}

/// The names a picker lists from the active game.
fn picker_names(config: &Config, p: Picker) -> Vec<String> {
    let game = config.active_game();
    match p {
        Picker::Layer => game.layers.iter().map(|l| l.name.clone()).collect(),
        Picker::Macro => game.macros.iter().map(|m| m.name.clone()).collect(),
        Picker::Menu => game.menus.iter().map(|m| m.name.clone()).collect(),
        Picker::Info => game.info.iter().map(|i| i.name.clone()).collect(),
        Picker::Log => game.logs.iter().map(|l| l.name.clone()).collect(),
    }
}

fn analog_name(a: Analog) -> String {
    match a {
        Analog::Stick(s) => format!("{} stick", values::side(s == crate::config::Stick::Left)),
        Analog::Trigger(t) => format!("{} trigger", values::side(t == crate::config::Trigger::Left)),
    }
}

/// Adds `code` to the keys a slot sends, or takes it out again. A slot that sends something
/// else starts from no keys.
fn toggle_key(config: &mut Config, slot: Slot, code: &str) {
    let mut keys = match current_action(config, slot) {
        ButtonAction::Keys(keys) => keys,
        _ => Vec::new(),
    };
    match keys.iter().position(|k| k == code) {
        Some(at) => {
            keys.remove(at);
        }
        None => keys.push(code.into()),
    }
    set_slot(config, slot, Some(ButtonAction::Keys(keys)));
}

fn set_slot(config: &mut Config, slot: Slot, action: Option<ButtonAction>) {
    if let Slot::Zone(a, i) = slot {
        zones::set_zone_action(config, a, i, action.unwrap_or(ButtonAction::Disabled));
        return;
    }
    if let Slot::Layer(i, b) = slot {
        if let Some(layer) = active_game_mut(config).and_then(|g| g.layers.get_mut(i)) {
            buttons::set_layer_slot(&mut layer.buttons, b, action);
        }
        return;
    }
    if let Some(profile) = active_profile_mut(config) {
        buttons::set_slot(profile, slot, action);
    }
}

/// The game the active profile is in, or General.
fn active_game_mut(config: &mut Config) -> Option<&mut Game> {
    let at = config.active_ref();
    config.game_mut(at.game.as_deref())
}

#[cfg(test)]
mod tests;

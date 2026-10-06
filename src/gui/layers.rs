//! The Layers tab: a game's layers, edited with the profile editor over one of its profiles.
//!
//! The editor works on `App::layer_view`, the compared profile with the layer on top (and
//! only the layer's own combos). After each message, whatever changed there is written back
//! into the layer as an override (see [`App::write_back`]).

use iced::widget::{column, row};

use super::*;
use crate::config::{Indicator, Layer};

/// An entry in a layer's "Indicator" picker.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct IndicatorChoice(pub Indicator);

impl fmt::Display for IndicatorChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Indicator::Name => f.write_str("Its name"),
            Indicator::Info(name) => write!(f, "Info overlay “{name}”"),
            Indicator::Off => f.write_str("Nothing"),
        }
    }
}

impl App {
    /// The Layers tab shows a layer, so the profile editor edits it.
    pub(super) fn editing_layer(&self) -> bool {
        matches!(self.page, Page::Game(_)) && self.game_tab == GameTab::Layers && self.game().layers.get(self.layer).is_some()
    }

    /// The profile a layer is compared with (and shown over).
    fn compared(&self) -> Option<&Profile> {
        let profiles = &self.game().profiles;
        profiles.get(self.compare).or(profiles.first())
    }

    /// Rebuilds the profile the layer editor shows.
    pub(super) fn refresh_layer_view(&mut self) {
        self.layer_view = if self.editing_layer() {
            let layer = &self.game().layers[self.layer];
            self.compared().map(|base| {
                let mut view = base.with_layers([layer]);
                view.combos = layer.combos.clone();
                view
            })
        } else {
            None
        };
    }

    /// Records what an edit changed in the layer editor as the layer's overrides.
    pub(super) fn write_back(&mut self, before: &Profile) {
        let Some(after) = self.layer_view.clone() else { return };
        let i = self.layer;
        let Some(layer) = self.game_mut().layers.get_mut(i) else { return };
        for b in Button::EVERY {
            if before.buttons.get(&b) != after.buttons.get(&b) {
                layer.buttons.insert(b, after.button(b).clone());
            }
            if before.gestures.get(&b) != after.gestures.get(&b) {
                // Empty gestures in a layer mean "none while it's on".
                layer.gestures.insert(b, after.gestures.get(&b).cloned().unwrap_or_default());
            }
        }
        if before.left_stick != after.left_stick {
            layer.left_stick = Some(after.left_stick.clone());
        }
        if before.right_stick != after.right_stick {
            layer.right_stick = Some(after.right_stick.clone());
        }
        if before.left_trigger != after.left_trigger {
            layer.left_trigger = Some(after.left_trigger.clone());
        }
        if before.right_trigger != after.right_trigger {
            layer.right_trigger = Some(after.right_trigger.clone());
        }
        if before.gyro != after.gyro {
            layer.gyro = Some(after.gyro.clone());
        }
        if before.combos != after.combos {
            layer.combos = after.combos.clone();
        }
    }

    /// Makes a new layer in the shown game, named "Layer", "Layer 2", …, and returns its name.
    pub(super) fn add_layer(&mut self) -> String {
        let name = unique_name("Layer", |n| self.game().layers.iter().any(|l| l.name == n));
        self.game_mut().layers.push(Layer::new(&name));
        name
    }

    /// "+ New layer" picked in a "Layer…" action: makes the layer and names it there.
    pub(super) fn new_layer_for(&mut self, message: Message) -> Message {
        let Message::SetAction(target, mut action) = message else { return message };
        let mut wants = false;
        action.walk(&mut |a| wants |= matches!(a, ButtonAction::Layer(n) if n == NEW_LAYER));
        if wants {
            let name = self.add_layer();
            action.walk_mut(&mut |a| {
                if let ButtonAction::Layer(n) = a
                    && n == NEW_LAYER
                {
                    *n = name.clone();
                }
            });
        }
        Message::SetAction(target, action)
    }

    pub(super) fn update_layers(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NewLayer => {
                let name = self.add_layer();
                self.layer = self.game().layers.len() - 1;
                self.game_tab = GameTab::Layers;
                self.message = Some((format!("Added layer “{name}”. Hold it with \"Layer…\" on any input."), false));
            }
            Message::SelectLayer(name) => {
                if let Some(i) = self.game().layers.iter().position(|l| l.name == name) {
                    self.layer = i;
                    self.expanded.clear();
                }
            }
            Message::RenameLayer(name) => {
                let i = self.layer;
                let taken = self.game().layers.iter().enumerate().any(|(j, l)| j != i && l.name == name);
                if let Some(old) = self.game().layers.get(i).map(|l| l.name.clone())
                    && !taken
                {
                    let game = self.game_mut();
                    game.rename_item(ItemKind::Layer, &old, &name);
                    game.rename_refs(ItemKind::Layer, &old, &name);
                }
            }
            Message::DeleteLayer => {
                let i = self.layer;
                if i < self.game().layers.len() {
                    self.game_mut().layers.remove(i);
                    self.layer = i.saturating_sub(1);
                }
            }
            Message::SetCompare(name) => {
                if let Some(i) = self.game().profiles.iter().position(|p| p.name == name) {
                    self.compare = i;
                }
            }
            Message::SetIndicator(choice) => {
                let i = self.layer;
                if let Some(l) = self.game_mut().layers.get_mut(i) {
                    l.indicator = choice.0;
                }
            }
            Message::SetIndicatorStyle(style) => {
                let i = self.layer;
                if let Some(l) = self.game_mut().layers.get_mut(i) {
                    l.indicator_style = style;
                }
            }
            Message::ToggleIndicatorAppearance => self.indicator_appearance = !self.indicator_appearance,
            Message::OverrideInput(part) => {
                let Some(base) = self.compared().cloned() else { return Task::none() };
                let i = self.layer;
                let Some(layer) = self.game_mut().layers.get_mut(i) else { return Task::none() };
                match part {
                    LayerPart::Button(b) => {
                        layer.buttons.insert(b, base.button(b).clone());
                        if let Some(g) = base.gestures(b) {
                            layer.gestures.insert(b, g.clone());
                        }
                        self.expanded.insert(Target::Button(b));
                    }
                    LayerPart::Stick(Stick::Left) => layer.left_stick = Some(base.left_stick),
                    LayerPart::Stick(Stick::Right) => layer.right_stick = Some(base.right_stick),
                    LayerPart::Trigger(Trigger::Left) => layer.left_trigger = Some(base.left_trigger),
                    LayerPart::Trigger(Trigger::Right) => layer.right_trigger = Some(base.right_trigger),
                    LayerPart::Gyro => layer.gyro = Some(base.gyro),
                }
            }
            Message::RevertInput(part) => {
                let i = self.layer;
                let Some(layer) = self.game_mut().layers.get_mut(i) else { return Task::none() };
                match part {
                    LayerPart::Button(b) => {
                        layer.buttons.remove(&b);
                        layer.gestures.remove(&b);
                    }
                    LayerPart::Stick(Stick::Left) => layer.left_stick = None,
                    LayerPart::Stick(Stick::Right) => layer.right_stick = None,
                    LayerPart::Trigger(Trigger::Left) => layer.left_trigger = None,
                    LayerPart::Trigger(Trigger::Right) => layer.right_trigger = None,
                    LayerPart::Gyro => layer.gyro = None,
                }
            }
            Message::ToggleBaseCombo(key) => {
                let i = self.layer;
                if let Some(layer) = self.game_mut().layers.get_mut(i) {
                    let before = layer.disabled_combos.len();
                    layer.disabled_combos.retain(|d| crate::config::combo_key(d) != key);
                    if layer.disabled_combos.len() == before {
                        layer.disabled_combos.push(key);
                    }
                }
            }
            other => return self.update_packs(other),
        }
        Task::none()
    }

    pub(super) fn view_layers<'a>(&'a self, names: &Names) -> Element<'a, Message> {
        let game = self.game();
        let picker_row = {
            let names: Vec<String> = game.layers.iter().map(|l| l.name.clone()).collect();
            let current = game.layers.get(self.layer).map(|l| l.name.clone());
            row![
                dropdown(names, current, Message::SelectLayer).placeholder("No layers yet").width(220),
                button(text("+ New layer")).style(button::secondary).on_press(Message::NewLayer),
                space::horizontal(),
                button(text("Copy from another game…").size(13)).style(button::text).on_press(Message::OpenBrowse(ItemKind::Layer)),
                help(
                    "A layer changes some of the controller's mappings while it's on, then changes them \
                     back: e.g. hold LB, and the face buttons type F1–F4. Turn one on with \"Layer…\" on \
                     any button, trigger, stick direction, zone, gesture or combo: on while held, or wrapped \
                     in Toggle, until pressed again (that also works from a menu item). It applies over \
                     whichever of the game's profiles is active; what it doesn't set stays as in the profile. \
                     Several layers can be on at once; the newest wins."
                        .into(),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        };
        let mut col = column![picker_row].spacing(16);
        let Some(layer) = game.layers.get(self.layer) else {
            return col
                .push(text("This game has no layers.").color(MUTED_COLOR))
                .into();
        };

        // Name, indicator and the profile it's shown over.
        let mut indicators = vec![IndicatorChoice(Indicator::Name), IndicatorChoice(Indicator::Off)];
        indicators.extend(names.infos.iter().map(|n| IndicatorChoice(Indicator::Info(n.clone()))));
        let profiles: Vec<String> = game.profiles.iter().map(|p| p.name.clone()).collect();
        let compared = self.compared().map(|p| p.name.clone());
        let mut settings: Vec<Element<'a, Message>> = vec![
            labeled(
                "Name",
                row![
                    field("Layer name", &layer.name).on_input(Message::RenameLayer).width(220),
                    space::horizontal(),
                    button(text("Delete layer").size(13)).style(button::danger).on_press(Message::DeleteLayer),
                ]
                .align_y(Alignment::Center)
                .into(),
            ),
            labeled(
                "On screen while on",
                row![
                    dropdown(indicators, Some(IndicatorChoice(layer.indicator.clone())), Message::SetIndicator).width(260),
                    help(
                        "Shows that the layer is on: its name, one of the game's info overlays (e.g. a cheat \
                         sheet of what the layer's buttons do), or nothing, for quick ones such as \"hold Y to \
                         lean\"."
                            .into(),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ),
        ];
        if let Indicator::Info(name) = &layer.indicator
            && !names.infos.contains(name)
        {
            settings.push(labeled("", text(format!("⚠ missing info overlay {name:?}")).size(12).color(ERROR_COLOR).into()));
        }
        if layer.indicator == Indicator::Name {
            settings.push(labeled("", disclosure("Appearance", self.indicator_appearance, Message::ToggleIndicatorAppearance)));
            if self.indicator_appearance {
                settings.push(style_editor(&layer.indicator_style, Rc::new(Message::SetIndicatorStyle)));
            }
            let sample = InfoOverlay {
                name: layer.name.clone(),
                always: true,
                style: preview_style(&layer.indicator_style),
                rows: vec![vec![layer.name.clone()]],
            };
            let view = crate::info::resolve(&sample, &crate::info::Live::sample(self.config.info_glyphs));
            settings.push(preview(crate::overlay::draw::info_panel(&view)));
        }
        settings.push(labeled(
            "Shown over",
            row![
                dropdown(profiles, compared, Message::SetCompare).width(220),
                help("Inputs the layer doesn't set show this profile's mapping. The layer applies over any of the game's profiles.".into()),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ));
        col = col.push(column(settings).spacing(10));

        if let (Some(p), Some(base)) = (self.profile(), self.compared()) {
            col = col.push(self.view_profile_editor(p, names, Some(LayerMarks { layer, base })));
        }
        col.into()
    }
}

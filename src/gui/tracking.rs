//! What an input display follows, shared by log overlays and info overlays' `{current_input}`
//! cells: which controller, which inputs, how many per sequence, and the pause that ends one.

use iced::widget::{checkbox, column, row, slider};

use super::*;
use crate::config::{INPUT_GROUPS, InputTracking};

pub(super) type OnTracking<'a> = Rc<dyn Fn(InputTracking) -> Message + 'a>;

/// Which controller a display follows: every one, or one by its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DeviceChoice(pub(super) Option<u8>);

impl fmt::Display for DeviceChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => f.write_str("Every controller"),
            Some(n) => write!(f, "Controller {n}"),
        }
    }
}

const DEVICE_CHOICES: [DeviceChoice; 5] =
    [DeviceChoice(None), DeviceChoice(Some(0)), DeviceChoice(Some(1)), DeviceChoice(Some(2)), DeviceChoice(Some(3))];

impl App {
    /// Rows for the tracking settings of overlay `at` (a kind and index; its group lists
    /// open by `ToggleInputGroup`). `what` names a sequence there: "line" or "sequence".
    pub(super) fn tracking_editor<'a>(
        &self,
        at: (ItemKind, usize),
        t: &InputTracking,
        what: &'static str,
        on_change: &OnTracking<'a>,
    ) -> Vec<Element<'a, Message>> {
        let on = on_change.clone();
        let base = t.clone();
        let device = row![
            dropdown(DEVICE_CHOICES, Some(DeviceChoice(t.device)), move |d: DeviceChoice| on(InputTracking { device: d.0, ..base.clone() }))
                .width(200),
            help(
                "Controllers are numbered from 0 in the order they connected; each keeps its number until it \
                 disconnects. Every controller merges them all, by time."
                    .into(),
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let (on, base) = (on_change.clone(), t.clone());
        let max = row![
            slider(1.0..=30.0, f32::from(t.max_inputs), move |v| on(InputTracking { max_inputs: v as u8, ..base.clone() }))
                .step(1.0_f32)
                .width(300),
            text(format!("{} inputs", t.max_inputs)).size(13),
            help(format!("A longer {what} keeps its newest inputs; the oldest are pushed out.")),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let (on, base) = (on_change.clone(), t.clone());
        let gap = row![
            slider(100.0..=1000.0, t.gap_ms as f32, move |v| on(InputTracking { gap_ms: v as u64, ..base.clone() }))
                .step(50.0_f32)
                .width(300),
            text(format!("{} ms", t.gap_ms)).size(13),
            help(format!("A pause longer than this between presses starts a new {what}.")),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        vec![
            labeled("Controller", device.into()),
            labeled("Inputs", self.input_groups(at, t, on_change)),
            labeled(if what == "line" { "Per line, at most" } else { "At most" }, max.into()),
            labeled(if what == "line" { "New line after" } else { "New sequence after" }, gap.into()),
        ]
    }

    /// A checkbox per group of inputs; groups of more than one open to list their own.
    fn input_groups<'a>(&self, (kind, i): (ItemKind, usize), t: &InputTracking, on_change: &OnTracking<'a>) -> Element<'a, Message> {
        let mut col = column![].spacing(4);
        for (g, (name, members)) in INPUT_GROUPS.iter().enumerate() {
            let tracked = members.iter().filter(|m| t.tracks(**m)).count();
            let label = match tracked {
                n if n == members.len() || n == 0 => (*name).to_string(),
                n => format!("{name} ({n} of {})", members.len()),
            };
            let (on, base) = (on_change.clone(), t.clone());
            let group = checkbox(tracked == members.len()).label(label).on_toggle(move |v| {
                let mut t = base.clone();
                members.iter().for_each(|m| t.set_tracked(*m, v));
                on(t)
            });
            let mut line = row![group].spacing(8).align_y(Alignment::Center);
            let open = self.open_input_groups.contains(&(kind, i, g));
            if members.len() > 1 {
                let chevron = if open { "▾" } else { "▸" };
                line = line.push(
                    button(text(chevron).size(13)).style(button::text).padding([0, 4]).on_press(Message::ToggleInputGroup(kind, i, g)),
                );
            }
            col = col.push(line);
            if open && members.len() > 1 {
                let mut each = row![].spacing(14).padding(iced::Padding::ZERO.left(28));
                for m in members.iter() {
                    let (on, base) = (on_change.clone(), t.clone());
                    each = each.push(checkbox(t.tracks(*m)).label(m.name()).on_toggle(move |v| {
                        let mut t = base.clone();
                        t.set_tracked(*m, v);
                        on(t)
                    }));
                }
                col = col.push(each);
            }
        }
        col.into()
    }
}

//! Trigger and stick zones in Edit Controls: an action held while a stick or trigger is pushed
//! within a range. A stick's or trigger's page lists its zones, and each zone has a page for its
//! range and its action.

use crate::{
    config::{Analog, ButtonAction, Config, Zone},
    menu::ItemView,
    system_menu::active_profile_mut,
};

use super::views::{add_item, item, remove_item};
use super::EditStep;

/// Rows of a zone's page.
pub const ZONE_FROM: usize = 0;
pub const ZONE_TO: usize = 1;
pub const ZONE_ACTION: usize = 2;
pub const ZONE_DELETE: usize = 3;
pub const ZONE_ROWS: usize = 4;

/// How far a zone's edge moves with one step, and the range the edges stay in.
const EDGE_STEP: f32 = 0.05;
const EDGE_RANGE: (f32, f32) = (0.0, 1.0);

/// Where a new zone starts: the top half of the travel.
const NEW_ZONE: (f32, f32) = (0.5, 1.0);

pub fn zones(config: &Config, a: Analog) -> &[Zone] {
    let Some(profile) = config.active() else { return &[] };
    match a {
        Analog::Stick(crate::config::Stick::Left) => &profile.left_stick.zones,
        Analog::Stick(crate::config::Stick::Right) => &profile.right_stick.zones,
        Analog::Trigger(crate::config::Trigger::Left) => &profile.left_trigger.zones,
        Analog::Trigger(crate::config::Trigger::Right) => &profile.right_trigger.zones,
    }
}

fn zones_mut(config: &mut Config, a: Analog) -> Option<&mut Vec<Zone>> {
    let profile = active_profile_mut(config)?;
    Some(match a {
        Analog::Stick(crate::config::Stick::Left) => &mut profile.left_stick.zones,
        Analog::Stick(crate::config::Stick::Right) => &mut profile.right_stick.zones,
        Analog::Trigger(crate::config::Trigger::Left) => &mut profile.left_trigger.zones,
        Analog::Trigger(crate::config::Trigger::Right) => &mut profile.right_trigger.zones,
    })
}

/// The action of zone `i`, or Disabled when there is no such zone.
pub fn zone_action(config: &Config, a: Analog, i: usize) -> ButtonAction {
    zones(config, a).get(i).map_or(ButtonAction::Disabled, |z| z.action.clone())
}

/// Sets the action of zone `i`.
pub fn set_zone_action(config: &mut Config, a: Analog, i: usize, action: ButtonAction) {
    if let Some(zone) = zones_mut(config, a).and_then(|zs| zs.get_mut(i)) {
        zone.action = action;
    }
}

/// The rows of a zones page: each zone, then adding one.
pub fn zones_rows(config: &Config, a: Analog) -> Vec<ItemView> {
    zones(config, a)
        .iter()
        .map(|z| item(format!("{:.2}–{:.2}: {}", z.min, z.max, z.action.summary())))
        .chain(std::iter::once(add_item("Add zone".into())))
        .collect()
}

/// Adds a zone to the top half of the travel. Returns its index.
pub fn add_zone(config: &mut Config, a: Analog) -> Option<usize> {
    let zs = zones_mut(config, a)?;
    zs.push(Zone { min: NEW_ZONE.0, max: NEW_ZONE.1, action: ButtonAction::Disabled });
    Some(zs.len() - 1)
}

/// Deletes zone `i`. True when it existed.
pub fn delete_zone(config: &mut Config, a: Analog, i: usize) -> bool {
    match zones_mut(config, a) {
        Some(zs) if i < zs.len() => {
            zs.remove(i);
            true
        }
        _ => false,
    }
}

/// Moves a zone's start (`ZONE_FROM`) or end (`ZONE_TO`) by one step (`dir` is -1 or 1). The
/// start never passes the end, and neither leaves the travel.
pub fn adjust_zone(config: &mut Config, a: Analog, i: usize, row: usize, dir: i32) -> EditStep {
    let Some(zone) = zones_mut(config, a).and_then(|zs| zs.get_mut(i)) else { return EditStep::Stay };
    let step = dir as f32 * EDGE_STEP;
    let before = (zone.min, zone.max);
    match row {
        ZONE_FROM => zone.min = (zone.min + step).clamp(EDGE_RANGE.0, zone.max),
        ZONE_TO => zone.max = (zone.max + step).clamp(zone.min, EDGE_RANGE.1),
        _ => {}
    }
    EditStep::changed(before != (zone.min, zone.max))
}

/// The rows and hint of zone `i`'s page.
pub fn zone_page(config: &Config, a: Analog, i: usize) -> (Vec<ItemView>, &'static str) {
    let zone = zones(config, a).get(i);
    let rows = vec![
        item(format!("From: {:.2}", zone.map_or(0.0, |z| z.min))),
        item(format!("To: {:.2}", zone.map_or(0.0, |z| z.max))),
        item(format!("Action: {}", zone.map_or("Disabled".into(), |z| z.action.summary()))),
        remove_item("Delete zone".into()),
    ];
    (rows, "A choose · ◀ ▶ move an edge · B back")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Stick, Trigger};

    #[test]
    fn a_zone_is_added_moved_and_deleted() {
        let mut config = Config::default();
        let a = Analog::Trigger(Trigger::Left);
        let i = add_zone(&mut config, a).unwrap();
        assert_eq!(adjust_zone(&mut config, a, i, ZONE_FROM, -1), EditStep::Changed);
        assert!(zones(&config, a)[i].min < NEW_ZONE.0);
        // The start cannot pass the end.
        for _ in 0..40 {
            adjust_zone(&mut config, a, i, ZONE_FROM, 1);
        }
        assert!(zones(&config, a)[i].min <= zones(&config, a)[i].max);
        assert!(delete_zone(&mut config, a, i));
        assert!(zones(&config, a).is_empty());
    }

    #[test]
    fn a_stick_zone_takes_an_action() {
        let mut config = Config::default();
        let a = Analog::Stick(Stick::Right);
        let i = add_zone(&mut config, a).unwrap();
        set_zone_action(&mut config, a, i, ButtonAction::Screenshot);
        assert_eq!(zone_action(&config, a, i), ButtonAction::Screenshot);
    }
}

//! The Guide overlay's center menu: Quick Settings, Edit Controls, the keyboard and numpad, mouse
//! mode, media controls, the next profile and, while Steam runs, the Steam overlay. The daemon keeps the
//! cursor and runs the chosen item; the engine delivers the presses.

use crate::{
    config::OverlayStyle,
    menu::{ItemView, MenuView, Tone},
};

/// One row of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    QuickSettings,
    /// Opens the Edit Controls editor directly, not through Quick Settings.
    EditControls,
    Keyboard,
    Numpad,
    MouseMode,
    Media,
    NextProfile,
    /// Sends the real Guide button to the virtual pad, which opens Steam's overlay.
    Steam,
}

/// The menu's rows. The Steam overlay is only there while Steam runs.
pub fn items(steam: bool) -> Vec<Item> {
    let mut items = vec![
        Item::QuickSettings,
        Item::EditControls,
        Item::Keyboard,
        Item::Numpad,
        Item::MouseMode,
        Item::Media,
        Item::NextProfile,
    ];
    if steam {
        items.push(Item::Steam);
    }
    items
}

fn label(item: Item, profile: &str) -> String {
    match item {
        Item::QuickSettings => "Quick Settings".into(),
        Item::EditControls => "Edit Controls".into(),
        Item::Keyboard => "On-screen keyboard".into(),
        Item::Numpad => "On-screen numpad".into(),
        Item::MouseMode => "Mouse mode".into(),
        Item::Media => "Media controls".into(),
        Item::NextProfile => format!("Next profile (now {profile})"),
        Item::Steam => "Steam overlay".into(),
    }
}

/// The menu as the overlay draws it, with `cursor` on one of its rows.
pub fn view(steam: bool, cursor: usize, profile: &str, style: OverlayStyle) -> MenuView {
    let items = items(steam)
        .into_iter()
        .map(|item| ItemView {
            label: label(item, profile),
            button: None,
            submenu: item == Item::QuickSettings,
            buttons: Vec::new(),
            tone: Tone::Normal,
            keyword: None,
            weight: 1.0,
        })
        .collect();
    MenuView {
        title: "Guide".into(),
        kind: crate::config::MenuKind::List,
        items,
        selected: Some(cursor),
        picks: 0,
        crumbs: Vec::new(),
        hint: "A choose · B close".into(),
        style,
    }
}

/// Whether Steam is running: some process is named `steam`.
pub fn steam_running() -> bool {
    let Ok(dir) = std::fs::read_dir("/proc") else { return false };
    dir.flatten().any(|entry| {
        std::fs::read_to_string(entry.path().join("comm")).is_ok_and(|name| name.trim() == "steam")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steam_overlay_is_listed_only_while_steam_runs() {
        assert!(!items(false).contains(&Item::Steam));
        assert_eq!(items(true).last(), Some(&Item::Steam));
    }

    #[test]
    fn quick_settings_leads_the_menu_and_edit_controls_follows_it() {
        assert_eq!(&items(false)[..2], &[Item::QuickSettings, Item::EditControls]);
    }

    #[test]
    fn the_view_names_the_profile_and_marks_the_cursor() {
        let menu = view(false, 1, "Gameplay", OverlayStyle::default());
        assert_eq!(menu.selected, Some(1));
        assert_eq!(menu.items.last().unwrap().label, "Next profile (now Gameplay)");
        assert_eq!(menu.items.len(), items(false).len());
    }
}

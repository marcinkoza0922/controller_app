//! The font overlays are drawn in: a few free fonts bundled with the app, or any installed
//! family named by its name.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use iced::Font;

/// A font shipped inside the binary (all under the SIL Open Font License; see `assets/fonts`).
pub struct Bundled {
    pub name: &'static str,
    pub bytes: &'static [u8],
}

pub const BUNDLED: [Bundled; 5] = [
    Bundled { name: "Inter", bytes: include_bytes!("../assets/fonts/Inter-Regular.ttf") },
    Bundled { name: "Cantarell", bytes: include_bytes!("../assets/fonts/Cantarell-VF.otf") },
    Bundled { name: "Comfortaa", bytes: include_bytes!("../assets/fonts/Comfortaa-Regular.otf") },
    Bundled { name: "Source Code Pro", bytes: include_bytes!("../assets/fonts/SourceCodePro-Regular.otf") },
    Bundled { name: "Rajdhani", bytes: include_bytes!("../assets/fonts/Rajdhani-Medium.ttf") },
];

/// Names offered in the font dropdowns.
pub fn names() -> Vec<String> {
    BUNDLED.iter().map(|b| b.name.to_string()).collect()
}

/// The iced font for a setting; `None` is the system default.
pub fn resolve(name: Option<&str>) -> Font {
    let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) else { return Font::DEFAULT };
    if let Some(b) = BUNDLED.iter().find(|b| b.name == name) {
        return Font::with_name(b.name);
    }
    // iced wants a `'static` name; a pack can name any family, so keep each one only once.
    static OTHER: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut known = OTHER.get_or_init(Mutex::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let leaked = *known.entry(name.to_string()).or_insert_with(|| Box::leak(name.to_string().into_boxed_str()));
    Font::with_name(leaked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_is_the_default_and_names_resolve_to_themselves() {
        assert_eq!(resolve(None), Font::DEFAULT);
        assert_eq!(resolve(Some("  ")), Font::DEFAULT);
        assert_eq!(resolve(Some("Inter")), Font::with_name("Inter"));
        assert_eq!(resolve(Some("Fira Sans")), resolve(Some("Fira Sans")));
    }
}

//! On-screen keyboard used to pick evdev keys by clicking instead of typing their names.

use iced::{
    Element,
    widget::{button, column, row, space, text},
};

/// Width of a 1-unit key and the gap between keys, in pixels.
const UNIT: f32 = 30.0;
const GAP: f32 = 3.0;

/// One key cap. An empty `code` is a blank spacer used for alignment.
pub struct Key {
    pub label: &'static str,
    pub code: &'static str,
    pub width: f32,
}

const fn k(label: &'static str, code: &'static str) -> Key {
    Key { label, code, width: 1.0 }
}

const fn w(label: &'static str, code: &'static str, width: f32) -> Key {
    Key { label, code, width }
}

const fn gap(width: f32) -> Key {
    Key { label: "", code: "", width }
}

/// Every block has six rows so the blocks line up side by side.
const MAIN: [&[Key]; 6] = [
    &[
        k("Esc", "KEY_ESC"), gap(1.0),
        k("F1", "KEY_F1"), k("F2", "KEY_F2"), k("F3", "KEY_F3"), k("F4", "KEY_F4"), gap(0.5),
        k("F5", "KEY_F5"), k("F6", "KEY_F6"), k("F7", "KEY_F7"), k("F8", "KEY_F8"), gap(0.5),
        k("F9", "KEY_F9"), k("F10", "KEY_F10"), k("F11", "KEY_F11"), k("F12", "KEY_F12"),
    ],
    &[
        k("`", "KEY_GRAVE"), k("1", "KEY_1"), k("2", "KEY_2"), k("3", "KEY_3"), k("4", "KEY_4"),
        k("5", "KEY_5"), k("6", "KEY_6"), k("7", "KEY_7"), k("8", "KEY_8"), k("9", "KEY_9"),
        k("0", "KEY_0"), k("-", "KEY_MINUS"), k("=", "KEY_EQUAL"), w("Backspace", "KEY_BACKSPACE", 2.0),
    ],
    &[
        w("Tab", "KEY_TAB", 1.5), k("Q", "KEY_Q"), k("W", "KEY_W"), k("E", "KEY_E"), k("R", "KEY_R"),
        k("T", "KEY_T"), k("Y", "KEY_Y"), k("U", "KEY_U"), k("I", "KEY_I"), k("O", "KEY_O"),
        k("P", "KEY_P"), k("[", "KEY_LEFTBRACE"), k("]", "KEY_RIGHTBRACE"), w("\\", "KEY_BACKSLASH", 1.5),
    ],
    &[
        w("Caps", "KEY_CAPSLOCK", 1.75), k("A", "KEY_A"), k("S", "KEY_S"), k("D", "KEY_D"),
        k("F", "KEY_F"), k("G", "KEY_G"), k("H", "KEY_H"), k("J", "KEY_J"), k("K", "KEY_K"),
        k("L", "KEY_L"), k(";", "KEY_SEMICOLON"), k("'", "KEY_APOSTROPHE"), w("Enter", "KEY_ENTER", 2.25),
    ],
    &[
        w("Shift", "KEY_LEFTSHIFT", 2.25), k("Z", "KEY_Z"), k("X", "KEY_X"), k("C", "KEY_C"),
        k("V", "KEY_V"), k("B", "KEY_B"), k("N", "KEY_N"), k("M", "KEY_M"), k(",", "KEY_COMMA"),
        k(".", "KEY_DOT"), k("/", "KEY_SLASH"), w("Shift", "KEY_RIGHTSHIFT", 2.75),
    ],
    &[
        w("Ctrl", "KEY_LEFTCTRL", 1.25), w("Super", "KEY_LEFTMETA", 1.25), w("Alt", "KEY_LEFTALT", 1.25),
        w("Space", "KEY_SPACE", 6.25), w("AltGr", "KEY_RIGHTALT", 1.25), w("Super", "KEY_RIGHTMETA", 1.25),
        w("Menu", "KEY_COMPOSE", 1.25), w("Ctrl", "KEY_RIGHTCTRL", 1.25),
    ],
];

const NAV: [&[Key]; 6] = [
    &[w("PrtSc", "KEY_SYSRQ", 1.2), w("ScrLk", "KEY_SCROLLLOCK", 1.2), w("Pause", "KEY_PAUSE", 1.2)],
    &[w("Ins", "KEY_INSERT", 1.2), w("Home", "KEY_HOME", 1.2), w("PgUp", "KEY_PAGEUP", 1.2)],
    &[w("Del", "KEY_DELETE", 1.2), w("End", "KEY_END", 1.2), w("PgDn", "KEY_PAGEDOWN", 1.2)],
    &[gap(3.6)],
    &[gap(1.2), w("↑", "KEY_UP", 1.2), gap(1.2)],
    &[w("←", "KEY_LEFT", 1.2), w("↓", "KEY_DOWN", 1.2), w("→", "KEY_RIGHT", 1.2)],
];

const NUMPAD: [&[Key]; 6] = [
    &[gap(4.0)],
    &[k("Num", "KEY_NUMLOCK"), k("/", "KEY_KPSLASH"), k("*", "KEY_KPASTERISK"), k("-", "KEY_KPMINUS")],
    &[k("7", "KEY_KP7"), k("8", "KEY_KP8"), k("9", "KEY_KP9"), k("+", "KEY_KPPLUS")],
    &[k("4", "KEY_KP4"), k("5", "KEY_KP5"), k("6", "KEY_KP6"), gap(1.0)],
    &[k("1", "KEY_KP1"), k("2", "KEY_KP2"), k("3", "KEY_KP3"), k("Ent", "KEY_KPENTER")],
    &[w("0", "KEY_KP0", 2.0), k(".", "KEY_KPDOT"), gap(1.0)],
];

const MEDIA: &[Key] = &[
    w("Mute", "KEY_MUTE", 2.0),
    w("Vol −", "KEY_VOLUMEDOWN", 2.0),
    w("Vol +", "KEY_VOLUMEUP", 2.0),
    w("Prev", "KEY_PREVIOUSSONG", 2.0),
    w("Play/Pause", "KEY_PLAYPAUSE", 2.5),
    w("Next", "KEY_NEXTSONG", 2.0),
    w("Stop", "KEY_STOPCD", 2.0),
    w("Bright −", "KEY_BRIGHTNESSDOWN", 2.25),
    w("Bright +", "KEY_BRIGHTNESSUP", 2.25),
];

/// Pixel width of a key spanning `units`, including the gaps it covers, so rows of
/// different key counts still line up.
fn key_width(units: f32) -> f32 {
    units * (UNIT + GAP) - GAP
}

/// Renders the keyboard. Keys in `selected` (evdev names such as `KEY_A`) are highlighted.
pub fn view<'a, Message: Clone + 'a>(
    selected: &[String],
    on_key: impl Fn(&'static str) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    let key_row = |keys: &[Key]| {
        row(keys.iter().map(|key| {
            let width = key_width(key.width);
            if key.code.is_empty() {
                return space().width(width).height(UNIT).into();
            }
            let is_selected = selected.iter().any(|s| s == key.code);
            button(text(key.label).size(11).center().width(iced::Length::Fill))
                .width(width)
                .height(UNIT)
                .padding(2)
                .style(if is_selected { button::primary } else { button::secondary })
                .on_press(on_key(key.code))
                .into()
        }))
        .spacing(GAP)
    };
    let block = |rows: &[&[Key]]| column(rows.iter().map(|r| key_row(r).into())).spacing(GAP);

    column![
        row![block(&MAIN), block(&NAV), block(&NUMPAD)].spacing(14),
        key_row(MEDIA),
    ]
    .spacing(12)
    .into()
}

/// Display name for an evdev key: its keyboard label if it has one, else the bare name.
pub fn label(code: &str) -> String {
    let main_label = MAIN
        .iter()
        .chain(NAV.iter())
        .flat_map(|r| r.iter())
        .chain(MEDIA)
        .find(|key| key.code == code)
        .map(|key| key.label);
    // Left/right modifiers share a label, so say which side.
    match code {
        "KEY_LEFTSHIFT" | "KEY_LEFTCTRL" | "KEY_LEFTMETA" | "KEY_LEFTALT" => {
            format!("Left {}", main_label.unwrap_or_default())
        }
        "KEY_RIGHTSHIFT" | "KEY_RIGHTCTRL" | "KEY_RIGHTMETA" => {
            format!("Right {}", main_label.unwrap_or_default())
        }
        _ if code.starts_with("KEY_KP") => format!("Keypad {}", code.trim_start_matches("KEY_KP")),
        _ => main_label
            .map(str::to_string)
            .unwrap_or_else(|| code.trim_start_matches("KEY_").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use evdev::KeyCode;

    use super::*;

    fn all_keys() -> impl Iterator<Item = &'static Key> {
        MAIN.iter().chain(NAV.iter()).chain(NUMPAD.iter()).flat_map(|r| r.iter()).chain(MEDIA)
    }

    #[test]
    fn every_key_is_a_valid_code_the_virtual_keyboard_supports() {
        for key in all_keys().filter(|k| !k.code.is_empty()) {
            let code = KeyCode::from_str(key.code).unwrap_or_else(|_| panic!("bad code {}", key.code));
            // VirtualKbm enables codes 1..=248.
            assert!((1..=248).contains(&code.code()), "{} not on the virtual keyboard", key.code);
        }
    }

    #[test]
    fn rows_in_a_block_have_equal_width() {
        for block in [&MAIN, &NAV, &NUMPAD] {
            let widths: Vec<f32> = block.iter().map(|r| r.iter().map(|k| k.width).sum()).collect();
            assert!(widths.iter().all(|w| (w - widths[0]).abs() < 1e-3), "{widths:?}");
        }
    }

    #[test]
    fn labels_disambiguate_sides() {
        assert_eq!(label("KEY_LEFTSHIFT"), "Left Shift");
        assert_eq!(label("KEY_RIGHTCTRL"), "Right Ctrl");
        assert_eq!(label("KEY_KP7"), "Keypad 7");
        assert_eq!(label("KEY_A"), "A");
        assert_eq!(label("KEY_F13"), "F13");
    }
}

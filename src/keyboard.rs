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
pub const MAIN: [&[Key]; 6] = [
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

/// The on-screen numpad. Digits are the top-row keys, which type numbers whatever the
/// Num Lock state (keypad keys turn into arrows and Home/End with it off).
pub const PAD: [&[Key]; 4] = [
    &[k("7", "KEY_7"), k("8", "KEY_8"), k("9", "KEY_9")],
    &[k("4", "KEY_4"), k("5", "KEY_5"), k("6", "KEY_6")],
    &[k("1", "KEY_1"), k("2", "KEY_2"), k("3", "KEY_3")],
    &[w("0", "KEY_0", 2.0), k(".", "KEY_DOT")],
];

/// Which on-screen key layout the controller drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    #[default]
    Keyboard,
    Numpad,
}

impl Layout {
    pub fn rows(self) -> &'static [&'static [Key]] {
        match self {
            Layout::Keyboard => &MAIN,
            Layout::Numpad => &PAD,
        }
    }

    /// Where the cursor starts the first time.
    pub fn home(self) -> Cursor {
        let code = match self {
            Layout::Keyboard => "KEY_Q",
            Layout::Numpad => "KEY_5",
        };
        find(self, code).unwrap_or_default()
    }
}

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
        _ if code.starts_with("KEY_KP") => {
            let name = code.trim_start_matches("KEY_KP").to_lowercase();
            let mut chars = name.chars();
            let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
            format!("Keypad {first}{}", chars.as_str())
        }
        _ => main_label
            .map(str::to_string)
            .unwrap_or_else(|| code.trim_start_matches("KEY_").to_string()),
    }
}

/// A key position in a layout for controller navigation (gaps skipped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
}

/// A layout without spacers: each key with its horizontal center in key units.
pub fn grid(layout: Layout) -> Vec<Vec<(&'static Key, f32)>> {
    layout
        .rows()
        .iter()
        .map(|row| {
            let mut x = 0.0;
            let mut keys = Vec::new();
            for key in row.iter() {
                if !key.code.is_empty() {
                    keys.push((key, x + key.width / 2.0));
                }
                x += key.width;
            }
            keys
        })
        .collect()
}

/// The key under the cursor (clamped into the grid).
pub fn key_at(layout: Layout, cursor: Cursor) -> &'static Key {
    let grid = grid(layout);
    let row = &grid[cursor.row.min(grid.len() - 1)];
    row[cursor.col.min(row.len() - 1)].0
}

/// Moves the cursor one step: left/right along the row (wrapping), up/down to the key in
/// the next row whose center lines up best (also wrapping).
pub fn step(layout: Layout, cursor: Cursor, dx: i32, dy: i32) -> Cursor {
    let grid = grid(layout);
    let row = cursor.row.min(grid.len() - 1);
    let col = cursor.col.min(grid[row].len() - 1);
    if dy == 0 {
        let len = grid[row].len() as i32;
        return Cursor { row, col: (col as i32 + dx).rem_euclid(len) as usize };
    }
    let x = grid[row][col].1;
    let new_row = (row as i32 + dy).rem_euclid(grid.len() as i32) as usize;
    let new_col = grid[new_row]
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1.1 - x).abs().total_cmp(&(b.1.1 - x).abs()))
        .map(|(i, _)| i)
        .unwrap_or(0);
    Cursor { row: new_row, col: new_col }
}

/// Where a key code sits in the grid.
pub fn find(layout: Layout, code: &str) -> Option<Cursor> {
    grid(layout).iter().enumerate().find_map(|(row, keys)| {
        keys.iter().position(|(k, _)| k.code == code).map(|col| Cursor { row, col })
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use evdev::KeyCode;

    use super::*;

    fn all_keys() -> impl Iterator<Item = &'static Key> {
        MAIN.iter().chain(NAV.iter()).chain(NUMPAD.iter()).chain(PAD.iter()).flat_map(|r| r.iter()).chain(MEDIA)
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
        for block in [&MAIN[..], &NAV[..], &NUMPAD[..], &PAD[..]] {
            let widths: Vec<f32> = block.iter().map(|r| r.iter().map(|k| k.width).sum()).collect();
            assert!(widths.iter().all(|w| (w - widths[0]).abs() < 1e-3), "{widths:?}");
        }
    }

    #[test]
    fn controller_navigation_follows_the_layout() {
        let at = |c: Cursor| key_at(Layout::Keyboard, c).code;
        let step = |c, dx, dy| step(Layout::Keyboard, c, dx, dy);
        let find = |code| find(Layout::Keyboard, code);
        let q = find("KEY_Q").unwrap();
        assert_eq!(at(step(q, 1, 0)), "KEY_W");
        assert_eq!(at(step(q, -1, 0)), "KEY_TAB");
        // Up from Q lands on the key above it (1 or 2 on a real keyboard), down on A.
        assert!(matches!(at(step(q, 0, -1)), "KEY_1" | "KEY_2"), "{}", at(step(q, 0, -1)));
        assert_eq!(at(step(q, 0, 1)), "KEY_A");
        // Down from H reaches the space bar region of the bottom row eventually.
        let b = find("KEY_B").unwrap();
        assert_eq!(at(step(b, 0, 1)), "KEY_SPACE");
        // Rows wrap around.
        let esc = find("KEY_ESC").unwrap();
        assert_eq!(at(step(esc, -1, 0)), "KEY_F12");
        assert_eq!(step(step(esc, 0, -1), 0, 1).row, esc.row);
    }

    #[test]
    fn numpad_navigation() {
        let at = |c: Cursor| key_at(Layout::Numpad, c).code;
        let five = Layout::Numpad.home();
        assert_eq!(at(five), "KEY_5");
        assert_eq!(at(step(Layout::Numpad, five, 0, -1)), "KEY_8");
        assert_eq!(at(step(Layout::Numpad, five, 1, 0)), "KEY_6");
        // Down from 2 lands on the wide 0; 0 to the right is the dot.
        let two = step(Layout::Numpad, five, 0, 1);
        let zero = step(Layout::Numpad, two, 0, 1);
        assert_eq!(at(zero), "KEY_0");
        assert_eq!(at(step(Layout::Numpad, zero, 1, 0)), "KEY_DOT");
        assert_eq!(step(Layout::Numpad, zero, 0, 1).row, 0, "columns wrap");
    }

    #[test]
    fn labels_disambiguate_sides() {
        assert_eq!(label("KEY_LEFTSHIFT"), "Left Shift");
        assert_eq!(label("KEY_RIGHTCTRL"), "Right Ctrl");
        assert_eq!(label("KEY_KP7"), "Keypad 7");
        assert_eq!(label("KEY_KPPLUS"), "Keypad Plus");
        assert_eq!(label("KEY_A"), "A");
        assert_eq!(label("KEY_F13"), "F13");
    }
}

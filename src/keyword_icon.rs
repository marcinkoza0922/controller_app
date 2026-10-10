//! The icon drawn in front of a keyword (Macro, Turbo, Menu…), on the profile page and in the
//! in-game editor. Both draw SVG, so the icon is the same in each.

use crate::config::Keyword;

/// The color a keyword is drawn in: one per kind of item, readable on light and dark.
pub fn color(k: Keyword) -> [u8; 3] {
    match k {
        Keyword::Toggle => [0x2a, 0x9d, 0x8f],
        Keyword::Turbo => [0xe7, 0x6f, 0x20],
        Keyword::Macro => [0x8e, 0x5b, 0xd6],
        Keyword::Menu => [0xd6, 0x45, 0x7a],
        Keyword::Layer => [0xb0, 0x85, 0x10],
        Keyword::Info => [0x3f, 0x7f, 0xd8],
        Keyword::Log => [0x4f, 0x95, 0xa0],
    }
}

/// The keyword's icon as SVG, in its color: a hamburger for a menu, a circled "i" for info, and
/// a picture of the idea for the rest.
pub fn svg(k: Keyword) -> String {
    svg_in(k, color(k))
}

/// The same icon in another color, for the places that draw it neutral (the editor's tabs).
pub fn svg_in(k: Keyword, [r, g, b]: [u8; 3]) -> String {
    let c = format!("#{r:02x}{g:02x}{b:02x}");
    let body = match k {
        // Three bars.
        Keyword::Menu => format!("<path d='M4 6H20 M4 12H20 M4 18H20' stroke='{c}' stroke-width='2.4' stroke-linecap='round'/>"),
        Keyword::Info => format!(
            "<circle cx='12' cy='12' r='9.5' fill='none' stroke='{c}' stroke-width='2'/>\
             <circle cx='12' cy='7.5' r='1.5' fill='{c}'/>\
             <path d='M12 11V17' stroke='{c}' stroke-width='2.2' stroke-linecap='round'/>"
        ),
        // A lightning bolt.
        Keyword::Turbo => format!("<path d='M13 2 4.5 13.5H11L10 22 19.5 10.5H13Z' fill='{c}'/>"),
        // Numbered steps: a dot and a line for each.
        Keyword::Macro => format!(
            "<circle cx='4.5' cy='6' r='1.8' fill='{c}'/><circle cx='4.5' cy='12' r='1.8' fill='{c}'/><circle cx='4.5' cy='18' r='1.8' fill='{c}'/>\
             <path d='M9 6H20 M9 12H20 M9 18H20' stroke='{c}' stroke-width='2' stroke-linecap='round'/>"
        ),
        // Stacked sheets.
        Keyword::Layer => format!(
            "<path d='M12 3 21 8 12 13 3 8Z M3 12.5 12 17.5 21 12.5 M3 16.5 12 21.5 21 16.5' fill='none' stroke='{c}' stroke-width='2' stroke-linejoin='round'/>"
        ),
        // A switch, knob on.
        Keyword::Toggle => format!(
            "<rect x='2' y='7' width='20' height='10' rx='5' fill='none' stroke='{c}' stroke-width='2'/>\
             <circle cx='16' cy='12' r='3.5' fill='{c}'/>"
        ),
        // A page of text.
        Keyword::Log => format!(
            "<path d='M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z M14 3V8H19 M9 12H15 M9 16H15' \
             fill='none' stroke='{c}' stroke-width='2' stroke-linejoin='round' stroke-linecap='round'/>"
        ),
    };
    format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'>{body}</svg>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_keyword_has_an_svg_in_its_color() {
        for k in [Keyword::Toggle, Keyword::Turbo, Keyword::Macro, Keyword::Menu, Keyword::Layer, Keyword::Info, Keyword::Log] {
            let svg = svg(k);
            assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
            let [r, g, b] = color(k);
            assert!(svg.contains(&format!("#{r:02x}{g:02x}{b:02x}")));
        }
    }
}

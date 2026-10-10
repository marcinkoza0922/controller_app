//! The glyph before each editor tab's name. They're drawn like the keyword icons, in a neutral
//! gray, except the ones that show the controller in use: those take its family's colors, the
//! way the overlays draw them.

use super::*;
use crate::{
    config::{Button, Keyword},
    info::{Icon, PadFamily, Segment, button_glyph},
    keyword_icon,
    overlay::draw::icon_svg,
};

/// The gray the neutral glyphs are drawn in: readable on both themes.
const INK: [u8; 3] = [0x8a, 0x8f, 0x98];

/// An icon's SVG, from its drawing on a 24-unit grid.
fn svg_of(body: &str) -> String {
    format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'>{body}</svg>")
}

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The controller in use: its silhouette, in its family's color.
fn controller(family: PadFamily) -> String {
    let ink = Color::from_rgb8(INK[0], INK[1], INK[2]);
    icon_svg(&Icon::Controller(family), ink)
}

/// The four face buttons in a diamond, each in its color for the family (the Nintendo layout's
/// face buttons have none, so they're rings).
fn face_buttons(family: PadFamily) -> String {
    let at = [(Button::North, (12.0, 6.5)), (Button::West, (6.5, 12.0)), (Button::East, (17.5, 12.0)), (Button::South, (12.0, 17.5))];
    let discs: String = at
        .into_iter()
        .map(|(b, (x, y))| match button_glyph(b, family, false) {
            Segment::Glyph { fill: Some(rgb), .. } => format!("<circle cx='{x}' cy='{y}' r='3.2' fill='{}'/>", hex(rgb)),
            _ => format!("<circle cx='{x}' cy='{y}' r='3' fill='none' stroke='{}' stroke-width='1.8'/>", hex(INK)),
        })
        .collect();
    svg_of(&discs)
}

/// A switch's stick: a ring with its cap pushed up and left.
fn stick() -> String {
    let c = hex(INK);
    svg_of(&format!("<circle cx='12' cy='12' r='9' fill='none' stroke='{c}' stroke-width='2'/><circle cx='11' cy='11' r='4.5' fill='{c}'/>"))
}

/// Two buttons pressed together.
fn chord() -> String {
    let c = hex(INK);
    svg_of(&format!("<circle cx='8.5' cy='12' r='5.5' fill='none' stroke='{c}' stroke-width='2'/><circle cx='15.5' cy='12' r='5.5' fill='none' stroke='{c}' stroke-width='2'/>"))
}

/// A gyroscope: two rings crossing.
fn gyro() -> String {
    let c = hex(INK);
    svg_of(&format!(
        "<ellipse cx='12' cy='12' rx='9' ry='4' fill='none' stroke='{c}' stroke-width='2'/>\
         <ellipse cx='12' cy='12' rx='4' ry='9' fill='none' stroke='{c}' stroke-width='2'/>"
    ))
}

/// The Guide button: a ring with a center dot.
fn guide() -> String {
    let c = hex(INK);
    svg_of(&format!("<circle cx='12' cy='12' r='9' fill='none' stroke='{c}' stroke-width='2'/><circle cx='12' cy='12' r='3' fill='{c}'/>"))
}

/// Two sliders: settings.
fn details() -> String {
    let c = hex(INK);
    svg_of(&format!(
        "<path d='M4 7H20 M4 17H20' stroke='{c}' stroke-width='2' stroke-linecap='round'/>\
         <circle cx='9' cy='7' r='2.6' fill='{c}'/><circle cx='15' cy='17' r='2.6' fill='{c}'/>"
    ))
}

fn tab_svg(svg_text: String) -> Element<'static, Message> {
    svg(svg::Handle::from_memory(svg_text.into_bytes())).width(16).height(16).into()
}

impl GameTab {
    /// The glyph before the tab's name. Profiles shows the controller in use.
    pub(super) fn glyph(self, family: PadFamily) -> Element<'static, Message> {
        let text = match self {
            GameTab::Profiles => controller(family),
            GameTab::Layers => keyword_icon::svg_in(Keyword::Layer, INK),
            GameTab::Macros => keyword_icon::svg_in(Keyword::Macro, INK),
            GameTab::Menus => keyword_icon::svg_in(Keyword::Menu, INK),
            GameTab::Info => keyword_icon::svg_in(Keyword::Info, INK),
            GameTab::Logs => keyword_icon::svg_in(Keyword::Log, INK),
            GameTab::Details => details(),
        };
        tab_svg(text)
    }
}

impl ProfileTab {
    /// The glyph before the sub-tab's name. Buttons shows the face buttons in the family's colors.
    pub(super) fn glyph(self, family: PadFamily) -> Element<'static, Message> {
        let text = match self {
            ProfileTab::Buttons => face_buttons(family),
            ProfileTab::Sticks => stick(),
            ProfileTab::Combos => chord(),
            ProfileTab::Gyro => gyro(),
            ProfileTab::Guide => guide(),
        };
        tab_svg(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_tab_takes_the_family_colors() {
        let xbox = face_buttons(PadFamily::Xbox);
        assert!(xbox.contains("#3ca03c"), "A is green on Xbox");
        assert!(!xbox.contains("fill='none'"));
        assert!(face_buttons(PadFamily::Nintendo).contains("fill='none'"), "Nintendo's face buttons have no color");
    }

    #[test]
    fn controller_tab_is_tinted_by_family() {
        assert!(controller(PadFamily::PlayStation).contains("#5a8cdc"));
        assert_ne!(controller(PadFamily::Xbox), controller(PadFamily::PlayStation));
    }
}

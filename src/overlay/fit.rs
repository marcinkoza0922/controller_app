//! How big an overlay may be on this display: the percentages its style sets, in pixels.

use iced::Size;

use crate::config::OverlayStyle;

/// An overlay's size and offset in pixels of the display it is drawn on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// The box's width and height, when the style sets them; unset, the box fits its contents.
    pub width: Option<f32>,
    pub height: Option<f32>,
    /// The most the box may stretch before its contents wrap or scroll.
    pub max_width: f32,
    pub max_height: f32,
    /// How far the box sits from where its position puts it.
    pub x: f32,
    pub y: f32,
}

impl Fit {
    /// For a preview, which isn't the display: the box fits its contents, with no max and no offset.
    pub fn contents() -> Self {
        Fit { width: None, height: None, max_width: f32::INFINITY, max_height: f32::INFINITY, x: 0.0, y: 0.0 }
    }

    pub fn of(style: &OverlayStyle, display: Size) -> Self {
        let across = |pct: f32, of: f32| (pct / 100.0 * of).max(0.0);
        let max_width = across(style.max_width, display.width);
        let max_height = across(style.max_height, display.height);
        Fit {
            width: style.width.map(|w| across(w, display.width).min(max_width)),
            height: style.height.map(|h| across(h, display.height).min(max_height)),
            max_width,
            max_height,
            x: style.x_offset / 100.0 * display.width,
            y: style.y_offset / 100.0 * display.height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: Size = Size::new(2000.0, 1000.0);

    #[test]
    fn percentages_become_pixels_of_the_display() {
        let style = OverlayStyle {
            width: Some(50.0),
            height: Some(25.0),
            x_offset: -10.0,
            y_offset: 5.0,
            max_width: 80.0,
            max_height: 60.0,
            ..OverlayStyle::default()
        };
        let fit = Fit::of(&style, DISPLAY);
        assert_eq!(fit.width, Some(1000.0));
        assert_eq!(fit.height, Some(250.0));
        assert_eq!((fit.max_width, fit.max_height), (1600.0, 600.0));
        assert_eq!((fit.x, fit.y), (-200.0, 50.0));
    }

    #[test]
    fn a_set_size_is_held_inside_the_max() {
        let style = OverlayStyle { width: Some(95.0), height: Some(95.0), max_width: 50.0, max_height: 50.0, ..OverlayStyle::default() };
        let fit = Fit::of(&style, DISPLAY);
        assert_eq!(fit.width, Some(1000.0));
        assert_eq!(fit.height, Some(500.0));
    }

    #[test]
    fn unset_size_fits_the_contents() {
        let fit = Fit::of(&OverlayStyle::default(), DISPLAY);
        assert_eq!(fit.width, None);
        assert_eq!(fit.height, None);
        assert_eq!((fit.x, fit.y), (0.0, 0.0));
    }
}

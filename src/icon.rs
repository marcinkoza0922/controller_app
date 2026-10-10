//! The app icon, rasterized from the SVG that also installs as the desktop icon.

use resvg::{tiny_skia, usvg};

const SVG: &str = include_str!("../flatpak/io.github.marcinkoza0922.Padwight.svg");

/// The icon as straight-alpha RGBA pixels, `size` by `size`.
pub fn rgba(size: u32) -> Vec<u8> {
    let tree = usvg::Tree::from_str(SVG, &usvg::Options::default()).expect("the bundled icon is valid SVG");
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("icon sizes are non-zero");
    let scale = size as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // The rasterizer leaves color premultiplied by alpha; window and tray hosts expect straight.
    let mut rgba = pixmap.take();
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = (u32::from(*c) * 255 / a).min(255) as u8;
            }
        }
    }
    rgba
}

/// The icon for a settings window's title bar and task switcher.
pub fn window() -> Option<iced::window::Icon> {
    iced::window::icon::from_rgba(rgba(256), 256, 256).ok()
}

#[cfg(test)]
mod tests {
    use super::rgba;

    #[test]
    fn rounded_corner_is_transparent_and_body_opaque() {
        let size = 64;
        let px = rgba(size);
        assert_eq!(px.len(), (size * size * 4) as usize);
        assert_eq!(px[3], 0, "corner is outside the rounded tile");
        let center = ((size / 2 * size + size / 2) * 4) as usize;
        assert_eq!(px[center + 3], 255, "center is inside the tile");
    }

    #[test]
    fn window_icon_builds() {
        assert!(super::window().is_some());
    }
}

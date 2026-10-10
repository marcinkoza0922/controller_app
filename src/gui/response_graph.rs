//! The mouse stick's response in two pictures: the pointer's speed against how far the stick is
//! pushed (the acceleration curve), and the pointer's speed in every direction at one push, with
//! the live stick's speed when a controller is connected. Hovering the curve picks the push both
//! show, so the curve's settings can be read off at any point.

use std::fmt::Write;

use iced::widget::{column, mouse_area, row, space, svg};
use iced::{Element, Length};

use super::*;
use crate::config::MouseResponse;
use crate::engine::apply_deadzone;
use crate::engine::stick::{OUTER_EDGE, steady_speed};

/// The curve's size, and the side of the direction shape, in logical pixels; `PAD` is kept clear
/// inside both.
const W: f32 = 280.0;
const H: f32 = 170.0;
const SIDE: f32 = 170.0;
const PAD: f32 = 10.0;
const PANEL: &str = "#1d2026";
const GRID: &str = "#30343c";
const LINEAR: &str = "#5d6470";
const STEADY: &str = "#4ea1ff";
const HELD: &str = "#e8a33d";
const MARK: &str = "#ffffff";

/// What the graphs show: the mouse stick's speed and response, its curve and deadzone, the push
/// that hovering the curve picked (0..1), and the live stick position when a controller is on.
#[derive(Clone, Copy)]
pub(super) struct Shown {
    pub(super) speed: f32,
    pub(super) curve: f32,
    pub(super) response: MouseResponse,
    pub(super) deadzone: f32,
    pub(super) probe: f32,
    pub(super) live: Option<(f32, f32)>,
}

impl Shown {
    /// The pointer's speed in px/s at a push of `mag` (0..1, after the deadzone), before
    /// acceleration.
    fn steady(&self, mag: f32) -> f32 {
        self.speed * steady_speed(mag, self.curve, self.response.outer_boost)
    }

    /// The same once acceleration has ramped up: only a push past the outer edge is accelerated.
    fn held(&self, mag: f32) -> f32 {
        let base = self.steady(mag);
        if mag >= OUTER_EDGE { base * (1.0 + self.response.accel) } else { base }
    }

    /// The fastest the pointer goes: full push, fully accelerated. Both pictures are scaled to it.
    fn top(&self) -> f32 {
        (self.speed * (1.0 + self.response.outer_boost) * (1.0 + self.response.accel)).max(1.0)
    }

    /// The push the graphs show, within the stick's travel.
    fn push(&self) -> f32 {
        self.probe.clamp(0.0, 1.0)
    }
}

/// `steps + 1` values evenly from `from` to `to`.
fn ramp(from: f32, to: f32, steps: u32) -> impl Iterator<Item = f32> {
    (0..=steps).map(move |i| from + (to - from) * i as f32 / steps as f32)
}

fn line(from: (f32, f32), to: (f32, f32), stroke: &str, dash: &str, width: f32) -> String {
    format!(
        r#"<path d="M{:.1} {:.1} L{:.1} {:.1}" fill="none" stroke="{stroke}" stroke-width="{width}" stroke-dasharray="{dash}"/>"#,
        from.0, from.1, to.0, to.1
    )
}

fn polyline(points: impl Iterator<Item = (f32, f32)>, stroke: &str, dash: &str, width: f32) -> String {
    let mut d = String::new();
    for (i, (x, y)) in points.enumerate() {
        let _ = write!(d, "{} {x:.1} {y:.1} ", if i == 0 { "M" } else { "L" });
    }
    format!(r#"<path d="{d}" fill="none" stroke="{stroke}" stroke-width="{width}" stroke-dasharray="{dash}" stroke-linejoin="round"/>"#)
}

fn dot(at: (f32, f32), color: &str) -> String {
    format!(r##"<circle cx="{:.1}" cy="{:.1}" r="4.5" fill="{color}" stroke="#000" stroke-width="1"/>"##, at.0, at.1)
}

/// SVG of the acceleration curve: the pointer's speed (up) against the push (across). A dotted
/// line shows a plain linear stick for comparison, the solid one the curve and boost, and the
/// orange one what acceleration adds once the stick is held past the outer edge. The marker is
/// the push, with the speeds at it.
fn curve_svg(s: &Shown) -> String {
    let top = s.top();
    let x = |m: f32| PAD + m * (W - 2.0 * PAD);
    let y = |v: f32| H - PAD - v / top * (H - 2.0 * PAD);
    let p = s.push();

    let mut out = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><rect width="{W}" height="{H}" rx="6" fill="{PANEL}"/>"#);
    out.push_str(&line((x(OUTER_EDGE), PAD), (x(OUTER_EDGE), H - PAD), GRID, "3 3", 1.0));
    out.push_str(&line((PAD, H - PAD), (W - PAD, H - PAD), GRID, "none", 1.0));
    out.push_str(&polyline(ramp(0.0, 1.0, 100).map(|m| (x(m), y(s.speed * m))), LINEAR, "3 3", 1.5));
    out.push_str(&polyline(ramp(0.0, 1.0, 100).map(|m| (x(m), y(s.steady(m)))), STEADY, "none", 2.0));
    if s.response.accel > 0.0 {
        out.push_str(&polyline(ramp(OUTER_EDGE, 1.0, 20).map(|m| (x(m), y(s.held(m)))), HELD, "5 3", 2.0));
    }
    out.push_str(&line((x(p), PAD), (x(p), H - PAD), MARK, "none", 1.0));
    out.push_str(&dot((x(p), y(s.steady(p))), STEADY));
    if s.response.accel > 0.0 && p >= OUTER_EDGE {
        out.push_str(&dot((x(p), y(s.held(p))), HELD));
    }
    out.push_str("</svg>");
    out
}

/// SVG of the pointer's speed in every direction at the push: the pointer moves as an ellipse,
/// as wide as its speed and as tall as that times the vertical scale. A dashed ellipse marks the
/// fastest it goes, and the live stick, when there is one, is a white dot.
fn shape_svg(s: &Shown) -> String {
    let c = SIDE / 2.0;
    let vertical = s.response.y_scale;
    // Scaled so the widest direction reaches the edge of the square.
    let k = (c - PAD) / (s.top() * vertical.max(1.0));
    // An ellipse of pointer speed `v` (px/s) sideways, and `v × vertical` up and down. `fill` is
    // the fill's attributes, such as `fill="none"`.
    let ellipse = |v: f32, fill: &str, stroke: &str, dash: &str| {
        format!(
            r#"<ellipse cx="{c}" cy="{c}" rx="{:.1}" ry="{:.1}" {fill} stroke="{stroke}" stroke-width="1.5" stroke-dasharray="{dash}"/>"#,
            v * k,
            v * k * vertical
        )
    };
    let mut out = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{SIDE}" height="{SIDE}" viewBox="0 0 {SIDE} {SIDE}"><rect width="{SIDE}" height="{SIDE}" rx="6" fill="{PANEL}"/>"#);
    out.push_str(&ellipse(s.top(), r#"fill="none""#, GRID, "3 3"));
    out.push_str(&ellipse(s.held(s.push()), &format!(r#"fill="{STEADY}" fill-opacity="0.25""#), STEADY, "none"));
    if let Some((x, y)) = s.live {
        let (x, y) = apply_deadzone(x, y, s.deadzone);
        let mag = x.hypot(y);
        if mag > 0.0 {
            let v = s.steady(mag) * k;
            out.push_str(&dot((c + x / mag * v, c + y / mag * v * vertical), MARK));
        }
    }
    out.push_str("</svg>");
    out
}

/// What the curve says at the push, in words.
fn curve_readout(s: &Shown) -> String {
    let p = s.push();
    let now = s.steady(p);
    if s.response.accel > 0.0 && p >= OUTER_EDGE {
        let full = s.held(p);
        format!("At {:.0}% push: {now:.0} px/s, {full:.0} px/s after {} ms held at full push", p * 100.0, s.response.accel_ramp_ms)
    } else {
        format!("At {:.0}% push: {now:.0} px/s", p * 100.0)
    }
}

/// Both pictures, side by side. Hovering the curve sets the push both of them show.
pub(super) fn view<'a>(shown: Shown) -> Element<'a, Message> {
    let curve_pic = mouse_area(svg(svg::Handle::from_memory(curve_svg(&shown).into_bytes())).width(W).height(H))
        .on_move(|at| Message::ProbeStick(((at.x - PAD) / (W - 2.0 * PAD)).clamp(0.0, 1.0)))
        .on_exit(Message::ProbeStick(1.0));
    let shape_pic = svg(svg::Handle::from_memory(shape_svg(&shown).into_bytes())).width(SIDE).height(SIDE);

    let faint = |s: &'static str| text(s).size(12).color(MUTED_COLOR);
    let ticks = row![faint("0%"), space().width(Length::Fill), faint("Stick push"), space().width(Length::Fill), faint("100%")].width(W);
    let curve_col = column![
        text("Acceleration curve").size(13).color(MUTED_COLOR),
        curve_pic,
        ticks,
        text(curve_readout(&shown)).size(13),
    ]
    .spacing(6)
    .width(W);
    let sideways = shown.held(shown.push());
    let shape_col = column![
        text("Pointer speed by direction").size(13).color(MUTED_COLOR),
        shape_pic,
        text(format!("Sideways {sideways:.0} px/s, up and down {:.0} px/s", sideways * shown.response.y_scale)).size(13),
    ]
    .spacing(6)
    .width(W);
    row![curve_col, shape_col].spacing(24).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(response: MouseResponse, probe: f32) -> Shown {
        Shown { speed: 100.0, curve: 1.0, response, deadzone: 0.1, probe, live: None }
    }

    fn tuned() -> MouseResponse {
        MouseResponse { accel: 0.5, outer_boost: 0.2, y_scale: 0.5, ..MouseResponse::default() }
    }

    #[test]
    fn held_speed_adds_the_acceleration_only_past_the_outer_edge() {
        let s = Shown { curve: 2.0, ..shown(tuned(), 1.0) };
        assert!((s.held(1.0) - 100.0 * 1.2 * 1.5).abs() < 0.01);
        assert!((s.held(0.5) - s.steady(0.5)).abs() < 0.01);
    }

    #[test]
    fn the_curve_marks_the_push_and_draws_the_held_speed_only_with_acceleration() {
        // Push marker, steady speed dot; the held dot appears only past the edge with acceleration.
        assert_eq!(curve_svg(&shown(tuned(), 0.5)).matches("<circle").count(), 1);
        assert_eq!(curve_svg(&shown(tuned(), 0.95)).matches("<circle").count(), 2);
        assert_eq!(curve_svg(&shown(MouseResponse::default(), 0.95)).matches("<circle").count(), 1);
        assert_eq!(curve_svg(&shown(MouseResponse::default(), 0.95)).matches(r##"fill="#e8a33d""##).count(), 0);
        assert_eq!(curve_svg(&shown(tuned(), 0.95)).matches(r##"fill="#e8a33d""##).count(), 1);
    }

    #[test]
    fn the_shape_shows_a_live_stick_only_with_a_controller() {
        let mut s = shown(tuned(), 1.0);
        assert_eq!(shape_svg(&s).matches("<circle").count(), 0);
        s.live = Some((0.0, 0.0));
        assert_eq!(shape_svg(&s).matches("<circle").count(), 0);
        s.live = Some((0.8, 0.0));
        assert_eq!(shape_svg(&s).matches("<circle").count(), 1);
    }

    #[test]
    fn the_readout_names_the_push_and_the_held_speed() {
        assert_eq!(curve_readout(&shown(MouseResponse::default(), 0.5)), "At 50% push: 50 px/s");
        assert!(curve_readout(&shown(tuned(), 1.0)).contains("after 400 ms held at full push"));
    }
}

//! How the composer looks in plan mode, and so the new-workspace dialog, where a prompt is written too: in the brand's
//! copper, quieter than keyboard focus, over a texture of the fan's rows, as dotted half circles of fine copper dots
//! around a point just under the composer's bottom edge, as if the composer lay over a roundhouse far bigger than
//! itself. The texture fades toward the bottom, so the colour marks the whole composer and the texture doesn't crowd
//! what's typed or pressed.

use std::f32::consts::PI;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AbsoluteLength, App, Bounds, Hsla, IntoElement, Pixels, Point, Styled, canvas, point, px};

use crate::fan::{self, as_f32};

/// Between neighbouring dots along a row, about. Much closer than the rows are, so each row reads as a dotted line. At
/// the mark's own spacing the rows of a fan this big blur into an even field of dots.
const ALONG: f32 = 4.0;
const ROW_GAP: f32 = 11.0;
const DOT: f32 = 0.7;
/// How far under the composer's bottom edge the rows' centre is, and the inner row's radius, so the rows that crowd
/// around it fall below the buttons.
const BELOW: f32 = 24.0;
const INNER: f32 = 30.0;
/// How strongly the top of the texture shows the copper, and how much of that is gone by the bottom.
const STRONGEST: f32 = 0.29;
const FADE: f32 = 0.8;
/// How much copper the composer's surface takes, and its edge over that: a little over half, so it's quieter than the
/// full copper of keyboard focus, which the composer's edge also shows while files are dragged over it.
const WASH: f32 = 0.10;
const EDGE: f32 = 0.55;

/// The surface and edge of the composer while it's planning, when its own surface is `surface`.
pub fn plan_colors(surface: Hsla, cx: &App) -> (Hsla, Hsla) {
    let copper = cx.theme().ring;
    let planning = surface.blend(copper.opacity(WASH));
    (planning, planning.blend(copper.opacity(EDGE)))
}

/// Fills its parent, which it lies under, on the surface [`plan_colors`] gives. `corner` is the radius of the parent's
/// corners, which the dots stay inside.
pub fn plan_texture(cx: &App, corner: AbsoluteLength) -> impl IntoElement + use<> {
    let copper = cx.theme().ring;
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            for (at, strength) in dots(bounds, corner.to_pixels(window.rem_size())) {
                fan::disc(window, at, px(DOT), copper.opacity(strength));
            }
        },
    )
    .size_full()
}

/// Each dot inside `bounds` and its corners, and how strongly it shows. Most of each big row is outside, above the
/// composer or beyond its sides, so only the dots of a row that can be inside are worked out.
fn dots(bounds: Bounds<Pixels>, corner: Pixels) -> Vec<(Point<Pixels>, f32)> {
    let center = point(bounds.origin.x + bounds.size.width * 0.5, bounds.bottom() + px(BELOW));
    let half_width = bounds.size.width * 0.5 / px(1.);
    let to_top = bounds.size.height / px(1.) + BELOW;
    let mut placed = Vec::new();
    let mut radius = INNER;
    while radius < half_width.hypot(to_top) {
        let spaces = spaces(radius);
        let (first, last) = from_the_ends(radius, spaces, half_width, to_top);
        // The same dots counted from each end, the left ones first, and the top one once.
        let right = (spaces - last).max(last + 1)..=spaces - first;
        for j in (first..=last).chain(right) {
            let at = fan::on_half_circle(center, px(radius), as_f32(j) / as_f32(spaces));
            if inside(bounds, corner, at) {
                placed.push((at, STRONGEST * fade(bounds, at)));
            }
        }
        radius += ROW_GAP;
    }
    placed
}

/// How many spaces between dots a row of `radius` has: as near [`ALONG`] apart as an even number allows, so the row
/// has a dot at its top, over the composer's middle.
fn spaces(radius: f32) -> usize {
    2 * whole((PI * radius / ALONG / 2.0).round())
}

/// Which dots of a row, counted from either of its ends, can be inside: below the top edge, above the bottom edge
/// and inside the sides.
fn from_the_ends(radius: f32, spaces: usize, half_width: f32, to_top: f32) -> (usize, usize) {
    let per_dot = PI / as_f32(spaces);
    let lowest = (half_width / radius).min(1.0).acos().max((BELOW / radius).min(1.0).asin());
    // A row that doesn't reach the top edge is inside all the way up, which rounding could cut short of its top dot.
    let top = spaces / 2;
    let last = if radius <= to_top { top } else { whole(((to_top / radius).asin() / per_dot).floor()).min(top) };
    (whole((lowest / per_dot).ceil()), last)
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a row's dots, a few hundred at most")]
fn whole(count: f32) -> usize {
    count as usize
}

/// From 1 at the top to `1 - FADE` at the bottom.
fn fade(bounds: Bounds<Pixels>, at: Point<Pixels>) -> f32 {
    let down = (at.y - bounds.origin.y) / bounds.size.height;
    (1.0 - down * FADE).clamp(0.0, 1.0)
}

/// Inside `bounds` by a dot's width, and inside its corners of radius `corner`.
fn inside(bounds: Bounds<Pixels>, corner: Pixels, at: Point<Pixels>) -> bool {
    let margin = px(DOT * 2.0);
    let (left, right) = (bounds.origin.x + margin, bounds.right() - margin);
    let (top, bottom) = (bounds.origin.y + margin, bounds.bottom() - margin);
    if at.x < left || at.x > right || at.y < top || at.y > bottom {
        return false;
    }
    let nearest_x = at.x.clamp(bounds.origin.x + corner, bounds.right() - corner);
    let nearest_y = at.y.clamp(bounds.origin.y + corner, bounds.bottom() - corner);
    ((at.x - nearest_x) / px(1.)).hypot((at.y - nearest_y) / px(1.)) <= (corner - margin) / px(1.)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use gpui_kit::{Bounds, Pixels, Point, point, px, size};

    use super::{BELOW, INNER, ROW_GAP, dots, spaces};
    use crate::fan::{as_f32, on_half_circle};

    const CORNER: f32 = 7.0;

    fn composer() -> Bounds<Pixels> {
        Bounds::new(point(px(16.), px(600.)), size(px(748.), px(112.)))
    }

    fn wide() -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(0.)), size(px(1540.), px(220.)))
    }

    fn placed(bounds: Bounds<Pixels>) -> Vec<(Point<Pixels>, f32)> {
        dots(bounds, px(CORNER))
    }

    #[test]
    fn it_fades_toward_the_buttons() {
        let bounds = composer();
        let placed = placed(bounds);
        let mean_between = |from: f32, to: f32| {
            let band: Vec<f32> = placed
                .iter()
                .filter(|(at, _)| (from..to).contains(&((at.y - bounds.origin.y) / px(1.))))
                .map(|(_, strength)| *strength)
                .collect();
            band.iter().sum::<f32>() / f32::from(u16::try_from(band.len()).unwrap())
        };

        // Without the fade they'd be the same.
        let (top, bottom) = (mean_between(0., 20.), mean_between(92., 112.));
        assert!(top > bottom * 1.5, "{top} at the top against {bottom} at the bottom");
    }

    /// Every row is a half circle around the same centre, its dots the same distance apart along it.
    #[test]
    fn its_dots_lie_on_rows_around_a_point_under_the_composer() {
        let bounds = composer();
        let center = (bounds.center().x / px(1.), bounds.bottom() / px(1.) + BELOW);

        for (at, _) in placed(bounds) {
            let (x, y) = (at.x / px(1.), at.y / px(1.));
            let radius = (x - center.0).hypot(y - center.1);
            let rows_out = (radius - INNER) / ROW_GAP;
            assert!((rows_out - rows_out.round()).abs() < 1e-3, "{at:?} is off its row");
            let row = INNER + rows_out.round() * ROW_GAP;
            let along = (center.1 - y).atan2(center.0 - x) / PI * as_f32(spaces(row));
            assert!((along - along.round()).abs() < 1e-2, "{at:?} is between dots");
        }
    }

    /// So the texture is the same on both sides of the composer's middle.
    #[test]
    fn every_row_that_reaches_the_middle_has_a_dot_there() {
        let bounds = composer();
        let middle = bounds.center().x;
        let placed = placed(bounds);
        let mut radius = INNER;
        let mut rows = 0;
        while bounds.bottom() + px(BELOW - radius) > bounds.origin.y + px(2.0) {
            if radius > BELOW + 2.0 {
                let top = bounds.bottom() + px(BELOW - radius);
                assert!(placed.iter().any(|(at, _)| (at.x - middle).abs() < px(1e-3) && (at.y - top).abs() < px(1e-3)));
                rows += 1;
            }
            radius += ROW_GAP;
        }
        assert!(rows > 5, "{rows} rows");
    }

    /// Only the dots of each row that can be inside are worked out, and that loses none next to working out every
    /// dot. Both use the same test for being inside, so this checks which dots are tried, not that test.
    #[test]
    fn it_has_every_dot_of_every_row_that_falls_inside() {
        for bounds in [composer(), wide()] {
            let center = point(bounds.center().x, bounds.bottom() + px(BELOW));
            let mut every = Vec::new();
            let mut radius = INNER;
            while radius < 2000.0 {
                let spaces = spaces(radius);
                for j in 0..=spaces {
                    let at = on_half_circle(center, px(radius), as_f32(j) / as_f32(spaces));
                    if super::inside(bounds, px(CORNER), at) {
                        every.push(at);
                    }
                }
                radius += ROW_GAP;
            }

            let placed: Vec<_> = placed(bounds).into_iter().map(|(at, _)| at).collect();

            assert_eq!(placed, every);
        }
    }

    /// Just inside each corner, a dot beyond its curve is outside, and one further in is inside. No dot of these sizes
    /// happens to fall in a bottom corner's curve, so the test below can't show those are rounded.
    #[test]
    fn all_four_corners_are_rounded() {
        let bounds = composer();
        let (left, top, right, bottom) = (bounds.origin.x, bounds.origin.y, bounds.right(), bounds.bottom());
        for (x, y, across, down) in
            [(left, top, 1., 1.), (right, top, -1., 1.), (left, bottom, 1., -1.), (right, bottom, -1., -1.)]
        {
            let inset = |by: f32| point(x + px(across * by), y + px(down * by));
            assert!(!super::inside(bounds, px(CORNER), inset(2.0)), "{:?} is beyond the curve", inset(2.0));
            assert!(super::inside(bounds, px(CORNER), inset(4.0)), "{:?} is inside it", inset(4.0));
        }
    }

    #[test]
    fn every_dot_is_inside_the_composer_and_its_corners() {
        for bounds in [composer(), wide()] {
            let placed = placed(bounds);

            assert!(placed.len() > 1000, "{} dots", placed.len());
            let corners = [
                (bounds.origin.x, bounds.origin.y),
                (bounds.right(), bounds.origin.y),
                (bounds.origin.x, bounds.bottom()),
                (bounds.right(), bounds.bottom()),
            ];
            for (at, _) in placed {
                assert!(bounds.contains(&at), "{at:?}");
                for (x, y) in corners {
                    let (dx, dy) = ((at.x - x).abs() / px(1.), (at.y - y).abs() / px(1.));
                    if dx < CORNER && dy < CORNER {
                        assert!((CORNER - dx).hypot(CORNER - dy) < CORNER, "{at:?} is outside the corner");
                    }
                }
            }
        }
    }
}

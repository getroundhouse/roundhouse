//! The icon's smallest drawings, laid out on whole pixels, where the fan's own shape would blur.

use gpui_kit::{point, px};

use crate::fan;

/// A fan laid out in the display's pixels, on a tile `inset` from the edge: `rows` rows of dots `dot` across, with
/// every fan's counts, `spacing` apart along each row before they're moved onto whole pixels. The turntable is a hub
/// `hub` across, in a ring of one pixel out to `ring`, if any. The tile's middle falls between two pixels, so the hub
/// and the ring are an even number of pixels across, to centre on it.
pub(super) struct OnPixels {
    pub(super) inset: f32,
    pub(super) rows: usize,
    pub(super) dot: f32,
    pub(super) spacing: f32,
    pub(super) hub: f32,
    pub(super) ring: Option<f32>,
}

/// Where a fan on whole pixels goes, in pixels from the canvas's corner: its centre, and its dots.
pub(super) struct Layout {
    pub(super) center: (f32, f32),
    pub(super) dots: Vec<Square>,
}

/// A dot on whole pixels: its centre, how far along the fan it is from left to right, and its row.
pub(super) struct Square {
    pub(super) at: (f32, f32),
    pub(super) along: f32,
    pub(super) row: usize,
}

impl Layout {
    /// The left half is placed and the right mirrors it, so rounding can't make the two sides differ.
    pub(super) fn of(fan: &OnPixels, canvas: f32) -> Self {
        // Every row has a dot on the middle line, as in every fan, except where dots are an odd number of pixels
        // across: they sit on pixels' middles, so to mirror across the line between the tile's two middle pixels,
        // none can sit on it, and every row has one dot fewer, with a gap there instead.
        let first = fan::first_row(fan.rows) - usize::from(odd(fan.dot));
        let around =
            |center: (f32, f32)| fan::rows_of_dots(point(px(center.0), px(center.1)), px(fan.spacing), first, fan.rows);
        let outer = around((0., 0.)).map(|dot| -dot.at.y / px(1.)).fold(0., f32::max);
        let (up, down) = (outer + fan.dot / 2.0, (fan.hub / 2.0).max(fan.ring.unwrap_or(0.)));
        // On the line between the tile's two middle pixels, and centred up and down on what it reaches.
        let center = (canvas / 2.0, ((canvas + up - down) / 2.0).round());
        let mut dots: Vec<Square> = Vec::new();
        for dot in around(center).filter(|d| d.along <= 0.5) {
            let at = (on_whole_pixels(dot.at.x / px(1.), fan.dot), on_whole_pixels(dot.at.y / px(1.), fan.dot));
            dots.push(Square { at, along: dot.along, row: dot.row });
            if dot.along < 0.5 {
                dots.push(Square { at: (center.0 * 2.0 - at.0, at.1), along: 1.0 - dot.along, row: dot.row });
            }
        }
        Layout { center, dots }
    }
}

/// Where something `across` pixels wide covers whole pixels: centred on a pixel's edge if it's an even number of
/// pixels across, and on a pixel's middle if odd.
fn on_whole_pixels(v: f32, across: f32) -> f32 {
    if odd(across) { v.floor() + 0.5 } else { v.round() }
}

fn odd(pixels: f32) -> bool {
    (pixels / 2.0).fract() > 0.25
}

#[cfg(test)]
mod tests {
    use super::super::{Drawing, IconSize};
    use super::{Layout, OnPixels, odd};

    fn centres(layout: &Layout) -> Vec<(f32, f32)> {
        layout.dots.iter().map(|d| d.at).collect()
    }

    fn on_pixels() -> Vec<(u16, OnPixels)> {
        IconSize::ALL
            .into_iter()
            .filter_map(|size| match size.drawing() {
                Drawing::OnPixels(fan) => Some((size.pixels(), fan)),
                Drawing::Scaled { .. } => None,
            })
            .collect()
    }

    #[test]
    fn the_smallest_sizes_are_drawn_on_whole_pixels() {
        let sizes: Vec<u16> = on_pixels().into_iter().map(|(pixels, _)| pixels).collect();

        assert_eq!(sizes, [16, 32, 64]);
    }

    #[test]
    fn every_dot_on_whole_pixels_has_a_pixel_clear_around_it_and_sits_on_the_tile() {
        for (pixels, fan) in on_pixels() {
            let canvas = f32::from(pixels);
            let layout = Layout::of(&fan, canvas);

            assert!(centres(&layout).len() > 8, "{pixels}: only {} dots", centres(&layout).len());
            for (i, a) in centres(&layout).iter().enumerate() {
                for b in &centres(&layout)[i + 1..] {
                    let apart = (a.0 - b.0).abs().max((a.1 - b.1).abs());
                    assert!(apart >= fan.dot + 1., "{pixels}: dots at {a:?} and {b:?} touch");
                }
                let reach = (a.0 - fan.dot / 2.0, a.0 + fan.dot / 2.0, a.1 - fan.dot / 2.0, a.1 + fan.dot / 2.0);
                let tile = (fan.inset, canvas - fan.inset);
                assert!(reach.0 >= tile.0 && reach.1 <= tile.1 && reach.2 >= tile.0 && reach.3 <= tile.1, "{pixels}");
            }
        }
    }

    #[test]
    fn a_fan_on_whole_pixels_keeps_every_dot_of_its_rows() {
        // Four more a row, as in every fan: 5 and 9, and 7, 11 and 15. Dots one pixel across have one fewer.
        let expected = [(16, 4 + 8), (32, 5 + 9), (64, 7 + 11 + 15)];
        for ((pixels, fan), (size, dots)) in on_pixels().into_iter().zip(expected) {
            let layout = Layout::of(&fan, f32::from(pixels));

            assert_eq!((pixels, centres(&layout).len()), (size, dots));
        }
    }

    #[test]
    fn every_dot_on_whole_pixels_has_a_pixel_clear_of_the_turntable() {
        for (pixels, fan) in on_pixels() {
            let layout = Layout::of(&fan, f32::from(pixels));
            let turntable = (fan.hub / 2.0).max(fan.ring.unwrap_or(0.));
            let (cx, cy) = layout.center;

            for (x, y) in &centres(&layout) {
                let half = fan.dot / 2.0;
                let nearest = (cx.clamp(x - half, x + half), cy.clamp(y - half, y + half));
                let apart = (nearest.0 - cx).hypot(nearest.1 - cy);
                assert!(apart >= turntable + 1.0, "{pixels}: the dot at {x}, {y} is {apart} from the centre");
            }
        }
    }

    #[test]
    fn a_fan_on_whole_pixels_is_the_same_on_both_sides() {
        for (pixels, fan) in on_pixels() {
            let layout = Layout::of(&fan, f32::from(pixels));
            let mirrored = |d: &(f32, f32)| (layout.center.0 * 2.0 - d.0, d.1);

            for dot in &centres(&layout) {
                assert!(centres(&layout).contains(&mirrored(dot)), "{pixels}: {dot:?} has no twin");
            }
        }
    }

    #[test]
    fn a_fan_on_whole_pixels_sits_in_the_middle_of_its_tile() {
        for (pixels, fan) in on_pixels() {
            let canvas = f32::from(pixels);
            let layout = Layout::of(&fan, canvas);
            let left = centres(&layout).iter().map(|d| d.0 - fan.dot / 2.0).fold(f32::MAX, f32::min);
            let right = centres(&layout).iter().map(|d| d.0 + fan.dot / 2.0).fold(f32::MIN, f32::max);

            let (space_left, space_right) = (left - fan.inset, canvas - fan.inset - right);
            assert!((space_left - space_right).abs() < 0.01, "{pixels}: {space_left} on the left, {space_right} right");
            assert!((layout.center.0 - canvas / 2.0).abs() < 0.01, "{pixels}: the turntable");
        }
    }

    #[test]
    fn every_row_of_a_fan_on_whole_pixels_has_the_same_middle() {
        for (pixels, fan) in on_pixels() {
            let layout = Layout::of(&fan, f32::from(pixels));
            let on_the_line = centres(&layout).iter().filter(|d| (d.0 - layout.center.0).abs() < 0.01).count();

            let expected = if odd(fan.dot) { 0 } else { fan.rows };
            assert_eq!(on_the_line, expected, "{pixels}: dots on the middle line");
        }
    }

    #[test]
    fn the_turntable_centres_on_the_line_between_two_pixels() {
        for (pixels, fan) in on_pixels() {
            assert!(!odd(fan.hub), "{pixels}: the hub is {} across", fan.hub);
            if let Some(ring) = fan.ring {
                assert!(!odd(ring * 2.0), "{pixels}: the ring is {} across", ring * 2.0);
            }
        }
    }
}

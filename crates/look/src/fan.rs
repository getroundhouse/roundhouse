//! The fan: a half circle of rows of dots around a turntable, like a roundhouse seen from above. The status marks,
//! the brand and the app icon all draw it.

use std::f32::consts::PI;

use gpui_kit::{
    BorderStyle, Bounds, Corners, Edges, Hsla, Pixels, Point, Window, point, px, quad, size, transparent_black,
};

/// How tall a fan is, for its width: a half circle, with room under it for the turntable.
pub(crate) const ASPECT: f32 = 0.62;
/// The inner row's radius, as a share of the outer row's. Inside it is the turntable.
const INNER: f32 = 0.4;
/// A dot's radius, as a share of the space between rows.
const DOT: f32 = 0.32;
/// The turntable's ring, in the dot radii of a four-row fan.
const TURNTABLE: f32 = 2.6;
/// The rows the turntable, and where it sits, are sized from.
const TURNTABLE_ROWS: usize = 4;

/// How many rows of dots a fan `width` wide has: more as it grows, so its dots stay fine. The status marks, 16 and 18
/// wide, have four, and the empty state's, 72 wide, has five.
fn rows_for(width: Pixels) -> usize {
    match width / px(1.) {
        w if w <= 32. => 4,
        w if w <= 76. => 5,
        w if w <= 120. => 7,
        _ => 9,
    }
}

/// One dot: where it is, how far along the fan from left to right, from 0 to 1, and which row, from the inside.
pub(crate) struct Dot {
    pub(crate) at: Point<Pixels>,
    pub(crate) along: f32,
    pub(crate) row: usize,
}

/// The turntable, at the fan's centre: a ring, and a hub inside it.
pub(crate) struct Turntable {
    /// To the ring's outer edge.
    pub(crate) radius: Pixels,
    pub(crate) line: Pixels,
    pub(crate) hub: Pixels,
}

/// Where everything sits inside a fan's bounds.
pub(crate) struct Fan {
    pub(crate) center: Point<Pixels>,
    inner: Pixels,
    pitch: Pixels,
    /// Every dot's radius.
    pub(crate) dot: Pixels,
    rows: usize,
    /// A four-row fan's dot, at the same width. The turntable, and where it sits, come from it, so a fan with more
    /// rows has finer dots and nothing else changes.
    turntable_dot: Pixels,
}

impl Fan {
    /// With as many rows as its width calls for.
    pub(crate) fn within(bounds: Bounds<Pixels>) -> Self {
        Fan::with_rows(bounds, rows_for(bounds.size.width))
    }

    pub(crate) fn with_rows(bounds: Bounds<Pixels>, rows: usize) -> Self {
        let outer = bounds.size.width * 0.5;
        let inner = outer * INNER;
        let dot_for = |rows: usize| ((outer - inner) / (as_f32(rows) - 0.4) * DOT).max(px(0.45));
        let pitch = (outer - inner) / (as_f32(rows) - 0.4);
        let dot = dot_for(rows);
        let turntable_dot = dot_for(TURNTABLE_ROWS);
        let bottom = bounds.origin.y + bounds.size.height - turntable_dot * 1.4;
        Fan { center: point(bounds.origin.x + bounds.size.width * 0.5, bottom), inner, pitch, dot, rows, turntable_dot }
    }

    pub(crate) fn dots(&self) -> impl Iterator<Item = Dot> + use<> {
        rows_of_dots(self.center, self.inner, self.pitch, self.rows, Count::Fitting)
    }

    /// Its line and hub keep a share of its size, so they don't thin out as the dots get finer.
    pub(crate) fn turntable(&self) -> Turntable {
        let radius = self.turntable_dot * TURNTABLE;
        Turntable { radius, line: (self.dot * 0.8).max(radius * 0.16).max(px(0.6)), hub: self.dot.max(radius * 0.28) }
    }

    /// How far the fan reaches above its centre, to the outer row's edge, and below it, to the turntable's.
    pub(crate) fn reach(&self) -> (Pixels, Pixels) {
        (self.inner + self.pitch * as_f32(self.rows - 1) + self.dot, self.turntable().radius)
    }
}

/// How many dots a row gets.
#[derive(Clone, Copy)]
pub(crate) enum Count {
    /// As many as fit `pitch` apart.
    Fitting,
    /// The even number nearest that, so no dot sits on the middle line. A fan of dots an odd number of pixels across
    /// needs it to centre on the line between two pixels.
    Even,
}

/// Rows of dots in a half circle over `center`, the first `inner` from it and each `pitch` further out. The dots in
/// a row are spaced evenly, about `pitch` apart, so outer rows have more of them and the edge stays as dense as the
/// middle.
pub(crate) fn rows_of_dots(
    center: Point<Pixels>,
    inner: Pixels,
    pitch: Pixels,
    rows: usize,
    count: Count,
) -> impl Iterator<Item = Dot> + use<> {
    (0..rows).flat_map(move |row| {
        let r = inner + pitch * as_f32(row);
        let count = match count {
            Count::Fitting => dots_in((r * PI) / pitch) + 1,
            Count::Even => 2 * dots_in((r * PI) / pitch / 2.0 + 0.5),
        };
        (0..count).map(move |j| {
            let along = as_f32(j) / as_f32(count - 1);
            let angle = PI * (along - 1.0);
            Dot { at: point(center.x + r * angle.cos(), center.y + r * angle.sin()), along, row }
        })
    })
}

pub(crate) fn disc(window: &mut Window, center: Point<Pixels>, radius: Pixels, color: Hsla) {
    window.paint_quad(quad(
        square(center, radius),
        Corners::all(radius),
        color,
        Edges::all(px(0.)),
        transparent_black(),
        BorderStyle::default(),
    ));
}

/// `radius` is to the ring's outer edge, and the line is drawn inside it.
pub(crate) fn ring(window: &mut Window, center: Point<Pixels>, radius: Pixels, line: Pixels, color: Hsla) {
    window.paint_quad(quad(
        square(center, radius),
        Corners::all(radius),
        transparent_black(),
        Edges::all(line),
        color,
        BorderStyle::default(),
    ));
}

fn square(center: Point<Pixels>, radius: Pixels) -> Bounds<Pixels> {
    Bounds::new(point(center.x - radius, center.y - radius), size(radius * 2.0, radius * 2.0))
}

#[expect(clippy::cast_precision_loss, reason = "a count of rows or dots, a few dozen at most")]
pub(crate) fn as_f32(n: usize) -> f32 {
    n as f32
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "how many dots fit along a row: positive, and a few dozen at most"
)]
fn dots_in(length_in_pitches: f32) -> usize {
    length_in_pitches.round().max(1.0) as usize
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};

    use super::{ASPECT, Fan};

    fn fan(width: f32) -> Fan {
        Fan::within(Bounds::new(point(px(0.), px(0.)), size(px(width), px(width * ASPECT))))
    }

    #[test]
    fn outer_rows_have_more_dots_and_every_row_reaches_both_ends() {
        let fan = fan(60.);
        let dots: Vec<_> = fan.dots().collect();
        let per_row: Vec<usize> = (0..fan.rows).map(|row| dots.iter().filter(|d| d.row == row).count()).collect();

        assert!(per_row.windows(2).all(|pair| pair[0] < pair[1]), "{per_row:?}");
        for row in 0..fan.rows {
            let along: Vec<f32> = dots.iter().filter(|d| d.row == row).map(|d| d.along).collect();
            assert_eq!((along.first().copied(), along.last().copied()), (Some(0.0), Some(1.0)));
        }
    }

    #[test]
    fn a_wider_fan_has_more_rows_so_its_dots_stay_fine() {
        let widths = [16., 18., 72., 100., 320.];
        let fans: Vec<Fan> = widths.iter().map(|w| fan(*w)).collect();

        assert_eq!((fans[0].rows, fans[1].rows), (4, 4), "the status marks have four rows");
        assert!(fans.windows(2).all(|pair| pair[0].rows <= pair[1].rows), "rows only grow");
        let fineness: Vec<f32> = fans.iter().zip(widths).map(|(fan, w)| fan.dot / px(w)).collect();
        assert!(fineness.windows(2).all(|pair| pair[1] <= pair[0]), "{fineness:?}");
    }

    #[test]
    fn more_rows_leave_the_turntable_as_it_was() {
        let share = |width: f32| {
            let fan = fan(width);
            (fan.turntable().radius / px(width), (fan.center.y - px(width * ASPECT)) / px(width))
        };

        for width in [72., 100., 320.] {
            let (radius, from_bottom) = share(width);
            let (four_rows, four_rows_from_bottom) = share(30.);
            assert!((radius - four_rows).abs() < 1e-5, "{width}: {radius} against {four_rows}");
            assert!((from_bottom - four_rows_from_bottom).abs() < 1e-5, "{width}");
        }
    }
}

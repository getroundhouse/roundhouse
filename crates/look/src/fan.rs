//! The fan: a half circle of rows of dots around a turntable, like a roundhouse seen from above. The status marks,
//! the brand and the app icon all draw it.

use std::f32::consts::PI;

use gpui_kit::{
    BorderStyle, Bounds, Corners, Edges, Hsla, Pixels, Point, Window, point, px, quad, size, transparent_black,
};

/// How tall a fan's box is, for its width: a half circle, with room under it for the turntable. The fan sits in the
/// middle of it, top to bottom.
pub(crate) const ASPECT: f32 = 0.62;
/// How many more dots each row has than the one inside it. A row one dot's spacing further out would fit π more, so
/// with four the rows sit a little further apart than the dots in them. Four is even, so every row keeps the inner
/// row's odd count, with a dot on the middle line.
const MORE: usize = 4;
/// The inner row's radius is at least this share of the outer row's, 2/5, which leaves the turntable its hole.
const HOLE: (usize, usize) = (2, 5);
/// A dot's radius, as a share of the space between dots along a row.
const DOT: f32 = 0.32;
/// The space outside the outer row's dots, to the fan's edge, in dot radii.
const MARGIN: f32 = 1.25;
/// The turntable's ring, to its outer edge, as a share of the fan's width, whatever its rows.
const TURNTABLE: f32 = 0.07;

/// How many rows of dots a fan `width` wide has: more as it grows, so its dots stay fine. The status marks, 16 and 26
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
    /// Between neighbouring dots along a row, the same in every row.
    spacing: Pixels,
    rows: usize,
    /// Every dot's radius.
    pub(crate) dot: Pixels,
    /// The turntable's radius, which comes from the fan's width alone, so a fan with more rows has finer dots and
    /// nothing else changes.
    turntable: Pixels,
}

impl Fan {
    /// With as many rows as its width calls for.
    pub(crate) fn within(bounds: Bounds<Pixels>) -> Self {
        Fan::with_rows(bounds, rows_for(bounds.size.width))
    }

    /// The outer row is as big as fits, with `MARGIN` outside it, and every other row follows from it. What's drawn
    /// sits in the middle of the box, top to bottom, from the top of the outer row to the bottom of the turntable, so
    /// a fan centred beside a line of text looks level with it. The bottom dots of every row sit on the line through
    /// the centre, and the turntable's ring dips below it.
    pub(crate) fn with_rows(bounds: Bounds<Pixels>, rows: usize) -> Self {
        let width = bounds.size.width;
        let spaces = as_f32(first_row(rows) - 1 + MORE * (rows - 1));
        let outer = width * 0.5 / (1.0 + (1.0 + MARGIN) * DOT * PI / spaces);
        let spacing = outer * PI / spaces;
        let across = bounds.origin.x + width * 0.5;
        let fan = Fan {
            center: point(across, bounds.origin.y),
            spacing,
            rows,
            dot: (spacing * DOT).max(px(0.45)),
            turntable: width * TURNTABLE,
        };
        let (up, down) = fan.reach();
        Fan { center: point(across, bounds.origin.y + (bounds.size.height + up - down) * 0.5), ..fan }
    }

    pub(crate) fn dots(&self) -> impl Iterator<Item = Dot> + use<> {
        rows_of_dots(self.center, self.spacing, first_row(self.rows), self.rows)
    }

    /// Its line and hub keep a share of its size, so they don't thin out as the dots get finer.
    pub(crate) fn turntable(&self) -> Turntable {
        let radius = self.turntable;
        Turntable { radius, line: (self.dot * 0.8).max(radius * 0.16).max(px(0.6)), hub: self.dot.max(radius * 0.28) }
    }

    /// How far the fan reaches above its centre, to the outer row's edge, and below it, to the turntable's.
    pub(crate) fn reach(&self) -> (Pixels, Pixels) {
        let outer = row_radius(self.spacing, first_row(self.rows) + MORE * (self.rows - 1));
        (outer + self.dot, self.turntable)
    }
}

/// How many dots the inner row of a fan of `rows` has: the fewest that keep its radius at least `HOLE` of the outer
/// row's, and odd. A row's radius is in proportion to the spaces between its dots, its count less one.
pub(crate) fn first_row(rows: usize) -> usize {
    let (part, whole) = HOLE;
    let spaces = (part * MORE * (rows - 1)).div_ceil(whole - part);
    spaces + spaces % 2 + 1
}

/// The radius of a half circle of `count` dots `spacing` apart, its first and last on the line through its centre.
fn row_radius(spacing: Pixels, count: usize) -> Pixels {
    spacing * as_f32(count - 1) / PI
}

/// Rows of dots in a half circle over `center`, `spacing` apart along every row, with `first` in the inner row and
/// `MORE` more in each row out. Each row is as far out as its dots need, so the rows are `MORE` spacings over π apart,
/// a little more than the dots in them, and the fan keeps the same pattern with any number of rows.
pub(crate) fn rows_of_dots(
    center: Point<Pixels>,
    spacing: Pixels,
    first: usize,
    rows: usize,
) -> impl Iterator<Item = Dot> + use<> {
    (0..rows).flat_map(move |row| {
        let count = first + MORE * row;
        let r = row_radius(spacing, count);
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

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};

    use super::{ASPECT, Dot, Fan, as_f32};

    fn fan(width: f32) -> Fan {
        Fan::within(Bounds::new(point(px(0.), px(0.)), size(px(width), px(width * ASPECT))))
    }

    fn with_rows(rows: usize) -> Fan {
        Fan::with_rows(Bounds::new(point(px(0.), px(0.)), size(px(200.), px(200. * ASPECT))), rows)
    }

    fn row(dots: &[Dot], row: usize) -> Vec<&Dot> {
        dots.iter().filter(|d| d.row == row).collect()
    }

    fn radius(fan: &Fan, dot: &Dot) -> f32 {
        ((dot.at.x - fan.center.x) / px(1.)).hypot((dot.at.y - fan.center.y) / px(1.))
    }

    #[test]
    fn outer_rows_have_more_dots_and_every_row_reaches_both_ends() {
        let fan = fan(60.);
        let dots: Vec<_> = fan.dots().collect();
        let per_row: Vec<usize> = (0..fan.rows).map(|r| row(&dots, r).len()).collect();

        assert!(per_row.windows(2).all(|pair| pair[0] < pair[1]), "{per_row:?}");
        for r in 0..fan.rows {
            let along: Vec<f32> = row(&dots, r).iter().map(|d| d.along).collect();
            assert_eq!((along.first().copied(), along.last().copied()), (Some(0.0), Some(1.0)));
        }
    }

    #[test]
    fn every_row_has_four_more_dots_than_the_one_inside_it() {
        let expected: [(usize, &[usize]); 4] =
            [(2, &[5, 9]), (3, &[7, 11, 15]), (4, &[9, 13, 17, 21]), (5, &[13, 17, 21, 25, 29])];
        for (rows, counts) in expected {
            let dots: Vec<_> = with_rows(rows).dots().collect();
            let per_row: Vec<usize> = (0..rows).map(|r| row(&dots, r).len()).collect();

            assert_eq!(per_row, counts, "{rows} rows");
        }
        for rows in 2..=9 {
            let dots: Vec<_> = with_rows(rows).dots().collect();
            let per_row: Vec<usize> = (0..rows).map(|r| row(&dots, r).len()).collect();
            assert!(per_row.windows(2).all(|pair| pair[1] == pair[0] + 4), "{rows} rows: {per_row:?}");
        }
    }

    #[test]
    fn dots_are_as_far_apart_along_every_row() {
        for rows in 2..=9 {
            let fan = with_rows(rows);
            let dots: Vec<_> = fan.dots().collect();
            let spacing: Vec<f32> = (0..rows)
                .map(|r| {
                    let row = row(&dots, r);
                    radius(&fan, row[0]) * std::f32::consts::PI / as_f32(row.len() - 1)
                })
                .collect();

            let (least, most) = spacing.iter().fold((f32::MAX, f32::MIN), |(a, b), s| (a.min(*s), b.max(*s)));
            assert!((most - least) / least < 1e-4, "{rows} rows: {spacing:?}");
        }
    }

    #[test]
    fn every_fan_has_a_dot_on_the_middle_line_of_every_row() {
        for width in [16., 26., 72., 100., 320.] {
            let fan = fan(width);
            let dots: Vec<_> = fan.dots().collect();

            for r in 0..fan.rows {
                let middle = row(&dots, r).iter().filter(|d| (d.along - 0.5).abs() < 1e-6).count();
                assert_eq!(middle, 1, "{width} wide, row {r}");
            }
        }
    }

    #[test]
    fn the_inner_row_has_the_fewest_dots_that_leave_the_turntable_its_hole() {
        for rows in 2..=9 {
            let fan = with_rows(rows);
            let dots: Vec<_> = fan.dots().collect();
            let (inner, outer) = (row(&dots, 0), row(&dots, rows - 1));
            let hole = radius(&fan, inner[0]) / radius(&fan, outer[0]);
            // The next odd count down: two fewer dots in every row. A row's radius goes with its count less one.
            let spaces = |row: &[&Dot]| as_f32(row.len() - 1);
            let fewer = (spaces(&inner) - 2.0) / (spaces(&outer) - 2.0);

            assert!(hole >= 0.4 - 1e-5, "{rows} rows: the hole is {hole} of the outer row");
            assert!(fewer < 0.4, "{rows} rows: two fewer would still leave {fewer}");
            assert!(
                radius(&fan, inner[0]) - fan.dot / px(1.) > (fan.turntable().radius + fan.dot) / px(1.),
                "{rows} rows"
            );
        }
    }

    #[test]
    fn a_wider_fan_has_more_rows_so_its_dots_stay_fine() {
        let widths = [16., 26., 72., 100., 320.];
        let fans: Vec<Fan> = widths.iter().map(|w| fan(*w)).collect();

        assert_eq!((fans[0].rows, fans[1].rows), (4, 4), "the status marks have four rows");
        assert!(fans.windows(2).all(|pair| pair[0].rows <= pair[1].rows), "rows only grow");
        let fineness: Vec<f32> = fans.iter().zip(widths).map(|(fan, w)| fan.dot / px(w)).collect();
        assert!(fineness.windows(2).all(|pair| pair[1] <= pair[0]), "{fineness:?}");
    }

    #[test]
    fn every_fan_fits_its_box_across() {
        for width in [16., 26., 72., 100., 320.] {
            let fan = fan(width);
            let (up, _) = fan.reach();

            assert!(up <= px(width) * 0.5, "{width} wide reaches {up:?}");
            for dot in fan.dots() {
                assert!(dot.at.x - fan.dot >= px(0.) && dot.at.x + fan.dot <= px(width), "{width} wide");
            }
        }
    }

    #[test]
    fn every_fan_sits_in_the_middle_of_its_box_from_top_to_bottom() {
        for width in [16., 26., 72., 100., 320.] {
            let fan = fan(width);
            let (up, down) = fan.reach();
            let (above, below) = ((fan.center.y - up) / px(1.), (px(width * ASPECT) - fan.center.y - down) / px(1.));

            assert!(above >= 0. && below >= 0., "{width} wide leaves {above} above and {below} below");
            assert!((above - below).abs() < 1e-3, "{width} wide leaves {above} above and {below} below");
        }
    }

    #[test]
    fn more_rows_leave_the_turntable_as_big_as_it_was() {
        let share = |width: f32| fan(width).turntable().radius / px(width);

        for width in [72., 100., 320.] {
            assert!((share(width) - share(30.)).abs() < 1e-5, "{width}: {} against {}", share(width), share(30.));
        }
    }
}

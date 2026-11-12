//! How a chat's prompt cache is drawn and said wherever it shows: a wedge for the share of its time it has left, which
//! isn't a bar or a ring so it can't be taken for the context's, a clock once its time is up or nearly, and how long
//! it has left, in minutes. `docs/spec/chats.md` says where each shows.

use std::f32::consts::TAU;

use gpui_kit::base::plot::shape::{Arc, ArcData};
use gpui_kit::{
    App, BorderStyle, Bounds, Corners, Edges, Hsla, IntoElement, ParentElement as _, Pixels, Point, Styled as _,
    Window, canvas, div, point, px, quad, size, transparent_black,
};

use crate::fan;

const MINUTE_MS: i64 = 60_000;

/// What a cache's mark shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CacheMark {
    /// A wedge filled clockwise from the top by the share of its time it has left, from 0 to 1, over a fainter disc.
    Left(f32),
    /// A clock, for one whose time is up, or nearly.
    Clock,
}

/// How wide and high a cache's mark is wherever it shows: as small text is tall, so a wedge of a quarter or less can
/// still be seen.
const ACROSS: Pixels = px(11.);

/// `mark` drawn in `color`, still, since nothing about a cache moves but the time.
pub fn cache_mark(mark: CacheMark, color: Hsla) -> impl IntoElement {
    let drawn = canvas(
        |bounds: Bounds<Pixels>, _: &mut Window, _: &mut App| bounds,
        move |_, bounds, window, _| {
            let radius = bounds.size.width.min(bounds.size.height) / 2.;
            let center = bounds.center();
            match mark {
                CacheMark::Left(left) => {
                    fan::disc(window, center, radius, color.opacity(0.25));
                    let left = left.clamp(0., 1.);
                    if left > 0. {
                        let wedge = Arc::new().inner_radius(0.).outer_radius(radius.as_f32());
                        wedge.paint(&ArcData::new(&(), 0, left, 0., left * TAU), color, &bounds, window);
                    }
                }
                CacheMark::Clock => clock(window, center, radius, color),
            }
        },
    )
    .size_full();
    div().flex_none().size(ACROSS).child(drawn)
}

/// A clock face with its hands at three o'clock.
fn clock(window: &mut Window, center: Point<Pixels>, radius: Pixels, color: Hsla) {
    let line = (radius / 5.).max(px(1.));
    fan::ring(window, center, radius, line, color);
    let hand = |from: Point<Pixels>, to: Point<Pixels>| {
        let reach = size(to.x - from.x, to.y - from.y);
        quad(
            Bounds::new(from, reach),
            Corners::all(line / 2.),
            color,
            Edges::all(px(0.)),
            transparent_black(),
            BorderStyle::default(),
        )
    };
    let half = line / 2.;
    let up = radius * 0.55;
    let across = radius * 0.45;
    window.paint_quad(hand(point(center.x - half, center.y - up), point(center.x + half, center.y + half)));
    window.paint_quad(hand(point(center.x - half, center.y - half), point(center.x + across, center.y + half)));
}

/// How long a cache has left, as "42 min", in whole minutes rounded up, so it never says a minute it hasn't got left
/// is gone, and never "0 min" while it's warm.
pub fn minutes_left(left_ms: i64) -> String {
    format!("{} min", (left_ms.max(1) + MINUTE_MS - 1) / MINUTE_MS)
}

/// What's said of a cache about to expire with `left_ms` left, wherever it's said: "Cache expires in 9 min".
pub fn expires_in(left_ms: i64) -> String {
    format!("Cache expires in {}", minutes_left(left_ms))
}

/// How long it is until what's said of a cache that has `left_ms` changes, in minutes, so whatever says it can be
/// drawn again then and not before. Once it has expired, nothing changes.
pub fn next_minute_in_ms(left_ms: i64) -> Option<i64> {
    let part = left_ms % MINUTE_MS;
    (left_ms > 0).then_some(if part == 0 { MINUTE_MS } else { part })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_left_is_in_whole_minutes_rounded_up() {
        assert_eq!(minutes_left(42 * MINUTE_MS), "42 min");
        assert_eq!(minutes_left(41 * MINUTE_MS + 1), "42 min");
        assert_eq!(minutes_left(1), "1 min");
        assert_eq!(minutes_left(0), "1 min", "said only while it's warm");
    }

    #[test]
    fn what_is_said_changes_as_a_whole_minute_goes_and_not_once_it_has_expired() {
        assert_eq!(next_minute_in_ms(42 * MINUTE_MS), Some(MINUTE_MS));
        assert_eq!(next_minute_in_ms(41 * MINUTE_MS + 300), Some(300));
        assert_eq!(next_minute_in_ms(0), None);
    }
}

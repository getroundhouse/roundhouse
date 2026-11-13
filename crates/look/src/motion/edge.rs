//! Scrolling on under a pointer held past the edge of what scrolls with its button down, as while selecting a chat's
//! text past its top, or dragging a project to the bottom of the sidebar: faster the further past the edge it's held,
//! for as long as it's held there. It goes by the pointer, rather than by a time it's set to take, so it's the same
//! under Reduce Motion.

use std::ops::Range;
use std::time::{Duration, Instant};

use gpui_kit::{Pixels, px};

/// How fast it scrolls, in points a second for each point past the edge.
const PAST_THE_EDGE: f32 = 30.;
/// The slowest and fastest it scrolls, in points a second.
const SLOWEST: f32 = 240.;
const FASTEST: f32 = 4800.;
/// The longest a frame is taken to have been, so a stall doesn't throw what scrolls a long way.
const LONGEST_FRAME: Duration = Duration::from_millis(50);

/// What scrolls on under a pointer held past its edge, which keeps when it last did.
#[derive(Clone, Copy, Default, Debug)]
pub struct ScrollingOn {
    /// When it last scrolled, while the pointer's been past the edge.
    moved: Option<Instant>,
}

impl ScrollingOn {
    /// How far to scroll down, at `now`, for a pointer `y` down the window, past the top or bottom of `edges`, if it
    /// is. Up is less than nothing. It's nothing the first frame the pointer's past the edge, which only starts the
    /// clock.
    pub fn by(&mut self, y: Pixels, edges: Range<Pixels>, now: Instant) -> Option<Pixels> {
        let past = if y < edges.start {
            y - edges.start
        } else if y > edges.end {
            y - edges.end
        } else {
            self.moved = None;
            return None;
        };
        let since = self.moved.replace(now).map_or(Duration::ZERO, |then| now.saturating_duration_since(then));
        let speed = (f32::from(past.abs()) * PAST_THE_EDGE).clamp(SLOWEST, FASTEST);
        Some(px(past.signum() * speed * since.min(LONGEST_FRAME).as_secs_f32()))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui_kit::px;

    use super::ScrollingOn;

    const FRAME: Duration = Duration::from_millis(10);

    /// How fast it scrolls, in points a second, in the frame after the one that starts the clock, with the pointer at
    /// `y` and the edges at 100 and 500.
    fn speed(y: f32) -> Option<f32> {
        let (mut scrolling, from) = (ScrollingOn::default(), Instant::now());
        scrolling.by(px(y), px(100.)..px(500.), from);
        let by = scrolling.by(px(y), px(100.)..px(500.), from + FRAME)?;
        Some((f32::from(by) / FRAME.as_secs_f32()).round())
    }

    #[test]
    fn it_scrolls_only_past_an_edge_toward_it_and_faster_the_further_past_it_is() {
        assert_eq!(speed(300.), None, "between the edges");
        assert_eq!(speed(100.), None, "on one");
        assert_eq!(speed(510.), Some(300.), "10pt past the bottom");
        assert_eq!(speed(80.), Some(-600.), "and 20pt past the top, up");
        assert_eq!(speed(501.), Some(240.), "but never slower than this");
        assert_eq!(speed(9_000.), Some(4_800.), "or faster than this");
    }

    #[test]
    fn it_scrolls_nothing_as_the_pointer_gets_past_and_a_stall_counts_as_a_short_frame() {
        let (mut scrolling, from, edges) = (ScrollingOn::default(), Instant::now(), px(100.)..px(500.));

        assert_eq!(scrolling.by(px(600.), edges.clone(), from), Some(px(0.)), "the first frame starts the clock");
        let stalled = scrolling.by(px(600.), edges.clone(), from + Duration::from_secs(2));
        assert_eq!(stalled, Some(px(150.)), "two seconds count as 50ms");
        assert_eq!(scrolling.by(px(300.), edges.clone(), from + Duration::from_secs(3)), None);
        let again = scrolling.by(px(600.), edges, from + Duration::from_secs(4));
        assert_eq!(again, Some(px(0.)), "and back past it, the clock starts again");
    }
}

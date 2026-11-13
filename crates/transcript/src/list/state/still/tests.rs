use std::time::Instant;

use gpui_kit::px;
use look::motion::Scrolling;

use super::super::tests::{at, kept_at_the_bottom, lay_from, scrolled_to, session};
use super::*;
use crate::list::layout::Start;

/// A session of 100 rows of 50 in a window 300 tall, keeping to its latest row, as a frame left it.
fn at_the_end() -> State {
    let mut state = session(100, 50.);
    state.laid_out(at(94, 0.), None, true);
    state
}

/// The last row, where it is in the window while the list keeps to it.
const LAST: Anchor = Anchor { row: 99, top: px(250.) };

/// Laid out from `at`, with nothing moved since.
fn held(at: Anchor) -> Start {
    Start::Anchored { at, moved: px(0.) }
}

#[test]
fn a_row_someone_opens_or_closes_something_in_is_kept_still_if_it_is_in_sight() {
    let mut state = scrolled_to(100, 50., 10, 20.);
    let first_in_sight = Anchor { row: 11, top: px(30.) };
    state.laid_out(at(10, 20.), Some(first_in_sight), false);

    state.keep_still(9, true);
    assert_eq!(lay_from(&state), held(first_in_sight), "not a row above the window");
    state.keep_still(17, true);
    assert_eq!(lay_from(&state), held(first_in_sight), "nor one below it");
    state.keep_still(13, true);
    assert_eq!(lay_from(&state), held(Anchor { row: 13, top: px(130.) }));
    state.keep_still(10, false);
    assert_eq!(lay_from(&state), held(Anchor { row: 10, top: px(-20.) }), "even past the window's top");
}

#[test]
fn opening_something_above_a_row_at_the_bottom_keeps_what_was_clicked_still() {
    let mut state = kept_at_the_bottom();

    state.keep_still(38, true);

    assert_eq!(lay_from(&state), held(Anchor { row: 38, top: px(150.) }));
}

/// Keeping to the latest row would put its end at the window's bottom, and what was opened would push the line
/// that was clicked up from under the pointer.
#[test]
fn opening_something_in_the_latest_row_keeps_the_row_still_rather_than_its_end_at_the_bottom() {
    let mut state = at_the_end();

    state.keep_still(99, true);
    assert_eq!(lay_from(&state), held(LAST));
    // It opened to more than the window has room for under it.
    state.laid_out(at(94, 0.), Some(LAST), false);

    assert!(state.is_scrolled_up(), "the end is out of sight, as if someone had scrolled away from it");
}

/// The rows come down to put the session's end at the window's bottom either way, so the list goes on following
/// what arrives, and isn't scrolled away from it meanwhile.
#[test]
fn closing_something_while_keeping_to_the_latest_row_goes_on_keeping_to_it() {
    let mut state = at_the_end();

    state.keep_still(99, false);

    assert_eq!(lay_from(&state), Start::Latest);
    assert!(!state.is_scrolled_up());
}

#[test]
fn a_click_stops_the_list_on_its_way_down_to_the_latest_row_and_holds_the_row() {
    // Near enough to the end to slide all the way, from where it is.
    let mut state = scrolled_to(100, 50., 90, 0.);
    state.head_to_latest(Scrolling::since(Instant::now()));

    state.keep_still(92, false);

    assert!(!state.is_heading_to_latest());
    assert_eq!(lay_from(&state), held(Anchor { row: 92, top: px(100.) }));
}

/// Between the click that opened something and the frame that lays it out, the list is at a place, so a turn of the
/// wheel meanwhile moves it on from the row held still.
#[test]
fn the_wheel_moves_on_from_a_row_held_still() {
    let mut state = at_the_end();
    state.keep_still(99, true);

    state.scroll_by(px(40.));

    assert_eq!(lay_from(&state), Start::Anchored { at: LAST, moved: px(-40.) });
}

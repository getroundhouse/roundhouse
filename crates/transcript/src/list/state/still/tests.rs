use gpui_kit::px;

use super::super::tests::{at, kept_at_the_bottom, lay_from, scrolled_to, session};
use super::*;
use crate::list::layout::Start;
use crate::list::state::End;

/// A session of 100 rows of 50 in a window 300 tall, keeping to its latest row, as a frame left it.
fn at_the_end() -> State {
    let mut state = session(100, 50.);
    state.laid_out(at(94, 0.), None, End::InSight);
    state
}

/// The last row, where it is in the window while the list keeps to it.
const LAST: Anchor = Anchor { row: 99, top: px(250.) };

#[test]
fn a_row_someone_opens_or_closes_something_in_is_kept_still_if_it_is_in_sight() {
    let mut state = scrolled_to(100, 50., 10, 20.);
    let first_in_sight = Anchor { row: 11, top: px(30.) };
    state.laid_out(at(10, 20.), Some(first_in_sight), End::OutOfSight);

    state.keep_still(9);
    let anchored = Start::Anchored { at: first_in_sight, moved: px(0.) };
    assert_eq!(lay_from(&state), anchored, "not a row above the window");
    state.keep_still(17);
    assert_eq!(lay_from(&state), anchored, "nor one below it");
    state.keep_still(13);
    assert_eq!(lay_from(&state), Start::Still { at: Anchor { row: 13, top: px(130.) }, follows: false });
    state.keep_still(10);
    let reaching_up = Anchor { row: 10, top: px(-20.) };
    assert_eq!(lay_from(&state), Start::Still { at: reaching_up, follows: false }, "even past the window's top");
}

#[test]
fn opening_something_above_a_row_at_the_bottom_keeps_what_was_clicked_still() {
    let mut state = kept_at_the_bottom();

    state.keep_still(38);

    assert_eq!(lay_from(&state), Start::Still { at: Anchor { row: 38, top: px(150.) }, follows: false });
}

/// Keeping to the latest row would put its end at the window's bottom, and what was opened would push the line
/// that was clicked up from under the pointer.
#[test]
fn opening_something_in_the_latest_row_keeps_the_row_still_rather_than_its_end_at_the_bottom() {
    let mut state = at_the_end();

    state.keep_still(99);
    assert_eq!(lay_from(&state), Start::Still { at: LAST, follows: false });
    // It opened to more than the window has room for under it.
    state.laid_out(at(94, 0.), Some(LAST), End::OutOfSight);

    assert!(state.is_scrolled_up(), "the end is out of sight, as if someone had scrolled away from it");
    assert_eq!(lay_from(&state), Start::Anchored { at: LAST, moved: px(0.) }, "and it's at a place like any other");
}

#[test]
fn closing_it_again_brings_the_end_back_and_keeps_to_the_latest_row() {
    let mut state = at_the_end();
    state.keep_still(99);
    state.laid_out(at(94, 0.), Some(LAST), End::OutOfSight);

    state.keep_still(99);
    state.laid_out(at(94, 0.), Some(LAST), End::InSight);

    assert_eq!(lay_from(&state), Start::Latest);
    assert!(!state.is_scrolled_up());
}

/// Closing something in the session's last rows leaves it ending above the window's bottom. The rows don't come
/// down to fill that, which would take what was clicked from under the pointer.
#[test]
fn a_row_held_still_with_the_end_above_the_bottom_goes_on_keeping_to_the_latest_row() {
    let mut state = at_the_end();

    state.keep_still(99);
    state.laid_out(at(94, 0.), Some(LAST), End::Short);

    assert_eq!(lay_from(&state), Start::Still { at: LAST, follows: true });
    assert!(!state.is_scrolled_up(), "the end is in sight");
    state.scroll_by(px(-30.));
    assert_eq!(lay_from(&state), Start::Still { at: LAST, follows: true }, "there's nothing further down");

    // More arrives, and takes the end down to the window's bottom.
    state.laid_out(at(94, 0.), Some(LAST), End::InSight);
    assert_eq!(lay_from(&state), Start::Latest);
}

#[test]
fn the_wheel_moves_on_from_a_row_held_still_above_the_bottom() {
    let mut state = at_the_end();
    state.keep_still(99);
    state.laid_out(at(94, 0.), Some(LAST), End::Short);

    state.scroll_by(px(40.));

    assert_eq!(lay_from(&state), Start::Anchored { at: LAST, moved: px(-40.) });
}

/// The session's rows only go at its end, which can take the row that was held still.
#[test]
fn a_list_whose_row_held_still_has_gone_keeps_to_the_latest_row() {
    let mut state = at_the_end();
    state.keep_still(99);
    state.laid_out(at(94, 0.), Some(LAST), End::Short);

    state.set_rows(99, std::iter::once(0..99).collect());

    assert_eq!(lay_from(&state), Start::Latest);
}

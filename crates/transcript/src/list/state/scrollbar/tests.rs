use gpui_kit::px;

use super::super::tests::{at, kept_at_the_bottom, lay_from, scrolled_to, session};
use crate::list::layout::Start;

#[test]
fn the_scrollbar_reads_how_far_the_session_is_scrolled_and_how_tall_it_is() {
    let state = scrolled_to(100, 50., 10, 5.);
    let short = session(3, 50.);

    assert_eq!(state.bar_scrolled(), px(505.));
    assert_eq!(state.bar_tall(), px(5000.));
    assert_eq!(short.bar_tall(), px(300.), "never shorter than its window");
}

#[test]
fn a_drag_of_the_thumb_goes_by_the_heights_as_they_were_when_it_began() {
    let mut state = session(100, 50.);
    state.start_drag();
    state.bar_scroll_to(px(2000.));
    let held = state.top();
    assert_eq!(held, at(40, 0.));

    // Rows load under the thumb and turn out far taller than they were taken to be.
    for row in 0..30 {
        state.measured(row, px(900.));
    }
    state.bar_scroll_to(px(2000.));

    assert_eq!(state.top(), held, "the same place on the track is the same row");
    assert_eq!(state.bar_scrolled(), px(2000.), "and the thumb is where it was put");
    assert_eq!(state.bar_tall(), px(5000.), "on a track as long as it was");

    state.end_drag();
    assert_eq!(state.bar_tall(), px(30. * 900. + 70. * 50.), "which is put right once it's let go");
}

#[test]
fn as_far_down_as_the_scrollbar_goes_keeps_to_the_latest_row_again() {
    let mut dragged = scrolled_to(100, 50., 10, 0.);
    dragged.start_drag();
    dragged.bar_scroll_to(px(4699.));
    assert!(dragged.is_scrolled_up());
    dragged.bar_scroll_to(px(4700.));
    let mut wheeled = scrolled_to(100, 50., 93, 0.);
    wheeled.bar_scroll_to(px(4690.));
    assert!(wheeled.is_scrolled_up());
    wheeled.bar_scroll_to(px(4700.));

    assert!(!dragged.is_scrolled_up());
    // Before the next frame, so what's drawn in it has no button back to the latest row.
    assert!(!wheeled.is_scrolled_up());
    assert_eq!(lay_from(&wheeled), Start::Latest);
}

#[test]
fn anything_else_that_scrolls_the_bar_moves_by_the_difference_as_the_wheel_does() {
    let mut state = scrolled_to(100, 50., 10, 5.);

    state.bar_scroll_to(px(475.));

    assert_eq!(lay_from(&state), Start::Top { at: at(10, 5.), moved: px(-30.) }, "passed row by row at the next frame");
}

#[test]
fn the_scrollbar_reading_back_where_it_is_keeps_a_row_at_the_bottom() {
    let mut state = kept_at_the_bottom();

    state.bar_scroll_to(state.bar_scrolled());

    assert_eq!(lay_from(&state), Start::AtBottom { row: 40 });
}

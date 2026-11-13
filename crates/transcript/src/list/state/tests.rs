use std::time::Instant;

use gpui_kit::{Bounds, Pixels, point, px, size};
use look::motion::Scrolling;

use super::*;

fn window(wide: f32, tall: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(0.), px(0.)), size(px(wide), px(tall)))
}

pub(super) fn at(row: usize, offset: f32) -> Place {
    Place { row, offset: px(offset) }
}

/// A session of `rows` rows, all loaded and each measured at `height`, in a window 300 tall.
pub(super) fn session(rows: usize, height: f32) -> State {
    let mut state = State::new();
    state.set_rows(rows, std::iter::once(0..rows).collect());
    state.drawn_at(window(400., 300.));
    for row in 0..rows {
        state.measured(row, px(height));
    }
    state
}

/// The same, as a frame left it scrolled away from the latest row: at `row`, this far into it.
pub(super) fn scrolled_to(rows: usize, height: f32, row: usize, offset: f32) -> State {
    let mut state = session(rows, height);
    state.go_to_row(row);
    state.laid_out(at(row, offset), None, End::OutOfSight);
    state
}

/// The wheel's travel is walked row by row for up to 900 px.
pub(super) fn lay_from(state: &State) -> Start {
    state.lay_from(px(900.))
}

#[test]
fn it_keeps_to_the_latest_row_until_the_wheel_brings_earlier_rows_back() {
    let mut state = session(100, 50.);
    assert_eq!(lay_from(&state), Start::Latest);
    assert_eq!(state.scrolled(), px(4700.), "as far as it goes");
    assert!(!state.is_scrolled_up());

    state.scroll_by(px(-30.));
    assert_eq!(lay_from(&state), Start::Latest, "there's nothing further down");

    state.scroll_by(px(40.));
    assert!(state.is_scrolled_up());
    assert_eq!(state.scrolled(), px(4660.));
}

#[test]
fn the_wheel_moves_the_top_without_knowing_how_tall_the_rows_it_passes_are() {
    let mut state = scrolled_to(100, 50., 10, 5.);

    state.scroll_by(px(20.));

    // The place it was at and how far it's moved, until the next frame lays the rows out and finds which row that
    // comes to.
    assert_eq!(lay_from(&state), Start::Top { at: at(10, 5.), moved: px(-20.) });
    assert_eq!(state.scrolled(), px(485.));

    state.laid_out(at(9, 35.), None, End::OutOfSight);

    assert_eq!(lay_from(&state), Start::Top { at: at(9, 35.), moved: px(0.) });
    assert_eq!(state.scrolled(), px(485.));
}

#[test]
fn a_frame_lays_the_rows_out_from_the_one_the_last_left_to_keep_still() {
    let mut state = scrolled_to(100, 50., 10, 20.);
    let anchor = Anchor { row: 11, top: px(30.) };
    state.laid_out(at(10, 20.), Some(anchor), End::OutOfSight);
    assert_eq!(lay_from(&state), Start::Anchored { at: anchor, moved: px(0.) });

    state.scroll_by(px(20.));
    assert_eq!(lay_from(&state), Start::Anchored { at: anchor, moved: px(-20.) }, "and what the wheel moved since");

    state.go_to_row(40);
    assert_eq!(lay_from(&state), Start::Top { at: at(40, 0.), moved: px(0.) }, "until something sends it somewhere");
}

/// The same, as a frame left it after sending row 40 to the bottom, with row 35 at the top.
pub(super) fn kept_at_the_bottom() -> State {
    let mut state = scrolled_to(100, 50., 10, 0.);
    state.go_to_row_at_bottom(40);
    state.laid_out(at(35, 0.), Some(Anchor { row: 35, top: px(0.) }), End::OutOfSight);
    state
}

#[test]
fn a_row_sent_to_the_bottom_is_kept_there_frame_after_frame() {
    let mut state = scrolled_to(100, 50., 10, 0.);

    state.go_to_row_at_bottom(40);

    assert_eq!(lay_from(&state), Start::AtBottom { row: 40 });
    assert_eq!(state.kept_at_bottom(), Some(40), "before a frame has laid it out there");
    assert!(state.is_scrolled_up());
    let state = kept_at_the_bottom();
    assert_eq!(lay_from(&state), Start::AtBottom { row: 40 }, "and after, as the rows above it load");
}

/// Something that moves the list, and what it is.
type Move = (&'static str, fn(&mut State));

#[test]
fn anything_else_that_moves_the_list_lets_the_row_at_the_bottom_go() {
    let moves: [Move; 6] = [
        ("the wheel", |state| state.scroll_by(px(20.))),
        ("opening something in sight", |state| state.keep_still(38)),
        ("a drag of the scrollbar's thumb", State::start_drag),
        ("a click on the scrollbar's track", |state| state.bar_scroll_to(px(100.))),
        ("the button back down", |state| state.head_to_latest(Scrolling::since(Instant::now()))),
        ("going to a row", |state| state.go_to_row(10)),
    ];

    for (what, moving) in moves {
        let mut state = kept_at_the_bottom();
        moving(&mut state);
        assert_eq!(state.kept_at_bottom(), None, "{what}");
        assert!(!matches!(lay_from(&state), Start::AtBottom { .. }), "{what}");
    }
}

#[test]
fn a_row_sent_to_the_bottom_has_the_top_where_the_heights_put_it_until_a_frame_lays_it_out() {
    let mut state = scrolled_to(100, 50., 10, 0.);
    let mut tall = scrolled_to(100, 50., 10, 0.);
    tall.measured(40, px(500.));

    state.go_to_row_at_bottom(40);
    tall.go_to_row_at_bottom(40);

    assert_eq!(state.top(), at(35, 0.), "so the wheel moves on from there");
    assert_eq!(tall.top(), at(40, 0.), "or from the start of one taller than the window");
}

#[test]
fn a_row_at_the_bottom_stays_as_rows_arrive_and_goes_once_the_end_is_in_sight() {
    let mut arriving = kept_at_the_bottom();
    let mut at_the_end = kept_at_the_bottom();

    arriving.set_rows(120, std::iter::once(0..120).collect());
    at_the_end.laid_out(at(94, 0.), None, End::InSight);

    assert_eq!(lay_from(&arriving), Start::AtBottom { row: 40 });
    assert_eq!(lay_from(&at_the_end), Start::Latest);
}

#[test]
fn the_wheel_moves_on_from_where_the_last_frame_left_a_row_at_the_bottom() {
    let mut state = kept_at_the_bottom();

    state.scroll_by(px(20.));

    assert_eq!(lay_from(&state), Start::Anchored { at: Anchor { row: 35, top: px(0.) }, moved: px(-20.) });
}

#[test]
fn a_row_past_the_end_isnt_sent_to_the_bottom() {
    let mut state = scrolled_to(100, 50., 10, 0.);

    state.go_to_row_at_bottom(100);

    assert_eq!(state.kept_at_bottom(), None);
    assert_eq!(lay_from(&state), Start::Top { at: at(10, 0.), moved: px(0.) });
}

#[test]
fn a_top_that_has_moved_far_is_found_from_what_the_rows_are_taken_to_come_to() {
    let mut before_the_start = scrolled_to(200, 50., 50, 0.);
    before_the_start.scroll_by(px(5000.));
    let mut far_on = scrolled_to(200, 50., 50, 0.);
    far_on.scroll_by(px(-4000.));
    let mut near = scrolled_to(200, 50., 50, 0.);
    near.scroll_by(px(-800.));

    assert_eq!(lay_from(&before_the_start), Start::Top { at: at(0, 0.), moved: px(0.) });
    assert_eq!(lay_from(&far_on), Start::Top { at: at(130, 0.), moved: px(0.) });
    assert_eq!(lay_from(&near), Start::Top { at: at(50, 0.), moved: px(800.) }, "near enough to pass row by row");
}

#[test]
fn laying_the_rows_out_with_the_end_in_sight_keeps_to_the_latest_row_again() {
    let mut state = scrolled_to(100, 50., 90, 0.);
    assert!(state.is_scrolled_up());

    state.laid_out(at(94, 0.), None, End::InSight);

    assert!(!state.is_scrolled_up());
    assert_eq!(lay_from(&state), Start::Latest);

    // And goes on doing so however the next frame comes out, until someone scrolls away.
    state.laid_out(at(94, 0.), None, End::OutOfSight);
    assert_eq!(lay_from(&state), Start::Latest);
}

#[test]
fn a_rows_height_is_known_once_its_measured_until_it_changes_or_the_list_does() {
    let mut state = State::new();
    state.set_rows(10, vec![2..6, 8..10]);
    state.drawn_at(window(400., 300.));
    assert_eq!(state.kept(3), None, "never measured");

    state.measured(3, px(40.));
    state.measured(7, px(40.));
    assert_eq!(state.kept(3), Some(px(40.)));
    assert_eq!(state.kept(7), None, "not loaded, so it's drawn to find there's nothing to draw");

    state.out_of_date(3..4);
    assert_eq!(state.kept(3), None, "it changed");
    state.measured(3, px(60.));
    assert_eq!(state.kept(3), Some(px(60.)));

    state.drawn_at(window(500., 300.));
    assert_eq!(state.kept(3), None, "the list is another width");
    assert_eq!(state.heights.height_of(3), px(60.), "though it counts as what it was until it's measured again");
}

#[test]
fn a_row_that_showed_nothing_shows_nothing_at_another_width() {
    let mut state = session(4, 50.);
    state.measured(1, px(0.));

    state.drawn_at(window(500., 300.));
    assert_eq!((state.kept(0), state.kept(1)), (None, Some(px(0.))));
    assert_eq!(state.out_of_date_near_the_top(10).len(), 4, "though it's measured again with the rest");

    // Unless it's changed, or every row may have, such as when thinking is shown open.
    state.out_of_date(1..2);
    assert_eq!(state.kept(1), None);
    state.measured(1, px(0.));
    state.all_out_of_date();
    assert_eq!(state.kept(1), None);
}

/// An event of a call shows nothing while its call's row is loaded to draw it, and is drawn where it is once that
/// page is let go. Kept as showing nothing, it would never be drawn again.
#[test]
fn a_page_that_loads_or_is_let_go_puts_every_height_out_of_date() {
    // As the transcript gives them: the pages of history loaded, and the streaming tail's row after them as a run of
    // its own.
    let runs = |history: &[Range<usize>], rows: usize| {
        let mut runs = history.to_vec();
        runs.push(rows - 1..rows);
        runs
    };
    let mut state = State::new();
    state.set_rows(1001, runs(&[0..250, 500..1000], 1001));
    state.drawn_at(window(400., 300.));
    for row in (0..250).chain(500..1001) {
        state.measured(row, px(0.));
    }
    assert_eq!(state.kept(600), Some(px(0.)));

    // Rows come at the end as the agent works, and a prompt that was sent is taken back, which alters no other row.
    state.set_rows(1003, runs(&[0..250, 500..1002], 1003));
    state.set_rows(1002, runs(&[0..250, 500..1001], 1002));
    assert_eq!(state.kept(600), Some(px(0.)));

    state.set_rows(1002, runs(&[0..100, 500..1001], 1002));
    assert_eq!(state.kept(600), None, "a page was let go");
    state.measured(600, px(40.));
    state.set_rows(1002, runs(&[0..100, 250..1001], 1002));
    assert_eq!(state.kept(600), None, "and one loaded");
}

#[test]
fn no_row_out_of_sight_is_measured_in_a_frame_at_another_width() {
    let mut state = session(100, 50.);
    assert_eq!(state.to_measure_out_of_sight(48), (Vec::new(), false), "at rest, there's nothing to measure");

    // A sidebar slides, and each frame is another width.
    state.drawn_at(window(410., 300.));
    assert_eq!(state.to_measure_out_of_sight(48), (Vec::new(), true), "none yet, and a frame is asked for");
    state.drawn_at(window(420., 300.));
    assert_eq!(state.to_measure_out_of_sight(48), (Vec::new(), true));

    // It comes to rest, and the frame that was asked for is at the same width.
    state.drawn_at(window(420., 300.));
    let (rows, more) = state.to_measure_out_of_sight(48);
    assert_eq!((rows.len(), more), (48, true));
    for row in 0..100 {
        state.measured(row, px(60.));
    }
    assert_eq!(state.to_measure_out_of_sight(48), (Vec::new(), false));
}

#[test]
fn the_loaded_rows_out_of_date_are_measured_nearest_the_top_first() {
    let mut state = State::new();
    state.set_rows(10, vec![2..6, 8..10]);
    state.drawn_at(window(400., 300.));
    state.go_to_row(5);

    assert_eq!(state.out_of_date_near_the_top(10), [5, 4, 3, 2, 8, 9], "none measured yet, and none that isn't loaded");
    assert_eq!(state.out_of_date_near_the_top(2), [5, 4]);

    state.measured(5, px(40.));
    state.up_to_date(8);
    assert_eq!(state.out_of_date_near_the_top(10), [4, 3, 2, 9]);
}

#[test]
fn a_row_that_changes_or_a_new_width_puts_heights_out_of_date_but_keeps_them_as_guesses() {
    let mut state = session(6, 50.);
    state.go_to_row(0);
    assert!(state.out_of_date_near_the_top(10).is_empty());

    state.out_of_date(3..5);
    assert_eq!(state.out_of_date_near_the_top(10), [3, 4]);

    state.drawn_at(window(500., 300.));
    assert_eq!(state.out_of_date_near_the_top(10), [0, 1, 2, 3, 4, 5], "every row, at another width");
    assert_eq!(state.heights.total(), px(300.), "each still as tall as it was, until it's measured again");

    state.drawn_at(window(500., 280.));
    state.measured(2, px(60.));
    assert_eq!(state.out_of_date_near_the_top(10), [0, 1, 3, 4, 5], "a taller or shorter window changes none");

    state.all_out_of_date();
    assert_eq!(state.out_of_date_near_the_top(10).len(), 6);
}

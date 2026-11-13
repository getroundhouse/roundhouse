//! A row held still, as [`Start::Still`] says.

use super::*;

/// Row `row` held where its top was `top` below the window's, in the frame of the click that held it.
fn still(row: usize, top: f32) -> Start {
    Start::Still { at: Anchor { row, top: px(top) }, follows: false }
}

/// The same, in the frames after one that left the session's end above the window's bottom.
fn still_following(row: usize, top: f32) -> Start {
    Start::Still { at: Anchor { row, top: px(top) }, follows: true }
}

/// Ten rows of 100 keep to the latest with row 9 at 200. Something closed in it leaves it 40 tall.
#[test]
fn a_row_held_still_stays_where_it_was_though_the_session_then_ends_above_the_bottom() {
    let mut heights = [100.; 10];
    heights[9] = 40.;

    let (layout, _) = Session::of(&heights).from(still(9, 200.));

    assert_eq!(tops(&layout).last(), Some(&(9, px(200.))), "rather than coming down 60 to end at the bottom");
    assert_eq!(layout.end(), End::Short);
    assert_eq!(layout.anchor, Some(Anchor { row: 9, top: px(200.) }), "and it's the row to keep still next");
}

/// Something opened in the same row leaves it 400 tall.
#[test]
fn a_row_held_still_stays_where_it_was_though_it_then_ends_below_the_bottom() {
    let mut heights = [100.; 10];
    heights[9] = 400.;

    let (layout, _) = Session::of(&heights).from(still(9, 200.));

    assert_eq!(tops(&layout).last(), Some(&(9, px(200.))));
    assert_eq!(layout.end(), End::OutOfSight);
}

#[test]
fn a_row_held_still_above_the_bottom_stays_there_while_what_arrives_leaves_the_end_above_it() {
    let mut heights = [100.; 10];
    heights[9] = 70.;

    let (layout, _) = Session::of(&heights).from(still_following(9, 200.));

    assert_eq!(tops(&layout).last(), Some(&(9, px(200.))));
    assert_eq!(layout.end(), End::Short);
}

#[test]
fn once_what_arrives_takes_the_end_past_the_bottom_it_goes_there_and_the_rows_move_up() {
    let mut heights = [100.; 10];
    heights[9] = 130.;

    let (layout, _) = Session::of(&heights).from(still_following(9, 200.));

    assert_eq!(tops(&layout).last(), Some(&(9, px(170.))), "its end at the window's bottom");
    assert_eq!(layout.end(), End::InSight);
}

/// More rows than are laid out past the window's bottom can arrive at once, and the end is found all the same.
#[test]
fn the_end_goes_to_the_bottom_however_far_past_it_what_arrived_took_it() {
    let mut heights = vec![100.; 10];
    heights[9] = 40.;
    heights.extend([300.; 5]);

    let (layout, _) = Session::of(&heights).from(still_following(9, 200.));

    assert_eq!(tops(&layout).last(), Some(&(14, px(0.))), "the latest row, ending at the window's bottom");
    assert_eq!(layout.end(), End::InSight);
}

/// A session that doesn't fill the window sits at its top, with nothing above to have come down.
#[test]
fn a_session_shorter_than_the_window_ends_in_sight_rather_than_short() {
    let (layout, _) = Session::of(&[100., 50.]).from(still(1, 100.));

    assert_eq!(tops(&layout), [(0, px(0.)), (1, px(100.))]);
    assert_eq!(layout.end(), End::InSight);
}

//! The list's own rule: each frame moves it by all but the share of what it finds is left that the scroll says is
//! still to go. How that share goes down over time is `look::motion`'s, and tested there, so these give the share.

use std::time::Instant;

use gpui_kit::px;
use look::motion::Scrolling;

use super::super::tests::{at, scrolled_to};
use super::*;
use crate::list::layout::Start;

/// Sends it down to the latest row.
fn set_off(state: &mut State) {
    state.head_to_latest(Scrolling::since(Instant::now()));
}

/// Draws a frame, as far as moving it on goes, with `share` of what's left still to go after it.
fn frame(state: &mut State, share: f32) {
    state.head_on(|_| share);
}

/// How far the top of what's shown has still to go to the latest row.
fn to_go(state: &State) -> f32 {
    (state.furthest() - state.scrolled()).as_f32()
}

#[test]
fn each_frame_leaves_the_share_still_to_go_of_what_was_left_and_none_keeps_to_the_latest_row() {
    // 200 above the latest row, in a window 300 tall.
    let mut state = scrolled_to(100, 50., 90, 0.);

    set_off(&mut state);
    assert!(state.is_heading_to_latest());
    assert!(state.is_scrolled_up(), "it's still away from the latest row");
    frame(&mut state, 1.);
    assert!((to_go(&state) - 200.).abs() < 0.01, "it hasn't moved as it sets off");
    frame(&mut state, 0.5);
    assert!((to_go(&state) - 100.).abs() < 0.01, "{} to go", to_go(&state));
    frame(&mut state, 0.25);
    assert!((to_go(&state) - 25.).abs() < 0.01, "{} to go", to_go(&state));

    frame(&mut state, 0.);
    assert!(!state.is_heading_to_latest() && !state.is_scrolled_up());
    assert_eq!(state.lay_from(px(900.)), Start::Latest);
}

#[test]
fn rows_that_arrive_on_the_way_are_slid_down_to_with_the_rest() {
    let mut state = scrolled_to(100, 50., 90, 0.);
    set_off(&mut state);
    frame(&mut state, 0.25);
    assert!((to_go(&state) - 50.).abs() < 0.01);

    state.set_rows(104, std::iter::once(0..104).collect());
    for row in 100..104 {
        state.measured(row, px(50.));
    }
    frame(&mut state, 0.25);

    // A quarter of what it finds is left, the 200 that arrived with the rest, rather than all of it at once.
    assert!((to_go(&state) - 62.5).abs() < 0.01, "{} to go", to_go(&state));
}

#[test]
fn from_more_than_two_windows_up_it_goes_straight_to_a_window_above_the_latest_row_and_slides_from_there() {
    // In a window 300 tall: a window and a half up, two exactly, a little over two, and far up.
    let mut near = scrolled_to(100, 50., 85, 0.);
    let mut two = scrolled_to(100, 50., 82, 0.);
    let mut just_past = scrolled_to(100, 50., 81, 0.);
    let mut far = scrolled_to(100, 50., 10, 0.);

    for state in [&mut near, &mut two, &mut just_past, &mut far] {
        set_off(state);
        assert!(state.is_heading_to_latest());
    }

    assert!((to_go(&near) - 450.).abs() < 0.01, "it slides all the way from {}", to_go(&near));
    assert!((to_go(&two) - 600.).abs() < 0.01, "and from two windows up, {}", to_go(&two));
    // A jump of more than a window, so it doesn't look like a stumble.
    assert!((to_go(&just_past) - 300.).abs() < 0.01, "{} to go, from 650", to_go(&just_past));
    assert!((to_go(&far) - 300.).abs() < 0.01, "{} to go", to_go(&far));
}

#[test]
fn the_wheel_a_drag_or_a_jump_stops_it_where_it_is() {
    let halfway = || {
        let mut state = scrolled_to(100, 50., 90, 0.);
        set_off(&mut state);
        frame(&mut state, 0.5);
        state
    };
    let (mut wheeled, mut dragged, mut jumped) = (halfway(), halfway(), halfway());

    wheeled.scroll_by(px(10.));
    dragged.start_drag();
    jumped.go_to_row(20);

    for state in [&mut wheeled, &mut dragged, &mut jumped] {
        frame(state, 0.);
        assert!(!state.is_heading_to_latest() && state.is_scrolled_up());
    }
    assert!((to_go(&wheeled) - 110.).abs() < 0.01, "the wheel goes on from where it had got to");
    assert!((to_go(&dragged) - 100.).abs() < 0.01);
    assert_eq!(jumped.lay_from(px(900.)), Start::Top { at: at(20, 0.), moved: px(0.) });
}

#[test]
fn it_keeps_to_the_latest_row_once_that_ends_in_sight_before_none_of_the_way_is_left() {
    let mut state = scrolled_to(100, 50., 90, 0.);
    set_off(&mut state);

    // The rows below turned out shorter than they were taken to be.
    state.laid_out(at(94, 0.), None, true);

    assert!(!state.is_heading_to_latest());
    assert_eq!(state.lay_from(px(900.)), Start::Latest);
}

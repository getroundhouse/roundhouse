//! Going back down to a long session's latest row, by the wheel and by the button that comes up once it's scrolled
//! away from.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, ScrollDelta, TestAppContext, point, px};
use look::motion::SCROLL;

use super::{A_FRAME, open_on_a_long_session, thumb};
use crate::support::{Stack, Ui};

/// Someone who scrolls back down to the latest row is keeping to it again, and the button that would take them
/// there goes. The list finds the end in sight as it lays the rows out, which is after the button is drawn.
#[gpui_kit::test]
fn scrolling_back_down_to_the_latest_row_takes_away_the_button_to_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_on_a_long_session(cx, &stack);
    assert!(!shows(&mut ui), "there's no button at the latest row");

    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));
    assert!(shows(&mut ui));

    // Not quite as far as the scrollbar goes, which is near enough that the latest row's end is in sight.
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(-299.5))));
    assert!(!shows(&mut ui), "the button is still there with the latest row's end in sight");
}

/// The button slides the chat down to the latest row, slowing as it arrives, and goes as soon as it's clicked, so
/// it isn't pressed again on the way.
#[gpui_kit::test]
fn the_button_slides_the_chat_down_to_the_latest_row(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_on_a_long_session(cx, &stack);
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));
    assert!(shows(&mut ui));
    ui.allow_motion();
    let away = 1. - thumb(&ui);

    ui.click(button(&ui));

    assert!(ui.disabled(button(&ui)), "it's on its way out");
    assert!((1. - thumb(&ui) - away).abs() < f32::EPSILON, "nothing has moved yet");
    ui.play(SCROLL / 2);
    let left = (1. - thumb(&ui)) / away;
    assert!((left - 0.125).abs() < 0.01, "an eighth of the way is left halfway through, not {left}");
    ui.settle(A_FRAME);
    assert!((thumb(&ui) - 1.).abs() < f32::EPSILON, "it's at the latest row, not at {}", thumb(&ui));
    assert!(!shows(&mut ui));
    assert_eq!(ui.play(A_FRAME), 0, "and nothing asks for another frame");
}

/// Under Reduce Motion the button goes straight to the latest row, in the frame its click draws.
#[gpui_kit::test]
fn reduce_motion_takes_the_button_straight_to_the_latest_row(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_on_a_long_session(cx, &stack);
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));
    assert!(shows(&mut ui));

    ui.click(button(&ui));

    assert!((thumb(&ui) - 1.).abs() < f32::EPSILON, "it's at the latest row, not at {}", thumb(&ui));
    assert!(!shows(&mut ui));
}

/// Reduce Motion turned on partway through the scroll takes it the rest of the way in the next frame.
#[gpui_kit::test]
fn reduce_motion_turned_on_partway_takes_the_scroll_the_rest_of_the_way_at_once(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_on_a_long_session(cx, &stack);
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));
    assert!(shows(&mut ui));
    ui.allow_motion();
    ui.click(button(&ui));
    ui.play(SCROLL / 4);
    assert!(thumb(&ui) < 1., "it's partway");

    ui.cx.update(|cx| cx.set_reduce_motion(true));
    ui.play(A_FRAME);

    assert!((thumb(&ui) - 1.).abs() < f32::EPSILON, "it's at the latest row, not at {}", thumb(&ui));
    assert!(!shows(&mut ui));
}

/// The button back down to the latest row of the session shown.
fn button(ui: &Ui<'_>) -> ElementId {
    let session = ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.session_id().to_string());
    ElementId::from((ElementId::Name(session.into()), "jump-to-latest"))
}

/// Whether the button shows, once everything has come to rest.
fn shows(ui: &mut Ui<'_>) -> bool {
    ui.settle(A_FRAME);
    let button = button(ui);
    ui.cx.update_window(ui.handle, |_, window, _| window.try_find(button).is_some()).unwrap()
}

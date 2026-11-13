//! Going back down to a long session's latest row, by the wheel and by the button that comes up once it's scrolled
//! away from.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, ScrollDelta, TestAppContext, point, px};
use look::Region;
use look::motion::SCROLL;

use super::{A_FRAME, open_on_a_long_session, settle_at_the_end, thumb};
use crate::support::{Stack, Ui};
use crate::tools::in_a_new_workspace;

/// The fake agent's commands come with no description, so their line says they run something.
const COMMAND_LINE: &str = "Run";
/// The call the fake agent's subagent makes.
const SUBAGENT_CALL: &str = "List what's here";

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

/// The button sits at the chat's bottom left, on the composer's left edge, which reaches a little past the column's,
/// so it's over the edge of the chat, and it rests 8px above the composer. In a pane too narrow for the composer to be
/// centred, the composer keeps a margin from the pane's edge, which the button could match without following the
/// composer, so the button is checked again in a pane the composer is centred in, with the diff hidden.
#[gpui_kit::test]
fn the_button_sits_on_the_composers_left_edge_just_above_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_on_a_long_session(cx, &stack);
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));
    assert!(shows(&mut ui));
    on_the_composers_edge_just_above_it(&mut ui);

    ui.click("toggle-diff");

    assert!(shows(&mut ui), "it's still scrolled up");
    let (pane, composer) = (ui.bounds(Region::Transcript.id()), ui.bounds(Region::Composer.id()));
    assert!(composer.left() - pane.left() > px(16.), "the composer isn't centred, at {:?}", composer.left());
    on_the_composers_edge_just_above_it(&mut ui);
}

/// The button takes the pointer from what's under it, so a click on it over a tool call's line, which a narrow pane
/// has under it, doesn't open the call too.
#[gpui_kit::test]
fn a_click_on_the_button_leaves_the_call_under_it_closed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_on_a_session_of_commands(cx, &stack);
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));
    assert!(shows(&mut ui));
    let middle = ui.bounds(Region::LatestButton.id()).center();
    let lines = ui.labelled(COMMAND_LINE);
    let under = lines.iter().find(|line| line.bounds().contains(&middle)).expect("a call's line under the button");
    let line = under.path().last().unwrap().clone();

    ui.click(button(&ui));

    let open = ui.cx.update_window(ui.handle, |_, window, _| window.try_find(line).and_then(|l| l.expanded())).unwrap();
    assert_eq!(open, Some(false), "the call under the button opened");
}

/// A subagent's tab has nothing under it, since a subagent can't be prompted, so there the button rests 8px above the
/// tab's bottom edge, as it rests above a chat's composer.
#[gpui_kit::test]
fn in_a_subagent_s_tab_the_button_rests_above_the_tab_s_bottom_edge(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    ui.input("agent-calls:60:Look around; say:done");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    ui.wait("the button that opens the subagent tab", |ui| !ui.shown("open-subagent/").is_empty());
    let open = ui.shown("open-subagent/").remove(0);
    ui.click(open);
    ui.wait("the subagent's work", |ui| ui.labelled(SUBAGENT_CALL).len() > 10);
    ui.settle(A_FRAME);

    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));

    ui.settle(A_FRAME);
    let (tab, button) = (ui.bounds(Region::Transcript.id()), ui.bounds(Region::LatestButton.id()));
    let room = tab.bottom() - button.bottom();
    assert!((room - px(8.)).abs() < px(0.5), "it rests {room:?} above the tab's bottom edge, not 8px");
}

/// Checks that the button is on the composer's left edge, and 8px above it.
fn on_the_composers_edge_just_above_it(ui: &mut Ui<'_>) {
    let (button, composer) = (ui.bounds(Region::LatestButton.id()), ui.bounds(Region::Composer.id()));
    assert!(
        (button.left() - composer.left()).abs() < px(0.5),
        "it's at {:?}, not on the composer's edge at {:?}",
        button.left(),
        composer.left()
    );
    let room = composer.top() - button.bottom();
    assert!((room - px(8.)).abs() < px(0.5), "it rests {room:?} above the composer, not 8px");
}

/// A session of sixty commands, open in the app at its end, so a call's line can be scrolled under the button.
fn open_on_a_session_of_commands<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let session = stack.chat_in_a_new_workspace();
    // A few to a prompt, as a person would ask.
    for _ in 0..3 {
        stack.run(&session, &["tool:echo signal"; 20].join("; "));
    }
    let mut ui = Ui::open(cx, stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    settle_at_the_end(&mut ui, stack, &session);
    ui
}

/// The wheel over the button scrolls the chat under it, as it does anywhere else in it.
#[gpui_kit::test]
fn the_wheel_over_the_button_scrolls_the_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_on_a_long_session(cx, &stack);
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));
    assert!(shows(&mut ui));
    let before = thumb(&ui);

    ui.scroll(button(&ui), ScrollDelta::Pixels(point(px(0.), px(300.))));
    ui.settle(A_FRAME);

    assert!(thumb(&ui) < before, "it went on up, to {} from {before}", thumb(&ui));
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

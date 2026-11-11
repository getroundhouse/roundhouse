//! The wheel over what a call shows. Its block scrolls once it's too tall, until it reaches an end and the wheel goes
//! on to the transcript. A scroll stays with what it started on, and a fling stops at the block's end. A block of a
//! file's lines scrolls sideways along a line too long for it, and a swipe goes one way, the way it started. The diff
//! an edit shows while it's hovered scrolls both ways too, and keeps the wheel from the transcript.

use std::time::Duration;

use gpui_kit::{
    AppContext as _, ElementId, InputEvent as _, Modifiers, MouseMoveEvent, Pixels, Point, ScrollDelta, ScrollEvent,
    TestAppContext, TouchPhase, point, px,
};
use transcript::Place;

use super::{COMMAND_LINE, bounds, in_a_new_workspace, open_a_call, open_a_command};
use crate::support::{Stack, Ui};

/// Longer than a mouse wheel's notches come apart in one spin, so the next notch starts a new gesture.
const A_PAUSE: Duration = Duration::from_secs(1);
/// Characters in a line far too long for any block to show whole.
const LONG_LINE: usize = 400;

#[gpui_kit::test]
fn a_call_s_output_scrolls_under_the_wheel_and_leaves_the_transcript(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (_, output) = open_a_command(&mut ui, "seq 1 20");
    let (lines, transcript) = (top(&mut ui, output.clone()), scrolled_to(&ui));

    wheel(&mut ui, &over(&output), -100.);
    wheel(&mut ui, &over(&output), 40.);

    assert_eq!(scrolled_to(&ui), transcript, "the wheel over the output leaves the transcript under it");
    assert_eq!(top(&mut ui, output), lines - px(60.), "and scrolls the output");
}

#[gpui_kit::test]
fn at_either_end_of_its_output_the_wheel_goes_on_to_the_transcript(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, output) = open_a_command(&mut ui, "seq 1 20");
    // How far the output has scrolled in its block, which the transcript moving doesn't change.
    let scrolled_in = |ui: &mut Ui<'_>| top(ui, command.clone()) - top(ui, output.clone());
    let (start, transcript) = (scrolled_in(&mut ui), scrolled_to(&ui));

    wheel(&mut ui, &over(&output), 40.);

    assert_ne!(scrolled_to(&ui), transcript, "at the top of the output, the wheel up scrolls the transcript");
    assert_eq!(scrolled_in(&mut ui), start, "and leaves the output at its top");

    let transcript = scrolled_to(&ui);
    wheel(&mut ui, &over(&output), -1000.);
    let end = scrolled_in(&mut ui);
    assert!(end > start, "the wheel down takes the output to its end first");
    assert_eq!(scrolled_to(&ui), transcript, "without the transcript");

    keep_turning(&mut ui, &over(&output), -40.);

    assert_ne!(scrolled_to(&ui), transcript, "at the end of the output, the wheel down scrolls the transcript");
    assert_eq!(scrolled_in(&mut ui), end, "and leaves the output at its end");

    let transcript = scrolled_to(&ui);
    wheel(&mut ui, &over(&output), 40.);

    assert_eq!(scrolled_in(&mut ui), end - px(40.), "back up, the wheel scrolls the output again");
    assert_eq!(scrolled_to(&ui), transcript, "and leaves the transcript");
}

#[gpui_kit::test]
fn a_fling_of_a_call_s_output_stops_at_its_end(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, output) = open_a_command(&mut ui, "seq 1 20");
    let scrolled_in = |ui: &mut Ui<'_>| top(ui, command.clone()) - top(ui, output.clone());
    let top_of_output = scrolled_in(&mut ui);
    // Down to the end of the output, so a swipe up has the output to scroll and the transcript to carry on into.
    wheel(&mut ui, &over(&output), -1000.);
    swipe(&mut ui, &over(&output), TouchPhase::Started, 40.);
    swipe(&mut ui, &over(&output), TouchPhase::Moved, 1000.);
    swipe(&mut ui, &over(&output), TouchPhase::Ended, 0.);
    let transcript = scrolled_to(&ui);
    assert_eq!(scrolled_in(&mut ui), top_of_output, "the swipe takes the output back to its top");

    swipe(&mut ui, &over(&output), TouchPhase::Moved, 60.);
    swipe(&mut ui, &over(&output), TouchPhase::Moved, 60.);

    assert_eq!(scrolled_to(&ui), transcript, "the fling it leaves stops there, without carrying the transcript on");
    assert_eq!(scrolled_in(&mut ui), top_of_output);

    swipe(&mut ui, &over(&output), TouchPhase::Started, 40.);

    assert_ne!(scrolled_to(&ui), transcript, "touching again, the swipe scrolls the transcript");
}

#[gpui_kit::test]
fn a_fling_of_the_transcript_passes_over_a_call_s_output(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, output) = open_a_command(&mut ui, "seq 1 20");
    let scrolled_in = |ui: &mut Ui<'_>| top(ui, command.clone()) - top(ui, output.clone());
    // Partway down the output, so it has room to catch a fling up if it would.
    wheel(&mut ui, &over(&output), -60.);
    let line = ui.labelled(COMMAND_LINE)[0].path().last().unwrap().clone();
    swipe(&mut ui, &line, TouchPhase::Started, 40.);
    swipe(&mut ui, &line, TouchPhase::Ended, 0.);
    let (inside, transcript) = (scrolled_in(&mut ui), scrolled_to(&ui));

    swipe(&mut ui, &over(&output), TouchPhase::Moved, 30.);

    assert_ne!(scrolled_to(&ui), transcript, "the transcript's fling carries on over the output");
    assert_eq!(scrolled_in(&mut ui), inside, "and leaves the output where it was");
}

#[gpui_kit::test]
fn a_swipe_that_starts_on_the_transcript_carries_on_over_a_call_s_output(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, output) = open_a_command(&mut ui, "seq 1 20");
    let scrolled_in = |ui: &mut Ui<'_>| top(ui, command.clone()) - top(ui, output.clone());
    // Partway down the output, so it has room to take the swipe up if it would.
    wheel(&mut ui, &over(&output), -60.);
    let line = ui.labelled(COMMAND_LINE)[0].path().last().unwrap().clone();
    swipe(&mut ui, &line, TouchPhase::Started, 40.);
    let (inside, transcript) = (scrolled_in(&mut ui), scrolled_to(&ui));

    swipe(&mut ui, &over(&output), TouchPhase::Moved, 30.);

    assert_ne!(scrolled_to(&ui), transcript, "the fingers go on scrolling the transcript over the output");
    assert_eq!(scrolled_in(&mut ui), inside, "and leave the output where it was");
}

#[gpui_kit::test]
fn a_spin_of_the_wheel_that_starts_on_the_transcript_carries_on_over_a_call_s_output(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, output) = open_a_command(&mut ui, "seq 1 20");
    let scrolled_in = |ui: &mut Ui<'_>| top(ui, command.clone()) - top(ui, output.clone());
    wheel(&mut ui, &over(&output), -60.);
    let line = ui.labelled(COMMAND_LINE)[0].path().last().unwrap().clone();
    wheel(&mut ui, &line, 40.);
    let (inside, transcript) = (scrolled_in(&mut ui), scrolled_to(&ui));

    keep_turning(&mut ui, &over(&output), 30.);

    assert_ne!(scrolled_to(&ui), transcript, "the spin goes on scrolling the transcript over the output");
    assert_eq!(scrolled_in(&mut ui), inside, "and leaves the output where it was");

    let transcript = scrolled_to(&ui);
    wheel(&mut ui, &over(&output), 30.);

    assert_eq!(scrolled_in(&mut ui), inside - px(30.), "after a pause, the wheel scrolls the output");
    assert_eq!(scrolled_to(&ui), transcript);
}

#[gpui_kit::test]
fn the_wheel_over_output_that_fits_scrolls_the_transcript(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (_, output) = open_a_command(&mut ui, "echo short");
    let transcript = scrolled_to(&ui);

    wheel(&mut ui, &over(&output), 40.);

    assert_ne!(scrolled_to(&ui), transcript, "a block with nothing to scroll leaves the wheel to the transcript");
}

#[gpui_kit::test]
fn a_line_too_long_for_its_block_scrolls_sideways_to_its_end(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    open_a_call(&mut ui, &format!("edit:20:{LONG_LINE}"), "Edit");
    let lines = ui.shown("file/").remove(0);
    let (start, transcript) = (bounds(&mut ui, lines.clone()).origin, scrolled_to(&ui));
    let at = bounds(&mut ui, over(&lines)).center();

    turn_at(&mut ui, at, point(px(-100.), px(0.)));

    assert_eq!(bounds(&mut ui, lines.clone()).origin, start - point(px(100.), px(0.)), "a sideways swipe moves along");
    assert_eq!(scrolled_to(&ui), transcript, "and leaves the transcript");

    turn_at(&mut ui, at, point(px(0.), px(-40.)));

    assert_eq!(bounds(&mut ui, lines.clone()).origin, start - point(px(100.), px(40.)), "the wheel down still scrolls");
    assert_eq!(scrolled_to(&ui), transcript, "the block, and not the transcript");

    keep_turning_at(&mut ui, at, point(px(30.), px(-4.)));

    assert_eq!(bounds(&mut ui, lines.clone()).origin, start - point(px(100.), px(44.)), "a wobble doesn't drift it");

    turn_at(&mut ui, at, point(px(-100_000.), px(0.)));
    let (end, edge) = (bounds(&mut ui, lines.clone()).right(), bounds(&mut ui, over(&lines)).right());
    assert!(
        (end - edge).abs() < px(1.),
        "it goes to the end of the longest line: {end:?}, at the block's edge {edge:?}"
    );

    turn_at(&mut ui, at, point(px(-100.), px(0.)));

    assert_eq!(bounds(&mut ui, lines).right(), end, "and stops there");
}

#[gpui_kit::test]
fn a_block_of_long_lines_moves_sideways_only_for_a_gesture_that_went_to_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    // Its two lines fit, so the block has nothing to scroll down.
    open_a_call(&mut ui, &format!("edit:1:{LONG_LINE}"), "Edit");
    let (lines, line) = (ui.shown("file/").remove(0), ui.labelled("Edit")[0].path().last().unwrap().clone());
    let (at, transcript) = (bounds(&mut ui, over(&lines)).center(), scrolled_to(&ui));

    turn_at(&mut ui, at, point(px(0.), px(40.)));

    assert_ne!(scrolled_to(&ui), transcript, "the wheel up over a block that fits scrolls the transcript");

    wheel(&mut ui, &line, 40.);
    let (left, at) = (bounds(&mut ui, lines.clone()).left(), bounds(&mut ui, over(&lines)).center());
    keep_turning_at(&mut ui, at, point(px(-100.), px(-5.)));

    assert_eq!(bounds(&mut ui, lines).left(), left, "a spin that started on the transcript doesn't move it sideways");
}

#[gpui_kit::test]
fn an_edit_s_diff_stays_while_the_pointer_is_on_it_and_scrolls(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    // Enough words first that the transcript itself could scroll, then a diff longer and wider than its preview.
    ui.input(&format!("burst:3000; edit:20:{LONG_LINE}; say:done"));
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    let edit = ui.labelled("Edit")[0].path().last().unwrap().clone();
    ui.hover(edit);
    ui.wait_out_a_tooltip();
    assert!(ui.visible("edit-preview"), "hovering the edit shows its diff");

    ui.hover("edit-preview");
    ui.wait_out_a_tooltip();

    assert!(ui.visible("edit-preview"), "the diff stays while the pointer is on it");
    let (lines, transcript) = (bounds(&mut ui, "edit-preview-lines").origin, scrolled_to(&ui));
    let over = bounds(&mut ui, "edit-preview").center();
    turn_at(&mut ui, over, point(px(0.), px(-100.)));
    keep_turning_at(&mut ui, over, point(px(0.), px(40.)));
    assert_eq!(scrolled_to(&ui), transcript, "the wheel over the diff leaves the transcript under it");
    assert_eq!(bounds(&mut ui, "edit-preview-lines").origin, lines - point(px(0.), px(60.)), "and scrolls the diff");
    turn_at(&mut ui, over, point(px(-100.), px(0.)));
    assert_eq!(bounds(&mut ui, "edit-preview-lines").origin, lines - point(px(100.), px(60.)), "and sideways");

    ui.hover("new-chat");
    ui.wait_out_a_tooltip();

    assert!(!ui.visible("edit-preview"), "the diff goes once the pointer leaves it");
}

/// The area that shows the block part with this ID, such as a command's output or a file's lines, so the pointer is
/// over what can be seen of it however tall or wide it is.
fn over(output: &ElementId) -> ElementId {
    ElementId::from((ElementId::from((output.clone(), "scroll")), "area"))
}

/// Turns the wheel over the element by `y`, down the page when it's negative.
fn wheel(ui: &mut Ui<'_>, id: &ElementId, y: f32) {
    ui.cx.executor().advance_clock(A_PAUSE);
    keep_turning(ui, id, y);
}

/// Turns the wheel on over the element by `y`, in the same spin as the last turn.
fn keep_turning(ui: &mut Ui<'_>, id: &ElementId, y: f32) {
    ui.scroll(id.clone(), ScrollDelta::Pixels(point(px(0.), px(y))));
}

/// Moves the pointer onto the element, and swipes by `y` on a trackpad at `phase` of a gesture: the fingers touching,
/// moving, lifting, or the fling they leave, which GPUI calls moving too.
fn swipe(ui: &mut Ui<'_>, id: &ElementId, phase: TouchPhase, y: f32) {
    ui.hover(id.clone());
    let position = bounds(ui, id.clone()).center();
    send(
        ui,
        ScrollEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(0.), px(y))),
            touch_phase: phase,
            ..ScrollEvent::default()
        },
    );
}

/// Moves the pointer to `at`, and turns the wheel there by `delta` after a pause, so it's a gesture of its own. A wide
/// element's middle can be far outside what shows of it, so the pointer goes where it's told. It's there before the
/// pause, so what was under it before, such as an edit's line, doesn't open a preview that covers `at`.
fn turn_at(ui: &mut Ui<'_>, at: Point<Pixels>, delta: Point<Pixels>) {
    let moved = MouseMoveEvent { position: at, pressed_button: None, modifiers: Modifiers::default() };
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(moved.to_platform_input(), cx)).unwrap();
    ui.frame();
    ui.cx.executor().advance_clock(A_PAUSE);
    keep_turning_at(ui, at, delta);
}

/// Turns the wheel on at `at` by `delta`, in the same spin as the last turn.
fn keep_turning_at(ui: &mut Ui<'_>, at: Point<Pixels>, delta: Point<Pixels>) {
    send(ui, ScrollEvent { position: at, delta: ScrollDelta::Pixels(delta), ..ScrollEvent::default() });
}

fn send(ui: &mut Ui<'_>, event: ScrollEvent) {
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(event.to_platform_input(), cx)).unwrap();
    ui.frame();
}

/// Where the top of the element is drawn.
fn top(ui: &mut Ui<'_>, id: impl Into<ElementId>) -> Pixels {
    bounds(ui, id).top()
}

/// How far the transcript is scrolled.
fn scrolled_to(ui: &Ui<'_>) -> Place {
    ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.list().top())
}

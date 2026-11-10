//! Tool calls in the transcript: each is one line, which opens to what the call did and closes again, in a block that
//! the wheel scrolls once it's too tall, until it reaches an end and the wheel goes on to the transcript. A scroll
//! stays with what it started on, and a fling stops at the block's end. A call that started a subagent opens to the
//! subagent's work, which arrives after it. An edit shows its diff while it's hovered, and a read of an image opens to
//! the image.

#![cfg(test)]

use std::io::Cursor;
use std::time::Duration;

use gpui_kit::test::{ElementSnapshot, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, ElementId, InputEvent as _, ListOffset, Pixels, ScrollDelta, ScrollEvent, TestAppContext,
    TouchPhase, point, px,
};
use image::ImageFormat;
use protocol::event::EventBody;

use crate::support::{Stack, Ui};

/// The fake agent's commands come with no description, so their line says they run something.
const COMMAND_LINE: &str = "Run";
/// What the fake agent says its subagent is for, and the description of the call the subagent makes.
const AGENT_LINE: &str = "Look around";
const SUBAGENT_CALL: &str = "List what's here";
/// Asking the advisor, which the API runs rather than the agent.
const ADVISOR_LINE: &str = "Ask the advisor";
/// Longer than a mouse wheel's notches come apart in one spin, so the next notch starts a new gesture.
const A_PAUSE: Duration = Duration::from_secs(1);

#[gpui_kit::test]
fn a_call_opens_to_what_it_did_and_closes_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    ui.input("tool:echo printed-by-the-tool; say:done");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    assert_eq!(command_open(&mut ui), Some(false), "a call starts closed");
    assert!(ui.shown("output/").is_empty());

    ui.click_labelled(COMMAND_LINE, 0);

    assert_eq!(command_open(&mut ui), Some(true));
    assert_eq!(ui.shown("command/").len(), 1, "the command");
    assert_eq!(ui.shown("output/").len(), 1, "and what it printed");

    ui.click_labelled(COMMAND_LINE, 0);

    assert_eq!(command_open(&mut ui), Some(false));
    assert!(ui.shown("output/").is_empty());
}

#[gpui_kit::test]
fn a_call_s_output_scrolls_under_the_wheel_and_leaves_the_transcript(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    let (_, output) = open_a_command(&mut ui, "seq 1 20");
    let (lines, transcript) = (top(&mut ui, output.clone()), scrolled_to(&ui));

    wheel(&mut ui, &output, -100.);
    wheel(&mut ui, &output, 40.);

    assert_eq!(scrolled_to(&ui), transcript, "the wheel over the output leaves the transcript under it");
    assert_eq!(top(&mut ui, output), lines - px(60.), "and scrolls the output");
}

#[gpui_kit::test]
fn at_either_end_of_its_output_the_wheel_goes_on_to_the_transcript(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    let (command, output) = open_a_command(&mut ui, "seq 1 20");
    // How far the output has scrolled in its block, which the transcript moving doesn't change.
    let scrolled_in = |ui: &mut Ui<'_>| top(ui, command.clone()) - top(ui, output.clone());
    let (start, transcript) = (scrolled_in(&mut ui), scrolled_to(&ui));

    wheel(&mut ui, &output, 40.);

    assert_ne!(scrolled_to(&ui), transcript, "at the top of the output, the wheel up scrolls the transcript");
    assert_eq!(scrolled_in(&mut ui), start, "and leaves the output at its top");

    let transcript = scrolled_to(&ui);
    wheel(&mut ui, &output, -1000.);
    let end = scrolled_in(&mut ui);
    assert!(end > start, "the wheel down takes the output to its end first");
    assert_eq!(scrolled_to(&ui), transcript, "without the transcript");

    keep_turning(&mut ui, &output, -40.);

    assert_ne!(scrolled_to(&ui), transcript, "at the end of the output, the wheel down scrolls the transcript");
    assert_eq!(scrolled_in(&mut ui), end, "and leaves the output at its end");

    let transcript = scrolled_to(&ui);
    wheel(&mut ui, &output, 40.);

    assert_eq!(scrolled_in(&mut ui), end - px(40.), "back up, the wheel scrolls the output again");
    assert_eq!(scrolled_to(&ui), transcript, "and leaves the transcript");
}

#[gpui_kit::test]
fn a_fling_of_a_call_s_output_stops_at_its_end(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    let (command, output) = open_a_command(&mut ui, "seq 1 20");
    let scrolled_in = |ui: &mut Ui<'_>| top(ui, command.clone()) - top(ui, output.clone());
    let top_of_output = scrolled_in(&mut ui);
    // Down to the end of the output, so a swipe up has the output to scroll and the transcript to carry on into.
    wheel(&mut ui, &output, -1000.);
    swipe(&mut ui, &output, TouchPhase::Started, 40.);
    swipe(&mut ui, &output, TouchPhase::Moved, 1000.);
    swipe(&mut ui, &output, TouchPhase::Ended, 0.);
    let transcript = scrolled_to(&ui);
    assert_eq!(scrolled_in(&mut ui), top_of_output, "the swipe takes the output back to its top");

    swipe(&mut ui, &output, TouchPhase::Moved, 60.);
    swipe(&mut ui, &output, TouchPhase::Moved, 60.);

    assert_eq!(scrolled_to(&ui), transcript, "the fling it leaves stops there, without carrying the transcript on");
    assert_eq!(scrolled_in(&mut ui), top_of_output);

    swipe(&mut ui, &output, TouchPhase::Started, 40.);

    assert_ne!(scrolled_to(&ui), transcript, "touching again, the swipe scrolls the transcript");
}

#[gpui_kit::test]
fn a_fling_of_the_transcript_passes_over_a_call_s_output(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    let (command, output) = open_a_command(&mut ui, "seq 1 20");
    let scrolled_in = |ui: &mut Ui<'_>| top(ui, command.clone()) - top(ui, output.clone());
    // Partway down the output, so it has room to catch a fling up if it would.
    wheel(&mut ui, &output, -60.);
    let line = ui.labelled(COMMAND_LINE)[0].path().last().unwrap().clone();
    swipe(&mut ui, &line, TouchPhase::Started, 40.);
    swipe(&mut ui, &line, TouchPhase::Ended, 0.);
    let (inside, transcript) = (scrolled_in(&mut ui), scrolled_to(&ui));

    swipe(&mut ui, &output, TouchPhase::Moved, 30.);

    assert_ne!(scrolled_to(&ui), transcript, "the transcript's fling carries on over the output");
    assert_eq!(scrolled_in(&mut ui), inside, "and leaves the output where it was");
}

#[gpui_kit::test]
fn a_swipe_that_starts_on_the_transcript_carries_on_over_a_call_s_output(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    let (command, output) = open_a_command(&mut ui, "seq 1 20");
    let scrolled_in = |ui: &mut Ui<'_>| top(ui, command.clone()) - top(ui, output.clone());
    // Partway down the output, so it has room to take the swipe up if it would.
    wheel(&mut ui, &output, -60.);
    let line = ui.labelled(COMMAND_LINE)[0].path().last().unwrap().clone();
    swipe(&mut ui, &line, TouchPhase::Started, 40.);
    let (inside, transcript) = (scrolled_in(&mut ui), scrolled_to(&ui));

    swipe(&mut ui, &output, TouchPhase::Moved, 30.);

    assert_ne!(scrolled_to(&ui), transcript, "the fingers go on scrolling the transcript over the output");
    assert_eq!(scrolled_in(&mut ui), inside, "and leave the output where it was");
}

#[gpui_kit::test]
fn a_spin_of_the_wheel_that_starts_on_the_transcript_carries_on_over_a_call_s_output(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    let (command, output) = open_a_command(&mut ui, "seq 1 20");
    let scrolled_in = |ui: &mut Ui<'_>| top(ui, command.clone()) - top(ui, output.clone());
    wheel(&mut ui, &output, -60.);
    let line = ui.labelled(COMMAND_LINE)[0].path().last().unwrap().clone();
    wheel(&mut ui, &line, 40.);
    let (inside, transcript) = (scrolled_in(&mut ui), scrolled_to(&ui));

    keep_turning(&mut ui, &output, 30.);

    assert_ne!(scrolled_to(&ui), transcript, "the spin goes on scrolling the transcript over the output");
    assert_eq!(scrolled_in(&mut ui), inside, "and leaves the output where it was");

    let transcript = scrolled_to(&ui);
    wheel(&mut ui, &output, 30.);

    assert_eq!(scrolled_in(&mut ui), inside - px(30.), "after a pause, the wheel scrolls the output");
    assert_eq!(scrolled_to(&ui), transcript);
}

#[gpui_kit::test]
fn the_wheel_over_output_that_fits_scrolls_the_transcript(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    let (_, output) = open_a_command(&mut ui, "echo short");
    let transcript = scrolled_to(&ui);

    wheel(&mut ui, &output, 40.);

    assert_ne!(scrolled_to(&ui), transcript, "a block with nothing to scroll leaves the wheel to the transcript");
}

#[gpui_kit::test]
fn a_subagent_s_work_shows_in_its_call_as_it_arrives(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    ui.input("agent:Look around; tool:echo after; say:done");
    ui.press("enter");
    // The subagent works for a second, so its call is open before its work arrives.
    ui.wait("the subagent's call", |ui| ui.labelled(AGENT_LINE).len() == 1);
    ui.click_labelled(AGENT_LINE, 0);
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));

    let nested = bounds(&mut ui, SUBAGENT_CALL);
    assert_eq!(nested.len(), 1, "the subagent's call shows once, in its agent's, and not in a row of its own");
    let (agent, after) = (bounds(&mut ui, AGENT_LINE), bounds(&mut ui, COMMAND_LINE));
    assert!(nested[0].left() > agent[0].left(), "it sits in from the agent's line: {nested:?} {agent:?}");
    assert!(nested[0].bottom() <= after[0].top(), "what came after is below it: {nested:?} {after:?}");

    ui.click_labelled(AGENT_LINE, 0);

    assert!(ui.labelled(SUBAGENT_CALL).is_empty(), "closing the call hides its work");
}

#[gpui_kit::test]
fn an_edit_s_diff_stays_while_the_pointer_is_on_it_and_scrolls(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    // Enough words first that the transcript itself could scroll, then a diff longer than its preview.
    ui.input("burst:3000; edit:20; say:done");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    let edit = ui.labelled("Edit")[0].path().last().unwrap().clone();
    ui.hover(edit);
    ui.wait_out_a_tooltip();
    assert!(ui.visible("edit-preview"), "hovering the edit shows its diff");

    ui.hover("edit-preview");
    ui.wait_out_a_tooltip();

    assert!(ui.visible("edit-preview"), "the diff stays while the pointer is on it");
    let (lines, transcript) = (top(&mut ui, "edit-preview-lines"), scrolled_to(&ui));
    ui.scroll("edit-preview", ScrollDelta::Pixels(point(px(0.), px(-100.))));
    ui.scroll("edit-preview", ScrollDelta::Pixels(point(px(0.), px(40.))));
    assert_eq!(scrolled_to(&ui), transcript, "the wheel over the diff leaves the transcript under it");
    assert_eq!(top(&mut ui, "edit-preview-lines"), lines - px(60.), "and scrolls the diff");

    ui.hover("new-chat");
    ui.wait_out_a_tooltip();

    assert!(!ui.visible("edit-preview"), "the diff goes once the pointer leaves it");
}

#[gpui_kit::test]
fn a_read_of_an_image_opens_to_the_image_the_hub_kept(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let workspace = ui.new_workspace();
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 3, image::Rgba([200, 30, 30, 255])).write_to(&mut png, ImageFormat::Png).unwrap();
    std::fs::write(ui.worktree(&workspace).join("shot.png"), png.into_inner()).unwrap();
    ui.input("read:shot.png; say:done");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    assert!(ui.shown("picture/").is_empty(), "a call starts closed");

    ui.click_labelled("Read 1 image", 0);

    ui.wait("the image, from the hub", |ui| ui.shown("picture/").len() == 1);
}

/// A new workspace in full access, so the fake agent's calls run without waiting to be allowed.
fn in_full_access<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.pick("mode-picker", 3);
    ui.wait("full access", |ui| ui.chat_mode().as_deref() == Some("bypassPermissions"));
    ui
}

/// Runs `command` between enough words that the transcript can scroll either way, and opens its call with the
/// transcript at its end, where the button that jumps to the end would leave it, so the call is on screen however far
/// the transcript had followed the turn. The IDs of the block's command and of its output.
fn open_a_command(ui: &mut Ui<'_>, command: &str) -> (ElementId, ElementId) {
    ui.input(&format!("burst:3000; tool:{command}; burst:150; say:done"));
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    // The line the turn ends with can come just after, and drawn mid-click it would move the call's line away from
    // under the pointer.
    ui.wait("the turn's end", |ui| ui.events().iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. })));
    ui.cx.update_entity(&ui.transcript().unwrap(), |t, cx| {
        t.scroller().scroll_to_end();
        cx.notify();
    });
    ui.click_labelled(COMMAND_LINE, 0);
    assert_eq!(command_open(ui), Some(true));
    (ui.shown("command/").remove(0), ui.shown("output/").remove(0))
}

/// Whether the command's line says it's open, as it tells assistive apps.
fn command_open(ui: &mut Ui<'_>) -> Option<bool> {
    ui.labelled(COMMAND_LINE).first().and_then(ElementSnapshot::expanded)
}

/// Where each element with this label is drawn.
fn bounds(ui: &mut Ui<'_>, label: &str) -> Vec<Bounds<Pixels>> {
    ui.labelled(label).iter().map(ElementSnapshot::bounds).collect()
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
    let position = ui.cx.update_window(ui.handle, |_, window, _| window.find(id.clone()).bounds().center()).unwrap();
    let delta = ScrollDelta::Pixels(point(px(0.), px(y)));
    let event = ScrollEvent { position, delta, touch_phase: phase, ..ScrollEvent::default() };
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(event.to_platform_input(), cx)).unwrap();
    ui.frame();
}

/// Where the top of the element is drawn.
fn top(ui: &mut Ui<'_>, id: impl Into<ElementId>) -> Pixels {
    let id = id.into();
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds().top()).unwrap()
}

/// How far the transcript is scrolled.
fn scrolled_to(ui: &Ui<'_>) -> ListOffset {
    ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.scroller().logical_scroll_top())
}

#[gpui_kit::test]
fn asking_the_advisor_opens_to_its_advice_or_says_only_the_agent_can_read_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    ui.input("advise; advise:Test it first.; advise-failed:overloaded; say:done");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    assert_eq!(ui.labelled(ADVISOR_LINE).len(), 3, "a line for each call, the failed one too");
    assert!(ui.shown("encrypted/").is_empty(), "a call starts closed");

    ui.click_labelled(ADVISOR_LINE, 0);
    ui.click_labelled(ADVISOR_LINE, 1);

    assert_eq!(ui.shown("encrypted/").len(), 1, "the reply only the agent can read");
    assert_eq!(ui.shown("advice/").len(), 1, "and the one that can be read");
}

#[gpui_kit::test]
fn a_call_to_the_advisor_the_agent_went_on_without_says_so(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    ui.input("advise-unanswered; advise; say:done");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    assert_eq!(ui.labelled(ADVISOR_LINE).len(), 2, "the call with no reply, and the one the agent made again");

    ui.click_labelled(ADVISOR_LINE, 0);
    ui.click_labelled(ADVISOR_LINE, 1);

    assert_eq!(ui.shown("dropped/").len(), 1, "the first says the agent went on without a reply");
    assert!(ui.shown("no-reply/").is_empty(), "rather than that one might still come");
    assert_eq!(ui.shown("encrypted/").len(), 1, "and the second opens to its reply");
}

#[gpui_kit::test]
fn a_call_to_the_advisor_with_nothing_to_read_opens_to_a_line_saying_why(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    ui.input("advise:; say:done; advise-unanswered");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    assert_eq!(ui.labelled(ADVISOR_LINE).len(), 2, "a call the turn ended on stays, since nothing went on without it");

    ui.click_labelled(ADVISOR_LINE, 0);
    ui.click_labelled(ADVISOR_LINE, 1);

    assert_eq!(ui.shown("empty-reply/").len(), 1, "a reply with nothing in it");
    assert_eq!(ui.shown("no-reply/").len(), 1, "and no reply at all");
}

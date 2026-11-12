//! Tool calls in the transcript: each is one line, which opens to what the call did and closes again. A read of an
//! image opens to the image. How the wheel scrolls what a call shows, in its block or in the diff an edit shows while
//! it's hovered, is in `wheel.rs`, and an opened command is in `commands.rs`. `subagents.rs` has the calls that start
//! subagents.

#![cfg(test)]

use std::io::Cursor;

use gpui_kit::test::{ElementSnapshot, TestWindowExt as _};
use gpui_kit::{AppContext as _, Bounds, ElementId, Pixels, TestAppContext};
use image::ImageFormat;
use protocol::event::EventBody;

use crate::support::{Stack, Ui};

mod commands;
mod wheel;

/// The fake agent's commands come with no description, so their line says they run something.
const COMMAND_LINE: &str = "Run";
/// Asking the advisor, which the API runs rather than the agent.
const ADVISOR_LINE: &str = "Ask the advisor";

#[gpui_kit::test]
fn a_call_opens_to_what_it_did_and_closes_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
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
fn a_read_of_an_image_opens_to_the_image_the_hub_kept(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
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

/// A new workspace, open in the app.
pub(crate) fn in_a_new_workspace<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui
}

/// Runs `command`, opens its call as [`open_a_call`] does, and gives the IDs of the block's command and of its output.
pub(crate) fn open_a_command(ui: &mut Ui<'_>, command: &str) -> (ElementId, ElementId) {
    open_a_call(ui, &format!("tool:{command}"), COMMAND_LINE);
    assert_eq!(command_open(ui), Some(true));
    (ui.shown("command/").remove(0), ui.shown("output/").remove(0))
}

/// Runs the fake agent's `step` between enough words that the transcript can scroll either way, and opens the call
/// it makes, whose line says `line`, with the transcript at its end, where the button that jumps to the end would
/// leave it, so the call is on screen however far the transcript had followed the turn.
fn open_a_call(ui: &mut Ui<'_>, step: &str, line: &str) {
    ui.input(&format!("burst:3000; {step}; burst:150; say:done"));
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    // The line the turn ends with can come just after, and drawn mid-click it would move the call's line away from
    // under the pointer.
    ui.wait("the turn's end", |ui| ui.events().iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. })));
    ui.cx.update_entity(&ui.transcript().unwrap(), |t, cx| {
        t.list().go_to_latest();
        cx.notify();
    });
    ui.click_labelled(line, 0);
}

/// Whether the command's line says it's open, as it tells assistive apps.
fn command_open(ui: &mut Ui<'_>) -> Option<bool> {
    ui.labelled(COMMAND_LINE).first().and_then(ElementSnapshot::expanded)
}

/// Where the element is drawn.
pub(crate) fn bounds(ui: &mut Ui<'_>, id: impl Into<ElementId>) -> Bounds<Pixels> {
    let id = id.into();
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds()).unwrap()
}

#[gpui_kit::test]
fn asking_the_advisor_opens_to_its_advice_or_says_only_the_agent_can_read_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
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
    let mut ui = in_a_new_workspace(cx, &stack);
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
    let mut ui = in_a_new_workspace(cx, &stack);
    ui.input("advise:; say:done; advise-unanswered");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    assert_eq!(ui.labelled(ADVISOR_LINE).len(), 2, "a call the turn ended on stays, since nothing went on without it");

    ui.click_labelled(ADVISOR_LINE, 0);
    ui.click_labelled(ADVISOR_LINE, 1);

    assert_eq!(ui.shown("empty-reply/").len(), 1, "a reply with nothing in it");
    assert_eq!(ui.shown("no-reply/").len(), 1, "and no reply at all");
}

//! Tool calls in the transcript: each is one line, which opens to what the call did and closes again. A call that
//! started a subagent opens to the subagent's work, which arrives after it. An edit shows its diff while it's hovered,
//! and a read of an image opens to the image.

#![cfg(test)]

use std::io::Cursor;
use std::time::Duration;

use gpui_kit::test::{ElementSnapshot, TestWindowExt as _};
use gpui_kit::{AppContext as _, Bounds, ListOffset, Pixels, ScrollDelta, TestAppContext, point, px};
use image::ImageFormat;

use crate::support::{Stack, Ui};

/// The fake agent's commands come with no description, so their line says they run something.
const COMMAND_LINE: &str = "Run";
/// What the fake agent says its subagent is for, and the description of the call the subagent makes.
const AGENT_LINE: &str = "Look around";
const SUBAGENT_CALL: &str = "List what's here";
/// Longer than GPUI waits to show a tooltip, and to hide one the pointer can move onto.
const LONGER_THAN_A_TOOLTIP_WAITS: Duration = Duration::from_secs(1);

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
    wait_out_a_tooltip(&mut ui);
    assert!(ui.visible("edit-preview"), "hovering the edit shows its diff");

    ui.hover("edit-preview");
    wait_out_a_tooltip(&mut ui);

    assert!(ui.visible("edit-preview"), "the diff stays while the pointer is on it");
    let (lines, transcript) = (top(&mut ui, "edit-preview-lines"), scrolled_to(&ui));
    ui.scroll("edit-preview", ScrollDelta::Pixels(point(px(0.), px(-100.))));
    ui.scroll("edit-preview", ScrollDelta::Pixels(point(px(0.), px(40.))));
    assert_eq!(scrolled_to(&ui), transcript, "the wheel over the diff leaves the transcript under it");
    assert_eq!(top(&mut ui, "edit-preview-lines"), lines - px(60.), "and scrolls the diff");

    ui.hover("new-chat");
    wait_out_a_tooltip(&mut ui);

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

/// Whether the command's line says it's open, as it tells assistive apps.
fn command_open(ui: &mut Ui<'_>) -> Option<bool> {
    ui.labelled(COMMAND_LINE).first().and_then(ElementSnapshot::expanded)
}

/// Where each element with this label is drawn.
fn bounds(ui: &mut Ui<'_>, label: &str) -> Vec<Bounds<Pixels>> {
    ui.labelled(label).iter().map(ElementSnapshot::bounds).collect()
}

fn wait_out_a_tooltip(ui: &mut Ui<'_>) {
    ui.cx.executor().advance_clock(LONGER_THAN_A_TOOLTIP_WAITS);
    ui.frame();
}

/// Where the top of the element is drawn.
fn top(ui: &mut Ui<'_>, id: &'static str) -> Pixels {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds().top()).unwrap()
}

/// How far the transcript is scrolled.
fn scrolled_to(ui: &Ui<'_>) -> ListOffset {
    let scroller = ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.scroller().clone());
    ui.cx.read_entity(&scroller, |s, _| s.logical_scroll_top())
}

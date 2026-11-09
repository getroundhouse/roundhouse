//! Tool calls in the transcript: each is one line, which opens to what the call did and closes again. A call that
//! started a subagent opens to the subagent's work, which arrives after it.

#![cfg(test)]

use gpui_kit::test::ElementSnapshot;
use gpui_kit::{Bounds, Pixels, TestAppContext};

use crate::support::{Stack, Ui};

/// The fake agent's commands come with no description, so their line says they run something.
const COMMAND_LINE: &str = "Run";
/// What the fake agent says its subagent is for, and the description of the call the subagent makes.
const AGENT_LINE: &str = "Look around";
const SUBAGENT_CALL: &str = "List what's here";

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

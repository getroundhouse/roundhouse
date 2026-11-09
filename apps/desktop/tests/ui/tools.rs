//! Tool calls in the transcript: each is one line, which opens to what the call did and closes again.

#![cfg(test)]

use gpui_kit::TestAppContext;
use gpui_kit::test::ElementSnapshot;

use crate::support::{Stack, Ui};

/// The fake agent's commands come with no description, so their line says they run something.
const COMMAND_LINE: &str = "Run";

#[gpui_kit::test]
fn a_call_opens_to_what_it_did_and_closes_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.pick("mode-picker", 3);
    ui.wait("full access", |ui| ui.chat_mode().as_deref() == Some("bypassPermissions"));
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

/// Whether the command's line says it's open, as it tells assistive apps.
fn command_open(ui: &mut Ui<'_>) -> Option<bool> {
    ui.labelled(COMMAND_LINE).first().and_then(ElementSnapshot::expanded)
}

//! Quitting, which waits for what was started on the hub, but not for the agent.

#![cfg(test)]

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::event::EventBody;

use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn a_prompt_sent_just_before_quitting_reaches_the_hub(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let chat = ui.active_chat();
    let client = ui.view().client().unwrap();
    ui.input("say:hi");

    // Straight after, with nothing let run in between.
    ui.cx.update_window(ui.handle, |_, window, cx| window.press("enter", cx)).unwrap();
    ui.cx.update(gpui_kit::App::shutdown);

    // The app's threads end once it has quit, so nothing it started may still be under way. The test's carry on.
    assert!(client.commands().finished().try_recv().is_ok(), "quitting didn't wait for the prompt");
    let (events, _) = stack.client().session_read(&chat, 0, 1000, false).unwrap();
    let prompts: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.body {
            EventBody::UserPrompt { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(prompts, ["say:hi"]);
}

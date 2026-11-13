//! Quitting, which waits for what was started on the hub, and for what's typed to be kept, but not for the agent, and
//! asks first about what it would still lose.

#![cfg(test)]

use std::sync::atomic::Ordering;
use std::time::Duration;

use desktop::app::Quitting;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};
use protocol::event::EventBody;
use protocol::model::Seq;
use protocol::prompt::Prompt;

use crate::menus::{QUIT, choose};
use crate::support::{Frozen, Stack, Ui};

#[gpui_kit::test]
fn a_prompt_sent_just_before_quitting_reaches_the_hub(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    let chat = ui.active_chat();
    let client = ui.view().client().unwrap();
    ui.input("say:hi");

    // Straight after, with nothing let run in between.
    ui.cx.update_window(ui.handle, |_, window, cx| window.press("enter", cx)).unwrap();
    ui.cx.update(gpui_kit::App::shutdown);

    // The app's threads end once it has quit, so nothing it started may still be under way. The test's carry on.
    assert!(client.commands().finished().try_recv().is_ok(), "quitting didn't wait for the prompt");
    let (events, _) = stack.client().session_read(&chat, Seq::default(), 1000, false).unwrap();
    let prompts: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.body {
            EventBody::UserPrompt { prompt: Prompt { text, .. }, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(prompts, ["say:hi"]);
}

#[gpui_kit::test]
fn whats_typed_just_before_quitting_is_kept(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    let chat = ui.active_chat();
    let kept = || stack.client().session_get(&chat).unwrap().draft.text;
    // The first change is kept at once, so once the hub has answered it, the one after it waits for typing to pause.
    ui.input("half");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    assert_eq!(kept(), "half");
    ui.input(" a thought");

    // Straight after, with nothing let run in between.
    ui.cx.update(gpui_kit::App::shutdown);

    assert_eq!(kept(), "half a thought", "quitting didn't wait for what was typed to be kept");
}

/// Leaves something only the app has, which quitting would lose: what's typed in the new-workspace dialog while the
/// hub doesn't answer, so it can't keep it until this goes.
fn leave_something_in_the_dialog(ui: &mut Ui<'_>, stack: &Stack) -> Frozen {
    ui.open_new_workspace();
    let frozen = Frozen::stop(stack.client().hub_status().unwrap().pid);
    ui.input("half a thought");
    ui.press("escape");
    frozen
}

fn quit_from_the_menu(ui: &mut Ui<'_>) {
    choose(ui, QUIT.0, QUIT.1);
}

#[gpui_kit::test]
fn quitting_asks_first_when_something_would_be_lost(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let _frozen = leave_something_in_the_dialog(&mut ui, &stack);

    quit_from_the_menu(&mut ui);

    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    let (title, detail) = ui.cx.pending_prompt().unwrap();
    assert_eq!(title, "Quit Roundhouse?");
    assert_eq!(detail, "Something in 1 chat hasn't been sent, and quitting loses it.");
    ui.cx.simulate_prompt_answer("Keep working");
    ui.frame();
    assert_eq!(ui.view().quitting, Quitting::No);
    // Asked again, rather than taken as already answered.
    quit_from_the_menu(&mut ui);
    ui.wait("the question again", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Quit");
    ui.wait("quitting", |ui| ui.view().quitting == Quitting::Quit);
}

/// What's typed and not yet kept, since typing hasn't paused, is on its way to the hub, so quitting waits for it
/// rather than asking.
#[gpui_kit::test]
fn quitting_waits_for_whats_on_its_way_to_the_hub(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ui.input("half a thought");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    // Once the hub has answered a change, the one after it waits for typing to pause. One letter, as typing several
    // can take longer than the pause in a test.
    ui.input("?");

    quit_from_the_menu(&mut ui);

    ui.wait("quitting", |ui| ui.view().quitting == Quitting::Quit);
    assert!(!ui.cx.has_pending_prompt());
    assert_eq!(stack.client().session_get(&chat).unwrap().draft.text, "half a thought?");
}

/// What's typed is lost if the hub hasn't got it by the time the app goes, as when the hub doesn't answer, so quitting
/// waits only so long for it, then asks.
#[gpui_kit::test]
fn quitting_asks_about_whats_typed_once_the_hub_has_had_long_enough_to_take_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ui.input("half");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    let _frozen = Frozen::stop(stack.client().hub_status().unwrap().pid);
    ui.input(" a thought");

    quit_from_the_menu(&mut ui);

    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    let (_, detail) = ui.cx.pending_prompt().unwrap();
    assert_eq!(detail, "Something in 1 chat hasn't been sent, and quitting loses it.");
}

/// Closing the window quits the app, so it asks as quitting does, and the window stays open while it asks.
#[gpui_kit::test]
fn closing_the_window_asks_first_only_when_something_would_be_lost(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let close = |ui: &mut Ui<'_>| VisualTestContext::from_window(ui.handle, ui.cx).simulate_close();
    assert!(close(&mut ui), "with nothing to lose, it closes");
    assert!(!ui.cx.has_pending_prompt());
    let _frozen = leave_something_in_the_dialog(&mut ui, &stack);

    assert!(!close(&mut ui), "it stays open to ask");

    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Quit");
    ui.wait("quitting", |ui| ui.view().quitting == Quitting::Quit);
}

/// Sends a prompt while the hub doesn't answer, so it's still on its way.
fn send_to_a_frozen_hub(ui: &mut Ui<'_>, stack: &Stack) -> Frozen {
    let frozen = Frozen::stop(stack.client().hub_status().unwrap().pid);
    ui.input("say:look");
    ui.press("enter");
    frozen
}

/// A prompt the hub hasn't recorded yet is lost if the app goes first, so quitting waits for it, rather than asking.
#[gpui_kit::test]
fn quitting_waits_for_a_prompt_on_its_way_to_the_hub(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    let frozen = send_to_a_frozen_hub(&mut ui, &stack);
    // Well within the wait.
    frozen.thaw_after(Duration::from_millis(500));

    quit_from_the_menu(&mut ui);

    ui.wait("quitting", |ui| ui.view().quitting == Quitting::Quit);
    assert!(!ui.cx.has_pending_prompt());
    let (events, _) = stack.client().session_read(&chat, Seq::default(), 1000, false).unwrap();
    assert!(
        events.iter().any(
            |e| matches!(&e.body, EventBody::UserPrompt { prompt: Prompt { text, .. }, .. } if text == "say:look")
        )
    );
}

/// Quitting waits for any call the app has made that the hub hasn't answered yet, as it does for what's typed.
#[gpui_kit::test]
fn quitting_waits_for_a_call_on_its_way_to_the_hub(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let frozen = Frozen::stop(stack.client().hub_status().unwrap().pid);
    let _reply = ui.view().client().unwrap().start(client_core::Client::hub_status);
    let thawing = frozen.thaw_after(Duration::from_millis(500));

    quit_from_the_menu(&mut ui);

    ui.wait("quitting", |ui| ui.view().quitting == Quitting::Quit);
    assert!(thawing.load(Ordering::SeqCst), "it quit before the hub could answer");
}

#[gpui_kit::test]
fn quitting_asks_about_a_prompt_the_hub_hasnt_taken_in_time(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let _frozen = send_to_a_frozen_hub(&mut ui, &stack);

    quit_from_the_menu(&mut ui);

    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    let (_, detail) = ui.cx.pending_prompt().unwrap();
    assert_eq!(detail, "Something in 1 chat hasn't been sent, and quitting loses it.");
}

#[gpui_kit::test]
fn quitting_again_while_it_waits_or_asks_doesnt_ask_twice(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let _frozen = leave_something_in_the_dialog(&mut ui, &stack);

    // Twice before anything runs, so the second comes as the first waits.
    ui.cx.update(|cx| {
        cx.dispatch_action(&desktop::Quit);
        cx.dispatch_action(&desktop::Quit);
    });
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    quit_from_the_menu(&mut ui);
    ui.cx.simulate_prompt_answer("Keep working");
    ui.frame();

    assert!(!ui.cx.has_pending_prompt(), "it asked more than once");
}

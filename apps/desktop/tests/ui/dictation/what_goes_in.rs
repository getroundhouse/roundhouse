//! What goes in the composer from the microphone button: final text at the cursor, nothing on an Enter with nothing to
//! send, all that was said once Enter sends or queues it, nothing more after, and what's told when nothing is heard or
//! hearing fails.

use desktop::workspace_pane::{MICROPHONE, NOTHING_HEARD, TURN_ON};
use dictation::fake::FakeEngine;
use gpui_kit::TestAppContext;
use protocol::event::EventBody;

use super::support::{SCRIPT, dictating, ended, hearing, live, ready, ready_with, say, sent};
use crate::queue::queued;
use crate::support::{Stack, Ui};

/// The text of the last prompt sent in the chat.
fn sent_text(ui: &Ui) -> Option<String> {
    ui.events().into_iter().rev().find_map(|e| match e.body {
        EventBody::UserPrompt { prompt, .. } => Some(prompt.text),
        _ => None,
    })
}

#[gpui_kit::test]
fn the_microphone_starts_and_finish_keeps_the_words_without_sending(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ui.click(MICROPHONE);
    assert!(ui.has(TURN_ON) && live(&mut ui).is_none(), "it offers dictation until it's on");
    ui.press("escape");
    let engine = ready(&mut ui);

    ui.click(MICROPHONE);
    say(&mut ui, &engine, "fix the flaky test");
    hearing(&mut ui, "fix");
    ui.click(dictation_ui::STOP);

    ui.wait("the final text", |ui| ui.composer_text() == "Fix the flaky test.");
    assert!(!sent(&ui));
}

#[gpui_kit::test]
fn what_is_said_goes_in_at_the_cursor_with_a_space_either_side(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Please now");
    for _ in 0.."now".len() {
        ui.press("left");
    }

    ui.click("dictate");
    say(&mut ui, &engine, "fix the test. / in hub");
    ui.click(dictation_ui::STOP);

    ui.wait("the final text", |ui| ui.composer_text() == "Please Fix the test. In hub. now");
}

#[gpui_kit::test]
fn an_enter_with_nothing_typed_or_heard_yet_goes_on_dictating(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.click("dictate");

    ui.press("enter");

    assert!(!sent(&ui) && dictating(&mut ui));
    say(&mut ui, &engine, "fix the test");
    ui.click(dictation_ui::STOP);
    ui.wait("the final text", |ui| ui.composer_text() == "Fix the test.");
}

#[gpui_kit::test]
fn an_enter_while_dictating_sends_all_that_was_said_once_it_has_gone_in(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test. / in hub");
    ui.wait("the first sentence", |ui| ui.composer_text() == "Fix the test.");
    hearing(&mut ui, "in");

    ui.press("enter");

    // The last sentence too, which was still showing.
    ui.wait("it to be sent", |ui| sent(ui));
    assert_eq!(sent_text(&ui).as_deref(), Some("Fix the test. In hub."));
    assert!(ui.composer_text().is_empty() && live(&mut ui).is_none() && !dictating(&mut ui));
}

#[gpui_kit::test]
fn an_enter_while_dictating_sends_nothing_if_hearing_fails_and_keeps_what_went_in(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready_with(&mut ui, FakeEngine::new(SCRIPT).failing());
    ui.input("Keep this");
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    ui.press("enter");

    ui.wait("what's told", |ui| ui.view().error.as_deref().is_some_and(|e| e.starts_with("Dictation stopped: ")));
    ended(&mut ui);
    assert!(!sent(&ui));
    assert_eq!(ui.composer_text(), "Keep this");
}

#[gpui_kit::test]
fn sending_ends_dictating_and_leaves_nothing_for_escape_to_put_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("say:sent");
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    ui.press("enter");

    ui.wait("it to be sent", |ui| sent(ui));
    assert_eq!(sent_text(&ui).as_deref(), Some("say:sent Fix the test."));
    assert_eq!(live(&mut ui), None);
    ui.press("escape");
    assert_eq!(ui.composer_text(), "", "what was sent doesn't come back");
}

#[gpui_kit::test]
fn queueing_ends_dictating_and_leaves_nothing_for_escape_to_put_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    // Busy until the test ends, so Enter queues.
    let _working = crate::steering::working(&mut ui, &stack);
    let engine = ready(&mut ui);
    ui.input("say:queued");
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    ui.press("secondary-enter");

    ui.wait("it to be queued", |ui| queued(ui).iter().any(|q| q.prompt.text == "say:queued Fix the test."));
    assert_eq!(ui.composer_text(), "");
    assert_eq!(live(&mut ui), None);
    ui.press("escape");
    assert_eq!(ui.composer_text(), "", "what was queued doesn't come back");
}

#[gpui_kit::test]
fn hearing_nothing_or_failing_to_hear_says_so(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    ui.click("dictate");
    ui.click(dictation_ui::STOP);
    ui.wait("what's told", |ui| ui.view().error.as_deref() == Some(NOTHING_HEARD));
    ended(&mut ui);

    let engine = ready_with(&mut ui, FakeEngine::new(SCRIPT).failing());
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    ui.click(dictation_ui::STOP);
    ui.wait("what's told", |ui| ui.view().error.as_deref().is_some_and(|e| e.starts_with("Dictation stopped: ")));
    ended(&mut ui);
    assert_eq!(ui.composer_text(), "");
}

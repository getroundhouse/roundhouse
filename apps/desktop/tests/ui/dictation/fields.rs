//! Shared dictation through the real dialog and question panel: draft saving, selecting the typed answer, deferred
//! submission, and the lifetime of inputs that remain alive while hidden.

use dictation::fake::FakeEngine;
use gpui_kit::{AppContext as _, Focusable as _, TestAppContext};
use protocol::event::EventBody;
use protocol::model::{AnswerSoFar, DecisionDraft, SoFar};

use super::support::{SCRIPT, ended, hearing, hold_option_space, let_go, live, press_space, ready, ready_with, say};
use crate::support::{Stack, Ui};

fn say_in_dialog(ui: &mut Ui, engine: &FakeEngine, words: &str) {
    let (dialog, audio) = (ui.dialog(), engine.saying(words));
    ui.cx.update(|cx| dialog.read(cx).hear(&audio, cx));
}

fn say_in_answer(ui: &mut Ui, engine: &FakeEngine, words: &str) {
    let (transcript, audio) = (ui.transcript().unwrap(), engine.saying(words));
    ui.cx.update(|cx| transcript.read(cx).question().unwrap().read(cx).hear(&audio, cx));
}

/// What's chosen and typed on each question asked, as the panel would have it kept, or `None` with nothing.
fn answer(ui: &Ui) -> Option<Vec<AnswerSoFar>> {
    let draft = ui.cx.read_entity(&ui.transcript().unwrap(), |t, cx| t.question().and_then(|p| p.read(cx).so_far(cx)));
    answers_of(&draft?)
}

fn answers_of(draft: &DecisionDraft) -> Option<Vec<AnswerSoFar>> {
    let SoFar::Answers { answers } = &draft.so_far else { return None };
    Some(answers.clone())
}

#[gpui_kit::test]
fn a_dictated_answer_is_selected_kept_and_sent_with_its_last_words(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.ask("ask:What next?=One/Two");
    let chat = ui.active_chat();
    assert!(!ui.has("dictate"), "question answers have no microphone button");
    ui.click("question-other");

    hold_option_space(&mut ui);
    say_in_answer(&mut ui, &engine, "fix the test. / in hub");

    ui.wait("the dictated answer to be selected and kept", |_| {
        stack
            .client()
            .session_get(&chat)
            .unwrap()
            .decision_draft
            .and_then(|draft| answers_of(&draft))
            .is_some_and(|answers| answers[0].typed == "Fix the test." && answers[0].other)
    });
    hearing(&mut ui, "in");
    ui.press("enter");
    ui.wait("the complete answer", |ui| ui.told_to_agent().contains("Fix the test. In hub."));
}

#[gpui_kit::test]
fn cancelling_an_answer_restores_its_text_without_leaving_the_input(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.ask("ask:What next?=One/Two");
    ui.click("question-other");
    ui.input("Keep this");
    hold_option_space(&mut ui);
    say_in_answer(&mut ui, &engine, "fix the test. / in hub");
    ui.wait("the first sentence", |ui| answer(ui).is_some_and(|d| d[0].typed.contains("Fix the test.")));

    ui.press("alt-escape");
    let_go(&mut ui);
    ui.input(" here");

    assert_eq!(answer(&ui).unwrap()[0].typed, "Keep this here");
    assert!(live(&mut ui).is_none());
    assert!(ui.has("question"));
}

#[gpui_kit::test]
fn a_question_change_drops_tentative_words_and_the_next_answer_has_its_own_recording(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.ask("ask:First?=One/Two|Second?=One/Two");
    ui.click("question-other");
    hold_option_space(&mut ui);
    say_in_answer(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    // The recording toolbar replaces the pager; navigation still ends a retained field.
    let panel = ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.question().unwrap().clone());
    ui.cx.update_window(ui.handle, |_, window, cx| window.focus(&panel.focus_handle(cx), cx)).unwrap();
    ui.let_what_is_asked_settle();
    ui.press("right");
    let_go(&mut ui);
    assert!(live(&mut ui).is_none());
    ui.click("question-other");
    hold_option_space(&mut ui);
    say_in_answer(&mut ui, &engine, "in hub");
    let_go(&mut ui);

    ui.wait("the second answer", |ui| answer(ui).is_some_and(|d| d[1].typed == "In hub."));
    let draft = answer(&ui).unwrap();
    assert!(draft[0].typed.is_empty());
    assert_eq!(draft[1].typed, "In hub.");
}

#[gpui_kit::test]
fn creating_waits_for_the_final_dictated_prompt(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let engine = ready(&mut ui);
    let before = ui.view().records().workspaces().len();
    ui.open_new_workspace();
    // Its middle is spare writing room below the input's current lines, and still takes typing and dictation.
    ui.click("new-workspace-writing");
    ui.input("say:");
    hold_option_space(&mut ui);
    say_in_dialog(&mut ui, &engine, "fix the test. / in hub");
    ui.wait("the first sentence", |ui| ui.dialog_text() == "say: Fix the test.");
    hearing(&mut ui, "in");

    assert!(!ui.has("new-workspace-create"));
    ui.press("enter");

    ui.wait_for_a_new_workspace(before);
    ui.wait("the complete prompt to be sent", |ui| {
        ui.events().iter().any(
            |e| matches!(&e.body, EventBody::UserPrompt { prompt, .. } if prompt.text == "say: Fix the test. In hub."),
        )
    });
    assert!(!ui.has_dialog());
}

#[gpui_kit::test]
fn escape_cancels_first_then_closes_and_reopening_has_no_old_recording(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let engine = ready(&mut ui);
    ui.open_new_workspace();
    ui.input("Keep this");
    hold_option_space(&mut ui);
    say_in_dialog(&mut ui, &engine, "fix the test. / in hub");
    ui.wait("the first sentence", |ui| ui.dialog_text().contains("Fix the test."));

    ui.press("alt-escape");
    let_go(&mut ui);
    assert!(ui.has_dialog());
    assert_eq!(ui.dialog_text(), "Keep this");
    ui.press("escape");
    ui.wait_out_the_dialog_going();
    ui.open_new_workspace();

    assert!(live(&mut ui).is_none());
    assert_eq!(ui.dialog_text(), "Keep this");
    ui.press("shift-tab");
    assert!(ui.dialog_planning(), "adding dictation preserves the prompt's other shortcuts");
}

#[gpui_kit::test]
fn a_transcription_failure_keeps_the_dialog_and_its_text_without_creating(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let engine = ready_with(&mut ui, FakeEngine::new(SCRIPT).failing());
    let before = ui.view().records().workspaces().len();
    ui.open_new_workspace();
    ui.input("Keep this");
    hold_option_space(&mut ui);
    say_in_dialog(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    ui.press("enter");

    ui.wait("the transcription error", |ui| {
        ui.view().error.as_deref().is_some_and(|e| e.starts_with("Dictation stopped:"))
    });
    ended(&mut ui);
    assert!(ui.has_dialog());
    assert_eq!(ui.view().records().workspaces().len(), before);
    assert_eq!(ui.dialog_text(), "Keep this");
}

#[gpui_kit::test]
fn holding_space_in_a_multiple_answer_keeps_its_choices_and_enter_finishes_live_words(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.ask("ask:*What next?=One/Two");
    ui.click("question-answer-0");
    ui.click("question-other");
    press_space(&mut ui);
    ui.cx.executor().advance_clock(dictation_ui::SPACE_DICTATES_AFTER);
    ui.frame();
    say_in_answer(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    assert!(!ui.has("question-send"));
    ui.press("enter");

    ui.wait("both the choice and the complete dictated answer", |ui| {
        let told = ui.told_to_agent();
        told.contains("One") && told.contains("Fix the test.")
    });
}

#[gpui_kit::test]
fn enter_can_finish_the_first_sentence_before_any_answer_is_final(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.ask("ask:What next?=One/Two");
    ui.click("question-other");
    hold_option_space(&mut ui);
    say_in_answer(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    assert!(answer(&ui).is_none(), "no final text has selected an answer yet");
    assert!(!ui.has("question-send"));
    ui.press("enter");
    ui.wait("the first sentence to be sent", |ui| ui.told_to_agent().contains("Fix the test."));
}

#[gpui_kit::test]
fn starting_in_the_dialog_releases_the_composers_recording(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Keep this");
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    ui.open_new_workspace();
    hold_option_space(&mut ui);
    say_in_dialog(&mut ui, &engine, "in hub");
    let_go(&mut ui);
    ui.wait("the dialog's own words", |ui| ui.dialog_text() == "In hub.");
    ui.press("escape");
    ui.wait_out_the_dialog_going();
    assert_eq!(ui.composer_text(), "Keep this");
    assert!(live(&mut ui).is_none());
}

#[gpui_kit::test]
fn dictating_feedback_never_turns_an_empty_denial_into_allow(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    let engine = ready(&mut ui);
    ui.ask(r#"tool:rm -rf "$(cat gone-path)""#);
    ui.click("question-answer-0");
    ui.click("question-other");
    hold_option_space(&mut ui);
    ui.press("enter");
    ended(&mut ui);
    // An answer sent is a call of the app's, and the hub records it before it replies, so once every call has ended
    // the hub's record says whether one was sent. The window's copy of the record comes on the feed, and can be later.
    ui.wait_for_every_call();
    let status = stack.client().session_get(&chat).unwrap().status;
    assert_eq!(status, protocol::model::SessionStatus::WaitingApproval, "the empty denial sent nothing");

    ui.click("question-other");
    let_go(&mut ui);
    hold_option_space(&mut ui);
    say_in_answer(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    ui.press("enter");
    ui.wait("the dictated denial", |ui| {
        ui.events().iter().any(|event| {
            matches!(&event.body, EventBody::ApprovalResolved { decision: Some(decision), .. }
                if matches!(**decision, protocol::capability::approvals::Decision::Deny { .. }))
        })
    });
    ui.wait("the agent to receive the dictated denial", |ui| ui.told_to_agent().contains("Fix the test."));
}

#[gpui_kit::test]
fn the_dialog_keeps_its_keys_when_the_microphone_offer_is_hidden(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.open_new_workspace();
    ui.click("dictate");
    ui.click("dictation-hide");
    super::support::type_keys(&mut ui, "a", "a");
    assert_eq!(ui.dialog_text(), "a");
    assert!(!ui.has("dictation-hide"));
    assert!(ui.has_dialog());
}

#[gpui_kit::test]
fn the_dialog_keeps_its_keys_when_the_model_is_ready_and_the_button_can_dictate(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.open_new_workspace();
    ui.click("dictate");
    let engine = ready(&mut ui);
    super::support::type_keys(&mut ui, "a", "a");
    assert_eq!(ui.dialog_text(), "a");
    ui.click("dictate");
    say_in_dialog(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    ui.press("escape");
    assert_eq!(ui.dialog_text(), "a");
    assert!(live(&mut ui).is_none());
    assert!(ui.has_dialog());
}

#[gpui_kit::test]
fn an_empty_recording_still_allows_a_workspace_without_a_first_prompt(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ready(&mut ui);
    let before = ui.view().records().workspaces().len();
    ui.open_new_workspace();
    hold_option_space(&mut ui);
    assert!(!ui.has("new-workspace-create"));
    ui.press("enter");
    ui.wait_for_a_new_workspace(before);
    assert!(!ui.has_dialog());
    assert!(!ui.events().iter().any(|event| matches!(event.body, EventBody::UserPrompt { .. })));
}

//! What's sent from where something was dictated is kept beside its recordings, once for each time something was dictated
//! there, and what's only typed never is.

use desktop::dictating::{DELETE_RECORDINGS, RECORDINGS_STATUS};
use desktop::workspace_pane::{CONVERSATION_BAR, SPACE_TALKS_AFTER};
use dictation::fake::FakeEngine;
use gpui_kit::{ScrollDelta, SharedString, TestAppContext, point, px};
use kept_recordings::{Field, SENT, Sent};
use protocol::model::SessionStatus;

use super::conversation::{in_conversation_mode, spoken};
use super::holding_space::{hold_for, hold_space_to_listen};
use super::keeping::folder;
use super::proposals::in_the_note;
use super::support::{
    ended, hold_option_space, let_go, on_the_keyboard, open_voice_settings, press_space, ready, say, type_keys,
};
use crate::queue::queued;
use crate::support::{Stack, Ui};
use crate::tools::in_a_new_workspace;

/// Turns Keep recordings on, or off.
fn keep(ui: &mut Ui, on: bool) {
    ui.cx.update(|cx| desktop::dictating::keep_recordings(on, cx));
}

/// Turns it on, or leaves it on, where there's already a folder, so something kept that shouldn't be is seen.
fn keep_in_its_folder(ui: &mut Ui, stack: &Stack) {
    std::fs::create_dir_all(folder(stack)).unwrap();
    keep(ui, true);
}

/// Everything kept of what was sent, in the order it was.
fn all_sent(stack: &Stack) -> Vec<Sent> {
    let lines = std::fs::read_to_string(folder(stack).join(SENT)).unwrap_or_default();
    lines.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

/// Waits for as many to have been kept as `texts`, and checks they're those.
fn sent_is(ui: &mut Ui, stack: &Stack, texts: &[&str]) -> Vec<Sent> {
    ui.wait("what was sent to be kept", |_| all_sent(stack).len() >= texts.len());
    let sent = all_sent(stack);
    assert_eq!(sent.iter().map(|sent| sent.text.as_str()).collect::<Vec<_>>(), texts);
    sent
}

/// Dictates "fix the test" into the field in focus, and waits for it to go in.
fn dictate(ui: &mut Ui, engine: &FakeEngine) {
    hold_option_space(ui);
    say(ui, engine, "fix the test");
    let_go(ui);
    ended(ui);
}

fn send(ui: &mut Ui, text: &str) {
    ui.input(text);
    ui.press("enter");
    ui.wait("the composer to empty", |ui| ui.composer_text().is_empty());
}

#[gpui_kit::test]
fn a_prompt_is_kept_whole_once_for_what_was_dictated_into_it_and_never_when_only_typed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    keep_in_its_folder(&mut ui, &stack);
    send(&mut ui, "say:only typed");

    ui.input("say: Please");
    dictate(&mut ui, &engine);
    ui.wait("the first to go in", |ui| ui.composer_text().ends_with("Fix the test."));
    ui.input(" Then");
    dictate(&mut ui, &engine);
    ui.wait("the second to go in", |ui| ui.composer_text().ends_with("Then Fix the test."));
    send(&mut ui, " today");
    send(&mut ui, "say:typed after");
    // Dictated, and then deleted: nothing's kept of it, and the next is only typed.
    dictate(&mut ui, &engine);
    ui.wait("it to go in", |ui| !ui.composer_text().is_empty());
    ui.press("secondary-a");
    send(&mut ui, "say:in place of it");
    send(&mut ui, "say:typed again");
    dictate(&mut ui, &engine);
    ui.wait("it to go in again", |ui| !ui.composer_text().is_empty());
    send(&mut ui, "");

    let whole = "say: Please Fix the test. Then Fix the test. today";
    let sent = sent_is(&mut ui, &stack, &[whole, "say:in place of it", "Fix the test."]);
    let chat = ui.active_chat().to_string();
    let from = &sent[0].from;
    assert_eq!((from.field, from.chat.as_ref(), from.said_out_loud), (Field::Composer, Some(&chat), false));
    assert!(from.workspace.is_some());
}

#[gpui_kit::test]
fn a_queued_prompt_is_kept_as_one_sent_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, done) = crate::queue::at_work(cx, &stack);
    let engine = ready(&mut ui);
    keep_in_its_folder(&mut ui, &stack);

    ui.input("say:only typed");
    ui.press("secondary-enter");
    ui.wait("the queue to have it", |ui| queued(ui).len() == 1);
    ui.input("say: Please");
    dictate(&mut ui, &engine);
    ui.wait("it to go in", |ui| ui.composer_text().ends_with("Fix the test."));
    ui.press("secondary-enter");

    sent_is(&mut ui, &stack, &["say: Please Fix the test."]);
    done.open();
}

#[gpui_kit::test]
fn whats_said_in_conversation_mode_is_kept_apart_from_whats_typed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, engine) = in_conversation_mode(cx, &stack);
    keep_in_its_folder(&mut ui, &stack);

    on_the_keyboard(&mut ui, true);
    press_space(&mut ui);
    hold_for(&mut ui, SPACE_TALKS_AFTER);
    say(&mut ui, &engine, "fix the flaky test");
    on_the_keyboard(&mut ui, false);
    let_go(&mut ui);
    ended(&mut ui);
    ui.wait("it to be sent", |ui| spoken(ui).is_some());

    let sent = sent_is(&mut ui, &stack, &["Fix the flaky test."]);
    assert!(sent[0].from.said_out_loud);
    type_keys(&mut ui, "a", "a");
    ui.wait("the composer to come back", |ui| !ui.has(CONVERSATION_BAR) && ui.composer_text() == "a");
    send(&mut ui, "");
    dictate(&mut ui, &engine);
    ui.wait("it to go in", |ui| !ui.composer_text().is_empty());
    send(&mut ui, "");
    let sent = sent_is(&mut ui, &stack, &["Fix the flaky test.", "Fix the test."]);
    assert!(!sent[1].from.said_out_loud);
}

#[gpui_kit::test]
fn what_was_typed_and_dictated_to_start_a_workspace_is_kept(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let engine = ready(&mut ui);
    keep_in_its_folder(&mut ui, &stack);
    let create = |ui: &mut Ui| {
        let before = ui.view().records().workspaces().len();
        ui.press("enter");
        ui.wait_for_a_new_workspace(before);
    };
    ui.open_new_workspace();
    ui.input("say:only typed");
    create(&mut ui);

    ui.open_new_workspace();
    ui.input("say:");
    dictate(&mut ui, &engine);
    ui.wait("it to go in", |ui| ui.dialog_text() == "say: Fix the test.");
    create(&mut ui);

    let sent = sent_is(&mut ui, &stack, &["say: Fix the test."]);
    let from = &sent[0].from;
    assert_eq!((from.field, from.workspace.as_ref(), from.chat.as_ref()), (Field::NewWorkspace, None, None));
}

#[gpui_kit::test]
fn a_comment_dictated_is_kept(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    let engine = ready(&mut ui);
    keep_in_its_folder(&mut ui, &stack);
    std::fs::write(ui.worktree(&workspace).join("signals.txt"), "one\ntwo\n").unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "signals.txt");
    ui.start_comment(&diff, "signals.txt", 1);
    ui.input("Only typed");
    ui.press("enter");
    ui.wait("the comment", |ui| ui.comments(&diff).len() == 1);

    ui.start_comment(&diff, "signals.txt", 2);
    ui.input("Please");
    dictate(&mut ui, &engine);
    ui.press("enter");
    ui.wait("the second comment", |ui| ui.comments(&diff).len() == 2);

    let sent = sent_is(&mut ui, &stack, &["Please Fix the test."]);
    let from = &sent[0].from;
    assert_eq!((from.field, from.workspace.as_deref()), (Field::DiffComment, Some(workspace.as_str())));
}

#[gpui_kit::test]
fn an_answer_dictated_is_kept_and_one_only_typed_is_not(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    keep_in_its_folder(&mut ui, &stack);
    ui.ask("ask:First?=One/Two");
    ui.click("question-other");
    ui.input("Only typed");
    ui.press("enter");
    ui.wait("the first answer", |ui| ui.told_to_agent().contains("Only typed"));
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));

    ui.ask("ask:What next?=One/Two");
    ui.click("question-other");
    ui.input("Please");
    dictate(&mut ui, &engine);
    ui.press("enter");

    ui.wait("the answer", |ui| ui.told_to_agent().contains("Please Fix the test."));
    let sent = sent_is(&mut ui, &stack, &["Please Fix the test."]);
    let from = &sent[0].from;
    assert_eq!((from.field, from.chat.clone()), (Field::Answer, Some(ui.active_chat().to_string())));
    assert!(from.workspace.is_some());
}

#[gpui_kit::test]
fn a_proposals_note_dictated_is_kept(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let engine = ready(&mut ui);
    keep_in_its_folder(&mut ui, &stack);
    in_the_note(&mut ui);
    ui.input("Please");
    dictate(&mut ui, &engine);
    ui.click("send-proposal-answer");

    let sent = sent_is(&mut ui, &stack, &["Please Fix the test."]);
    let from = &sent[0].from;
    assert_eq!((from.field, from.chat.clone()), (Field::ProposalNote, Some(ui.active_chat().to_string())));
}

#[gpui_kit::test]
fn nothing_sent_is_kept_for_what_wasnt_dictated_was_let_go_of_turned_off_or_deleted(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    // Dictated while it was off.
    dictate(&mut ui, &engine);
    keep_in_its_folder(&mut ui, &stack);
    send(&mut ui, "say:dictated while off");

    // Let go of before it dictated.
    hold_space_to_listen(&mut ui);
    say(&mut ui, &engine, "fix the test");
    let_go(&mut ui);
    send(&mut ui, "say:a short press");

    // Turned off, and on again.
    dictate(&mut ui, &engine);
    keep(&mut ui, false);
    keep(&mut ui, true);
    send(&mut ui, "say:turned off since");
    dictate(&mut ui, &engine);
    ui.wait("it to go in", |ui| !ui.composer_text().is_empty());
    send(&mut ui, "");
    sent_is(&mut ui, &stack, &["Fix the test."]);

    // Deleted.
    dictate(&mut ui, &engine);
    ui.wait("it to go in", |ui| !ui.composer_text().is_empty());
    open_voice_settings(&mut ui);
    ui.scroll("settings-body", ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    ui.wait("how much room they take", |ui| ui.has(RECORDINGS_STATUS));
    ui.click(DELETE_RECORDINGS);
    ui.click(SharedString::from(format!("{DELETE_RECORDINGS}-act")));
    ui.wait("them to go", |ui| !ui.has(RECORDINGS_STATUS));
    assert!(!folder(&stack).exists());
    ui.press("escape");
    keep_in_its_folder(&mut ui, &stack);
    send(&mut ui, "say:deleted since");

    dictate(&mut ui, &engine);
    ui.wait("it to go in", |ui| !ui.composer_text().is_empty());
    send(&mut ui, "");
    sent_is(&mut ui, &stack, &["Fix the test."]);
}

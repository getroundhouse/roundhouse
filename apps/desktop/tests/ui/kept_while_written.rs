//! A comment being written on the diff, and what's chosen and typed in answer to a question, which the hub keeps until
//! they're saved or sent, so they're there when the app opens again, and in any other client.

#![cfg(test)]

use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::model::{AnswerDraft, AnswerSoFar, CommentDraft, DiffSide, SessionStatus, WorkspaceId};

use crate::support::{Diff, Stack, Ui};

/// The comment the hub keeps as being written on the workspace's diff.
fn kept_comment(stack: &Stack, id: &WorkspaceId) -> Option<CommentDraft> {
    let workspaces = stack.client().workspace_list(false).unwrap();
    workspaces.into_iter().find(|w| &w.id == id).and_then(|w| w.comment_draft)
}

/// What's in the box on the diff, as the hub would keep it.
fn writing(ui: &Ui<'_>, diff: &Diff) -> Option<CommentDraft> {
    ui.cx.read_entity(&diff.file, desktop::file_diff::FileDiff::writing)
}

/// Opens a changed file's diff in the workspace showing, `a.txt`.
fn open_a_diff(ui: &mut Ui<'_>) -> Diff {
    let workspace = ui.view().selected.clone().unwrap();
    std::fs::write(ui.worktree(&workspace).join("a.txt"), "one\ntwo\n").unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");
    diff
}

fn id(name: &str) -> SharedString {
    SharedString::from(name.to_string())
}

/// What's chosen and typed in answer to the question showing.
fn so_far(ui: &Ui<'_>) -> Option<AnswerDraft> {
    let transcript = ui.transcript()?;
    ui.cx.read_entity(&transcript, |t, cx| t.question().and_then(|panel| panel.read(cx).so_far(cx)))
}

fn typed_in_answer(draft: Option<&AnswerDraft>) -> Option<&str> {
    Some(draft?.answers.first()?.typed.as_str())
}

/// Has the agent ask a question with two options and a row to type another answer.
fn ask(ui: &mut Ui<'_>) {
    ui.input("ask:Which day?=Monday/Tuesday");
    ui.press("enter");
    ui.wait("the question", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    ui.next_open_approval();
    ui.wait("the question to show", |ui| ui.has(id("question")));
}

#[gpui_kit::test]
fn a_comment_being_written_is_there_when_the_app_opens_again_until_its_saved(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    let diff = open_a_diff(&mut ui);
    ui.start_comment(&diff, "a.txt", 2);
    ui.input("Why this word?");
    ui.wait("the hub to keep it", |_| {
        kept_comment(&stack, &workspace).is_some_and(|kept| kept.text == "Why this word?")
    });

    ui.open_again(&stack);

    let diff = open_a_diff(&mut ui);
    ui.wait("the comment being written", |ui| writing(ui, &diff).is_some_and(|w| w.text == "Why this word?"));
    assert_eq!(writing(&ui, &diff).unwrap().line, 2, "where it was left");
    ui.cx.update_window(ui.handle, |_, window, cx| diff.file.update(cx, |f, cx| f.save_draft(window, cx))).unwrap();
    ui.wait("saving it to clear what's kept", |_| kept_comment(&stack, &workspace).is_none());
}

#[gpui_kit::test]
fn a_comment_changed_elsewhere_shows_in_the_box_and_goes_when_its_put_away_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    let diff = open_a_diff(&mut ui);
    ui.start_comment(&diff, "a.txt", 2);
    ui.input("Why this word?");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    let changed = CommentDraft { text: "Why not another?".to_string(), ..writing(&ui, &diff).unwrap() };

    stack.client().workspace_comment_draft(&workspace, Some(&changed)).unwrap();

    ui.wait("what was typed elsewhere", |ui| writing(ui, &diff).as_ref() == Some(&changed));
    stack.client().workspace_comment_draft(&workspace, None).unwrap();
    ui.wait("the box to go", |ui| writing(ui, &diff).is_none());
}

#[gpui_kit::test]
fn the_box_being_typed_in_keeps_the_focus_as_a_change_made_elsewhere_moves_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    let diff = open_a_diff(&mut ui);
    ui.start_comment(&diff, "a.txt", 2);
    ui.input("Why this word?");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    let changed = CommentDraft { line: 1, text: "Why not another?".to_string(), ..writing(&ui, &diff).unwrap() };

    stack.client().workspace_comment_draft(&workspace, Some(&changed)).unwrap();
    ui.wait("what was typed elsewhere", |ui| writing(ui, &diff).as_ref() == Some(&changed));

    ui.input(" Or this?");
    assert_eq!(writing(&ui, &diff).unwrap().text, "Why not another? Or this?", "typing goes on in the box");
}

#[gpui_kit::test]
fn a_diff_opened_after_a_comment_was_written_elsewhere_opens_its_box(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    // Hidden in every workspace, so the next one's diff is made only as it's shown.
    if ui.changes_open() {
        ui.click("toggle-diff");
    }
    let workspace = ui.new_workspace();
    std::fs::write(ui.worktree(&workspace).join("a.txt"), "one\ntwo\n").unwrap();
    let pane = ui.pane().unwrap();
    assert!(ui.cx.read_entity(&pane, |p, _| p.diff.is_none()), "no diff until it's shown");
    let kept = CommentDraft {
        path: "a.txt".to_string(),
        side: DiffSide::New,
        line: 2,
        editing: None,
        text: "Why?".to_string(),
    };

    stack.client().workspace_comment_draft(&workspace, Some(&kept)).unwrap();
    let heard = |ui: &mut Ui<'_>| ui.cx.read_entity(&pane, |p, _| p.workspace().comment_draft.is_some());
    ui.wait("the workspace's record to say it", heard);

    let diff = open_a_diff(&mut ui);
    ui.wait("the box the comment was left in", |ui| writing(ui, &diff).as_ref() == Some(&kept));
}

#[gpui_kit::test]
fn going_to_the_typing_row_doesnt_undo_an_answer_changed_elsewhere(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ask(&mut ui);
    let transcript = ui.transcript().unwrap();
    let question = ui.cx.read_entity(&transcript, |t, cx| t.question().unwrap().read(cx).approval_id().clone());
    // Choosing nothing, so there's nothing to keep.
    ui.click(id("question-other"));

    let elsewhere = AnswerSoFar { picked: vec![1], typed: String::new(), other: false };
    let changed = AnswerDraft { approval_id: question, answers: vec![elsewhere] };
    stack.client().session_answer_draft(&chat, Some(&changed)).unwrap();

    ui.wait("what was chosen elsewhere", |ui| so_far(ui).as_ref() == Some(&changed));
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    let kept = stack.client().session_get(&chat).unwrap().answer_draft;
    assert_eq!(kept.as_ref(), Some(&changed), "nothing from here replaced it");
}

#[gpui_kit::test]
fn whats_typed_in_answer_is_there_when_the_app_opens_again_until_its_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ask(&mut ui);
    ui.click(id("question-other"));
    ui.input("Friday");
    let kept = || stack.client().session_get(&chat).unwrap().answer_draft;
    ui.wait("the hub to keep it", |_| typed_in_answer(kept().as_ref()) == Some("Friday"));

    ui.open_again(&stack);

    ui.wait("the question", |ui| ui.has(id("question")));
    ui.wait("what was typed in answer", |ui| typed_in_answer(so_far(ui).as_ref()) == Some("Friday"));
    ui.click(id("question-send"));
    ui.wait("sending it to clear what's kept", |_| kept().is_none());
}

#[gpui_kit::test]
fn an_answer_changed_elsewhere_shows_in_the_question(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ask(&mut ui);
    ui.click(id("question-answer-0"));
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    let question = so_far(&ui).unwrap().approval_id;
    let elsewhere = AnswerSoFar { picked: Vec::new(), typed: "Friday".to_string(), other: true };

    let changed = AnswerDraft { approval_id: question, answers: vec![elsewhere] };
    stack.client().session_answer_draft(&chat, Some(&changed)).unwrap();

    ui.wait("what was typed elsewhere", |ui| so_far(ui).as_ref() == Some(&changed));
}

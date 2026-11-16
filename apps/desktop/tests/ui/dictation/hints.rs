//! Passing each field's surrounding workspace, repository, chat and question terms as hint words when dictating.

use client_core::session_log::Row;
use dictation::fake::FakeEngine;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::event::EventBody;
use protocol::model::SessionStatus;

use super::support::{hearing, hold_option_space, let_go, ready_with, say};
use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn composer_dialog_and_question_answers_pass_their_surrounding_terms_as_hints(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let engine = ready_with(&mut ui, FakeEngine::new(&["use", "sqlite", "in", "tokio"]));
    let repo_name = ui.view().records().repos()[0].name.clone();

    // 1. New-workspace dialog hints the repository's name, with neither SQLite nor Tokio in context yet.
    ui.open_new_workspace();
    ui.click("new-workspace-writing");
    hold_option_space(&mut ui);
    let dialog = ui.dialog();
    let audio = engine.saying("use sqlite in tokio.");
    ui.cx.update(|cx| dialog.read(cx).hear(&audio, cx));
    ui.wait("the dialog's live text", |ui| ui.has("dictation-live"));
    let_go(&mut ui);
    ui.wait("the dialog's dictated text", |ui| ui.dialog_text() == "Use sqlite in tokio.");
    assert!(engine.last_hints().words().iter().any(|h| h == &repo_name));
    ui.press("escape");
    ui.wait_out_the_dialog_going();

    // 2. Workspace composer hints the workspace, repository and identifiers from the chat (`Tokio`).
    ui.new_workspace();
    let ws_name = ui.view().records().workspaces().last().unwrap().name.clone();
    ui.input("say:Check `Tokio` runtime");
    ui.press("enter");
    ui.wait("the transcript to catch up with the reply", |ui| {
        ui.session_status() == Some(SessionStatus::Idle)
            && ui.transcript().is_some_and(|t| {
                ui.cx.read_entity(&t, |t, _| {
                    let log = t.log();
                    (0..log.row_count())
                        .any(|i| matches!(log.row(i), Row::Event(e) if matches!(e.body, EventBody::TurnEnded { .. })))
                })
            })
    });
    ui.click("dictate");
    say(&mut ui, &engine, "use sqlite in tokio.");
    hearing(&mut ui, "use");
    ui.click(dictation_ui::STOP);
    ui.wait("composer text with Tokio hinted", |ui| ui.composer_text() == "Use sqlite in Tokio.");
    let composer_hints = engine.last_hints();
    assert!(composer_hints.words().iter().any(|h| h == &ws_name));
    assert!(composer_hints.words().iter().any(|h| h == "Tokio"));

    // 3. Question answer hints the question's options (`SQLite`, `Postgres`) alongside the chat and workspace.
    ui.press("secondary-a");
    ui.press("backspace");
    ui.ask("ask:Database?=SQLite/Postgres");
    ui.click("question-other");
    hold_option_space(&mut ui);
    let transcript = ui.transcript().unwrap();
    let answer_audio = engine.saying("use sqlite in tokio.");
    ui.cx.update(|cx| transcript.read(cx).question().unwrap().read(cx).hear(&answer_audio, cx));
    ui.wait("the answer's live text", |ui| ui.has("dictation-live"));
    let_go(&mut ui);
    ui.wait("question answer with both SQLite and Tokio hinted", |ui| {
        ui.cx
            .read_entity(&transcript, |t, cx| t.question().and_then(|p| p.read(cx).so_far(cx)))
            .is_some_and(|draft| draft.answers[0].typed == "Use SQLite in Tokio.")
    });
    assert!(engine.last_hints().words().iter().any(|h| h == "Postgres"));
}

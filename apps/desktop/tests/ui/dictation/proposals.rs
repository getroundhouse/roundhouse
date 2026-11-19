//! A proposal's note is dictated into as a question's written answer is: Send waits for the last words and sends them
//! with the answer, and the tab going out of sight ends the recording, keeping what went in.

use desktop::workspace_pane::Showing;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use transcript::ProposalView;

use super::support::{hearing, hold_option_space, let_go, live, ready, say};
use crate::proposals::a_proposal;
use crate::subagents::showing;
use crate::support::{Stack, Ui};
use crate::tools::in_a_new_workspace;

/// Opens the proposal the agent makes in its tab, chooses its first option, and puts the keys in its note. The view.
fn in_the_note(ui: &mut Ui<'_>) -> Entity<ProposalView> {
    let open = a_proposal(ui);
    ui.click(open);
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    ui.click("choose-0-0");
    ui.click("input-state");
    ui.cx.read_entity(&ui.pane().unwrap(), |pane, _| pane.proposal_tab_view()).unwrap()
}

#[gpui_kit::test]
fn a_dictated_note_goes_with_the_answer_once_its_last_words_are_in(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let engine = ready(&mut ui);
    in_the_note(&mut ui);
    let chat = ui.active_chat();
    ui.input("Please");

    hold_option_space(&mut ui);
    say(&mut ui, &engine, "fix the test. / in hub");
    hearing(&mut ui, "in");
    assert!(!ui.has("send-proposal-answer"), "the recording's controls in place of Send");
    let workspace = ui.view().records().workspaces().last().unwrap().name.clone();
    assert!(engine.last_hints().words().contains(&workspace), "the workspace's words, from the pane");
    ui.press("enter");

    let answered = |ui: &mut Ui<'_>| ui.view().feed().unwrap().snapshot().proposals(&chat)[0].current.choice().cloned();
    ui.wait("the answer's record", |ui| answered(ui).is_some());
    assert_eq!(answered(&mut ui).unwrap().note, "Please Fix the test. In hub.", "with its last words");
    let_go(&mut ui);
    assert!(live(&mut ui).is_none());
}

#[gpui_kit::test]
fn leaving_the_tab_ends_the_notes_recording_and_keeps_only_what_went_in(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let engine = ready(&mut ui);
    let view = in_the_note(&mut ui);

    hold_option_space(&mut ui);
    say(&mut ui, &engine, "fix the test. / in hub");
    hearing(&mut ui, "in");
    ui.click("proposal-band-chat");
    let_go(&mut ui);

    assert_eq!(showing(&ui), Showing::Chat);
    assert!(!ui.cx.read_entity(&view, ProposalView::dictating), "nothing dictated out of sight");
    assert_eq!(ui.cx.read_entity(&view, ProposalView::note_text), "Fix the test.");
}

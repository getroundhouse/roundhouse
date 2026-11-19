//! What follows a proposal besides an answer: the person asks for another round, the agent revises it, and the agent
//! withdraws one it won't use, each through the real hub and the scripted agent, and each drawn in the chat and the
//! tab.

#![cfg(test)]

use desktop::workspace_pane::Showing;
use gpui_kit::{ElementId, SharedString, TestAppContext};
use protocol::model::OptionPlace;

use crate::proposals::a_proposal;
use crate::subagents::showing;
use crate::support::{Stack, Ui};
use crate::tools::in_a_new_workspace;

/// Another round, with a note that says what to change, is asked for without choosing: the tab says the agent is
/// to write the next revision, and the chat draws the prompt that told the agent as a reply.
#[gpui_kit::test]
fn another_round_is_asked_for_with_a_note_and_drawn_in_the_chat_as_a_reply(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_proposal(&mut ui);
    let chat = ui.active_chat();
    ui.click(open);
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    assert!(ui.off("another-round"), "nothing's written yet");

    ui.click("proposal-note");
    ui.input("Could the board own it?");
    ui.wait("Another round, once the note says something", |ui| !ui.off("another-round"));
    ui.click("another-round");

    let asked = |ui: &mut Ui<'_>| ui.view().feed().unwrap().snapshot().proposals(&chat)[0].current.response.clone();
    ui.wait("the request's record", |ui| asked(ui).is_some());
    let Some(protocol::model::Response::AnotherRound(round)) = asked(&mut ui) else { panic!("{:?}", asked(&mut ui)) };
    assert_eq!(round.note, "Could the board own it?");
    ui.wait("the banner that says the next revision comes there", |ui| ui.has("proposal-another-round"));
    assert!(!ui.has("choose-0-0"), "nothing to choose once another round is asked for");
    ui.click("proposal-band-chat");
    ui.wait("the chat", |ui| showing(ui) == Showing::Chat);
    let replied = "Replied Holding a train: Another round, with a note";
    ui.wait("the reply's line", |ui| !ui.labelled(replied).is_empty());
    ui.wait("its mark in the outline", |ui| !ui.labelled("Replied Holding a train").is_empty());
}

/// The agent revises the proposal by its ID: the call that revised it has a line of its own, the first's line says
/// the second replaced it, and the tab shows the second, what changed first, with the letter the agent kept.
#[gpui_kit::test]
fn a_revision_has_a_line_of_its_own_and_the_tab_shows_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    a_proposal(&mut ui);
    let chat = ui.active_chat();
    let id = ui.view().feed().unwrap().snapshot().proposals(&chat)[0].id.clone();
    let revise = format!(
        r#"mcp:revise_proposal {{"proposal":"{id}","changed":"Dropped B.","title":"Holding a train","context":"Trains wait a minute.","questions":[{{"header":"Holding","question":"Where does the wait live?","options":[{{"label":"The signal"}},{{"label":"The board","letter":"C"}}],"recommended":"C"}}]}}"#
    );

    ui.input(&format!("{revise}; say:revised"));
    ui.press("enter");

    ui.wait_idle_with("the turn to finish", |text| text.contains("revised"));
    ui.wait("the revision's line", |ui| ui.has(SharedString::from(format!("revised/{id}/2"))));
    let replaced = "Proposed Holding a train: Revision 1 · replaced by revision 2";
    ui.wait("the first's line says the second replaced it", |ui| !ui.labelled(replaced).is_empty());
    ui.click(SharedString::from(format!("open-proposal/{id}/2")));
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    let outline = |part: u64| ElementId::NamedInteger("proposal-outline".into(), part);
    // What changed, the context, the question, its two options and the recommendation.
    ui.wait("a part for what changed, besides the rest", |ui| ui.has(outline(5)) && !ui.has(outline(6)));
    let snapshot = ui.view().feed().unwrap().snapshot();
    let question = &snapshot.proposals(&chat)[0].current.proposed.questions[0];
    assert_eq!(question.letter(OptionPlace(1)).map(|letter| letter.to_string()).as_deref(), Some("C"));
}

/// The agent withdraws a proposal it won't use: the call has a line of its own that says why, the proposal's line says
/// it's withdrawn, and the tab says why, with nothing to choose and no answer bar.
#[gpui_kit::test]
fn a_withdrawn_proposal_says_why_and_takes_no_answer(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    a_proposal(&mut ui);
    let chat = ui.active_chat();
    let id = ui.view().feed().unwrap().snapshot().proposals(&chat)[0].id.clone();
    let withdraw =
        format!(r#"mcp:withdraw_proposal {{"proposal":"{id}","why":"Dispatch decides where holds live now."}}"#);

    ui.input(&format!("{withdraw}; say:withdrawn"));
    ui.press("enter");

    ui.wait_idle_with("the turn to finish", |text| text.contains("withdrawn"));
    ui.wait("the withdrawal's line", |ui| ui.has(SharedString::from(format!("withdrew/{id}"))));
    let why = "Withdrew Holding a train: Dispatch decides where holds live now.";
    assert!(!ui.labelled(why).is_empty(), "it says why");
    let proposed = |ui: &mut Ui<'_>| ui.labelled("Proposed Holding a train: 2 options · withdrawn").is_empty();
    ui.wait("the proposal's line says it's withdrawn", |ui| !proposed(ui));
    ui.click(SharedString::from(format!("open-withdrawn/{id}")));
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    ui.wait("the banner that says why", |ui| ui.has("proposal-withdrawn"));
    assert!(!ui.has("choose-0-0") && !ui.has("proposal-answer"), "nothing to choose, and no answer bar");
}

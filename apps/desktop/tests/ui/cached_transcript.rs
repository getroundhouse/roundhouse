//! The transcript is cached, so a frame draws it again only once it's been told it changed. What the pane passes on
//! that changes how its rows are drawn tells it, such as what the run going claims and what a run of the chat's kind
//! claims.

#![cfg(test)]

use client_core::fake_hub::events::{first_turn, text, user_prompt};
use gpui_kit::{AppContext as _, Context, Entity, TestAppContext};
use protocol::capability::Capabilities;
use protocol::model::{RunCapabilities, RunId, SessionStatus};
use transcript::Transcript;

use crate::support::{Ui, one_chat, open_on_the_chat};

/// Whether the frame after `change` draws any of the transcript's rows.
fn draws_rows(
    ui: &mut Ui<'_>,
    transcript: &Entity<Transcript>,
    change: impl FnOnce(&mut Transcript, &mut Context<Transcript>),
) -> bool {
    let drawn = |ui: &Ui<'_>| ui.cx.read_entity(transcript, |t, _| t.list().rows_drawn());
    let before = drawn(ui);
    ui.cx.update_entity(transcript, change);
    // And what's around it, so the window draws whether or not the transcript said it changed.
    let pane = ui.pane().unwrap();
    ui.cx.update_entity(&pane, |_, cx| cx.notify());
    ui.frame();
    drawn(ui) > before
}

#[gpui_kit::test]
fn what_changes_how_its_rows_are_drawn_draws_the_transcript_again(cx: &mut TestAppContext) {
    let events = first_turn(user_prompt("Keep the ledger."), [text("msg-0", "The ledger is kept.")]);
    let (world, chat) = one_chat(SessionStatus::Idle, events);
    let mut session = world.session(&chat).unwrap().clone();
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    let transcript = ui.transcript().unwrap();
    ui.wait("every row measured", |ui| ui.cx.read_entity(&transcript, |t, _| t.list().rows_to_measure()) == 0);
    assert!(!draws_rows(&mut ui, &transcript, |_, _| {}), "drawn again with nothing changed, so not cached");

    let claims = Capabilities::default();
    assert_ne!(session.agent_capabilities.as_ref().map(|run| &*run.claims), Some(&claims));
    session.agent_capabilities = Some(RunCapabilities::new(RunId::new(1), claims.clone()));
    assert!(draws_rows(&mut ui, &transcript, |t, cx| t.set_session(&session, cx)), "the run's claims");

    let mut agent = protocol::fixtures::claude();
    draws_rows(&mut ui, &transcript, |t, cx| t.set_agent(Some(&agent), cx));
    assert_ne!(agent.capabilities, claims);
    agent.capabilities = claims;
    assert!(draws_rows(&mut ui, &transcript, |t, cx| t.set_agent(Some(&agent), cx)), "the kind's claims");
}

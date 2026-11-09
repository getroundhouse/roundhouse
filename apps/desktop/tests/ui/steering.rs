//! Steering: Enter sends what's typed to an agent mid-turn, and the prompt waits until the agent takes it.

#![cfg(test)]

use client_core::session_log::PromptState;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::event::EventBody;
use protocol::model::SessionStatus;

use crate::queue::queued;
use crate::support::{Stack, Ui};

/// A workspace whose agent is running a tool for a while.
pub(crate) fn working(ui: &mut Ui, ms: u64) {
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input(&format!("hang:{ms}"));
    ui.press("enter");
    ui.wait("the agent to start working", |ui| ui.session_status() == Some(SessionStatus::Running));
}

#[gpui_kit::test]
fn enter_while_the_agent_works_steers_it_and_the_prompt_waits_until_its_taken(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    working(&mut ui, 4000);

    // Its own tool, so the agent is still working once it has taken this.
    ui.input("hang:2000; say:steered");
    ui.press("enter");

    let transcript = ui.transcript().unwrap();
    let row = |ui: &Ui| {
        ui.events()
            .iter()
            .position(|e| matches!(&e.body, EventBody::UserPrompt { text, .. } if text.ends_with("steered")))
    };
    ui.wait("the prompt to be recorded", |ui| row(ui).is_some());
    let at = row(&ui).unwrap();
    let state = |ui: &Ui| ui.cx.read_entity(&transcript, |t, _| t.prompt_state_at(at));
    assert_eq!(state(&ui), Some(PromptState::Waiting));
    assert!(queued(&ui).is_empty(), "steered, not queued");
    ui.wait("the agent to take it", |ui| state(ui) == Some(PromptState::Taken));
    let ended = ui.events().iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. }));
    assert!(!ended, "taken because the agent said so, not because the turn is over");
    ui.wait_idle_with("the steered answer", |text| text.contains("steered"));
}

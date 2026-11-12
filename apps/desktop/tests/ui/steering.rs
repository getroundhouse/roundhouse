//! Steering: Enter sends what's typed to an agent mid-turn, and the prompt waits until the agent takes it.

#![cfg(test)]

use client_core::session_log::PromptState;
use gpui_kit::{AppContext as _, Pixels, TestAppContext};
use protocol::event::EventBody;
use protocol::model::SessionStatus;

use crate::queue::queued;
use crate::support::{Stack, Ui, top};

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

/// How far down the window the lowest of `part`'s elements was drawn.
fn lowest(ui: &mut Ui, part: &str) -> Pixels {
    let drawn = ui.shown(part);
    drawn.into_iter().map(|id| top(ui, id)).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap()
}

/// Whether the prompt sent last is drawn under what the agent thought.
fn under_the_thought(ui: &mut Ui) -> bool {
    lowest(ui, "bubble/") > lowest(ui, "thinking/")
}

/// The thought began before the prompt came, so it's recorded after it, but the agent reads the prompt only after
/// the thought, and the prompt is drawn under it, at the end of the chat while it waits and where it was taken after.
/// It was drawn above the thought, where it was recorded, and a long thought hid it.
#[gpui_kit::test]
fn a_prompt_sent_while_the_agent_thinks_is_drawn_under_the_thought(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let transcript = ui.transcript().unwrap();
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    ui.input("think:30:idea");
    ui.press("enter");
    let thinking = |ui: &Ui, so_far: &str| ui.cx.read_entity(&transcript, |t, _| t.streaming_text().contains(so_far));
    ui.wait("the thought to stream", |ui| thinking(ui, "idea3"));

    ui.input("say:steered");
    ui.press("enter");

    assert!(under_the_thought(&mut ui), "as soon as it's sent, under the thought as it streams");
    let recorded = |ui: &Ui| {
        let events = ui.events();
        let prompt =
            events.iter().position(|e| matches!(&e.body, EventBody::UserPrompt { text, .. } if text == "say:steered"));
        prompt.map(|row| ui.cx.read_entity(&transcript, |t, _| t.prompt_state_at(row)))
    };
    ui.wait("the prompt to be recorded", |ui| recorded(ui).is_some());
    assert_eq!(recorded(&ui), Some(Some(PromptState::Waiting)));
    assert!(thinking(&ui, "idea"), "still thinking");
    assert!(under_the_thought(&mut ui), "recorded, and waiting under the thought as it streams");
    ui.wait("the agent to take it", |ui| recorded(ui) == Some(Some(PromptState::Taken)));
    assert!(under_the_thought(&mut ui), "taken, after the thought");
    ui.wait_idle_with("the steered answer", |text| text.contains("steered"));
    assert!(under_the_thought(&mut ui), "and once the turn is over");
}

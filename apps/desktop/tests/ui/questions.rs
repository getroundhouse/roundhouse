//! Questions from the agent.

#![cfg(test)]

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::event::EventBody;
use protocol::model::SessionStatus;

use crate::support::{Stack, Ui};

/// The result of the agent's question, from what it was told.
fn told_to_agent(ui: &Ui) -> String {
    let mut told = String::new();
    for event in ui.events() {
        if let EventBody::AgentUser { blocks, .. } = &event.body {
            for block in blocks {
                if let protocol::event::ContentBlock::ToolResult { text, .. } = block {
                    told.push_str(text);
                }
            }
        }
    }
    told
}

fn open_question(ui: &mut Ui, prompt: &str) -> u64 {
    ui.input(prompt);
    ui.press("enter");
    ui.wait("the question", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    ui.next_open_approval()
}

#[gpui_kit::test]
fn a_question_is_answered_by_clicking_or_typing_or_skipped(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();

    // One question with one answer needs only the click.
    let seq = open_question(&mut ui, "ask:Red or blue?=Red/Blue");
    assert!(
        ui.cx
            .update_window(ui.handle, |_, w, _| w.try_find(SharedString::from(format!("send-answers-{seq}"))).is_none())
            .unwrap()
    );
    ui.click(SharedString::from(format!("answer-{seq}-0-1")));
    ui.wait("the answer", |ui| told_to_agent(ui).contains(r#""Red or blue?"=Blue"#));
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));

    // Text typed while it's waiting answers it, rather than going to the agent as a new message.
    let seq = open_question(&mut ui, "ask:Which day?=Monday/Tuesday");
    ui.input("Friday, if that works");
    ui.press("enter");
    ui.wait("the typed answer", |ui| told_to_agent(ui).contains(r#""Which day?"=Friday, if that works"#));
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    assert_eq!(ui.composer_text(), "");
    let prompts = ui.events().iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(prompts, 2, "the answer wasn't sent as a prompt too (seq {seq})");

    // Skipping lets the agent know, and once it's over, typing is a new message again.
    let seq = open_question(&mut ui, "ask:Tea or coffee?=Tea/Coffee");
    ui.click(SharedString::from(format!("skip-question-{seq}")));
    ui.wait("the skip", |ui| told_to_agent(ui).contains("chose not to answer"));
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    ui.input("say:a new message");
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text.contains("a new message"));
}

#[gpui_kit::test]
fn several_questions_wait_until_they_are_all_answered(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();

    let seq = open_question(&mut ui, "ask:*Which colors?=Red/Green/Blue|How many?=One/Two");
    let send = format!("send-answers-{seq}");

    // Several can be chosen for the first, and choosing one again takes it back.
    ui.click(SharedString::from(format!("answer-{seq}-0-0")));
    ui.click(SharedString::from(format!("answer-{seq}-0-1")));
    ui.click(SharedString::from(format!("answer-{seq}-0-2")));
    ui.click(SharedString::from(format!("answer-{seq}-0-1")));
    // Not answered yet, so pressing send does nothing.
    ui.click(SharedString::from(send.clone()));
    ui.frame();
    assert_eq!(ui.session_status(), Some(SessionStatus::WaitingApproval));
    assert_eq!(told_to_agent(&ui), "");

    // One answer allowed for the second, so a later choice replaces an earlier one.
    ui.click(SharedString::from(format!("answer-{seq}-1-0")));
    ui.click(SharedString::from(format!("answer-{seq}-1-1")));
    ui.click(SharedString::from(send));
    ui.wait("the answers", |ui| told_to_agent(ui).contains(r#""How many?"=Two"#));
    assert!(told_to_agent(&ui).contains(r#""Which colors?"=Red, Blue"#), "{}", told_to_agent(&ui));
}

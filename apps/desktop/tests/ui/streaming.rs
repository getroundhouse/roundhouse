//! What a session shows as it runs.

#![cfg(test)]

use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, TestAppContext};
use protocol::event::EventBody;
use protocol::model::{SessionStatus, seq_index};

use crate::support::{Stack, TIMEOUT, Ui};

#[gpui_kit::test]
fn text_streams_in_as_it_is_written(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("slow:20:word");
    ui.press("enter");
    ui.wait("streaming text", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().contains("word5")))
    });
    assert_eq!(ui.session_status(), Some(SessionStatus::Running));
    ui.wait_idle_with("the finished message", |text| text.contains("word19"));
    assert!(ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().is_empty())));
}

#[gpui_kit::test]
fn the_stop_button_stops_the_turn(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("say:go; hang:60000");
    ui.press("enter");
    ui.wait("the agent to start", |ui| ui.assistant_text().contains("go"));
    ui.wait("the Stop button", |ui| {
        ui.session_status() == Some(SessionStatus::WaitingApproval)
            || ui.session_status() == Some(SessionStatus::Running)
    });
    ui.click("stop");
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    // The session's status and its log arrive separately, in either order.
    ui.wait("the cancelled turn in the log", |ui| {
        ui.events()
            .iter()
            .any(|e| matches!(e.body, EventBody::TurnEnded { outcome: protocol::event::TurnOutcome::Cancelled, .. }))
    });
}

#[gpui_kit::test]
fn long_history_loads_a_page_at_a_time_as_it_is_scrolled_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let workspace = client.workspace_create(&repo.id, None, None, None).unwrap();
    let session = client
        .session_create(&workspace.id, protocol::model::AgentKind::Claude, None, Some("bypassPermissions".into()), None)
        .unwrap();
    let script: Vec<String> = (0..700).map(|i| format!("say:message {i}")).collect();
    client.session_prompt(&session.id, &script.join("; "), None).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while client.session_get(&session.id).unwrap().status != SessionStatus::Idle {
        assert!(Instant::now() < deadline, "the agent never finished");
        std::thread::sleep(Duration::from_millis(20));
    }
    let total = seq_index(client.session_get(&session.id).unwrap().last_seq);
    assert!(total > 700);

    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    ui.wait("the tail to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(total - 1).is_some()));
    // Only the tail so far. The start of the session isn't held.
    let loaded = ui.cx.read_entity(&transcript, |t, _| t.loaded_events());
    assert!(loaded < total, "{loaded} of {total}");
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(0).is_none()));

    let scroller = ui.cx.read_entity(&transcript, |t, _| t.scroller().clone());
    ui.cx.update_entity(&scroller, |list, cx| {
        list.scroll_to_item(0, cx);
    });
    ui.wait("the first page to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(0).is_some()));
    assert!(matches!(ui.cx.read_entity(&transcript, |t, _| t.event_at(0).unwrap().body), EventBody::UserPrompt { .. }));
}

#[gpui_kit::test]
fn empty_prompts_are_not_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.press("enter");
    ui.input("   ");
    ui.press("enter");
    ui.frame();
    assert!(ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.row_count() == 0)));
    // They never reach the hub, so there's nothing to complain about either.
    assert_eq!(ui.view().error, None);
}

#[gpui_kit::test]
fn a_sent_prompt_shows_once_and_becomes_the_recorded_event(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("say:hello");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("hello"));
    let transcript = ui.transcript().unwrap();
    let rows = ui.cx.read_entity(&transcript, |t, _| t.row_count());
    let recorded = ui.events().len();
    assert_eq!(rows, recorded, "nothing is left waiting to be recorded");
    let prompts = ui.events().iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(prompts, 1);
}

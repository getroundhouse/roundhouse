//! What a session shows as it runs.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::{AppContext as _, TestAppContext};
use protocol::event::EventBody;
use protocol::model::{SessionStatus, seq_index};
use transcript::Transcript;

use crate::support::{Stack, Ui, wait_until};

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

/// The reply streams into a text view the transcript keeps, which its row goes on drawing once it's recorded. The
/// rest of it can arrive with the recorded reply, after the stream is gone from the tail, and the text view the row
/// drew then went on showing only what had streamed into it.
#[gpui_kit::test]
fn a_reply_shows_all_of_itself_once_it_is_recorded_however_its_end_arrived(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    stack.client().session_prompt(&session, "slow:6:word", None).unwrap();
    // Drawn, rather than only in the session's log, which a thread of its own fills ahead of what the app has drawn.
    ui.wait("the start of the reply to be drawn", |ui| {
        ui.cx.read_entity(&transcript, Transcript::kept_texts).iter().any(|kept| kept.contains("word0"))
    });

    // The app doesn't run while this waits, so it takes in the rest of the reply and its record in one go. The hub
    // sends the end of the stream as soon as it's written, and the record once it's stored, so the turn is over
    // only once the app's log, which a thread of its own fills, holds the record too.
    wait_until("the turn to end", || stack.client().session_get(&session).unwrap().status == SessionStatus::Idle);
    let recorded = seq_index(stack.client().session_get(&session).unwrap().last_seq);
    wait_until("the record to reach the app", || ui.cx.read_entity(&transcript, |t, _| t.row_count()) >= recorded);
    ui.wait_idle_with("the recorded reply", |text| text.contains("word5"));
    ui.settle(Duration::from_millis(16));

    let reply = ui.assistant_text();
    let shown = ui.cx.read_entity(&transcript, Transcript::kept_texts);
    assert!(shown.contains(&reply), "{reply:?} shows as {shown:?}");
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

#[gpui_kit::test]
fn a_turn_says_how_long_its_taken_until_it_ends_and_then_copies_its_reply(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("slow:20:word");
    ui.press("enter");

    ui.wait("the working line", |ui| ui.has("working-line"));
    assert_eq!(ui.session_status(), Some(SessionStatus::Running));
    ui.wait_idle_with("the finished message", |text| text.contains("word19"));
    assert!(!ui.has("working-line"), "it goes with the turn");
    // The session's status and its log arrive separately, in either order.
    let ended = |ui: &Ui| ui.events().iter().find(|e| matches!(e.body, EventBody::TurnEnded { .. })).map(|e| e.seq);
    ui.wait("the end of the turn in the log", |ui| ended(ui).is_some());
    ui.click(gpui_kit::SharedString::from(format!("copy-reply/event/{}/0", ended(&ui).unwrap())));

    let copied = ui.cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("done"), "what the fake agent says its turn came to");
}

//! A prompt sent while the agent works: it's recorded at once, and the agent takes it when it can.

#![cfg(test)]

use client_core::Client;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Seq, SessionId};

use crate::support::{Stack, assistant_text, read_all, wait_for_turn_end, wait_until};

/// Waits until the agent is running a tool, which is when a prompt sent now has to wait.
pub(crate) fn wait_for_tool(client: &Client, session_id: &SessionId) {
    wait_until("a tool to start", || {
        read_all(client, session_id).iter().any(|e| {
            matches!(&e.body, EventBody::Assistant { blocks, .. }
                if blocks.iter().any(|b| matches!(b, ContentBlock::ToolUse { .. })))
        })
    });
}

fn position(events: &[SessionEvent], wanted: impl Fn(&EventBody) -> bool) -> Option<Seq> {
    events.iter().find(|e| wanted(&e.body)).map(|e| e.seq)
}

#[test]
fn a_prompt_sent_mid_turn_is_taken_only_once_the_running_tool_has_finished() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let (first, _) = client.session_prompt(&session.id, "hang:1500", None).unwrap();
    wait_for_tool(&client, &session.id);

    let (steered, _) = client.session_prompt(&session.id, "say:steered", None).unwrap();
    let meanwhile = read_all(&client, &session.id);
    let events = wait_for_turn_end(&client, &session.id, 1);

    let taken =
        |prompt: Seq| move |body: &EventBody| matches!(body, EventBody::PromptTaken { prompt: p } if *p == prompt);
    assert!(position(&meanwhile, taken(first)).is_some(), "the first prompt was taken as its turn began");
    assert!(position(&meanwhile, taken(steered)).is_none(), "not while the tool runs");
    let finished = position(&events, |b| matches!(b, EventBody::AgentUser { .. })).unwrap();
    let taken_at = position(&events, taken(steered)).expect("taken in the end");
    assert!(taken_at > finished, "after the tool's result");
    assert!(assistant_text(&events).ends_with("steered"));
}

#[test]
fn stopping_drops_a_prompt_the_agent_hadnt_taken() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:20000", None).unwrap();
    wait_for_tool(&client, &session.id);
    let (steered, _) = client.session_prompt(&session.id, "say:never", None).unwrap();

    client.session_cancel(&session.id).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    let dropped = position(&events, |b| matches!(b, EventBody::PromptDropped { prompt } if *prompt == steered));
    assert!(dropped.is_some(), "{events:#?}");
    assert!(position(&events, |b| matches!(b, EventBody::PromptTaken { prompt } if *prompt == steered)).is_none());
    assert!(!assistant_text(&events).contains("never"));
}

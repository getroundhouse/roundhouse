//! The calls an agent makes to tools over MCP, and what they came back with.

use client_core::Client;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::SessionId;

use super::wait_for_turn_end;

/// What each of the turn's calls to a tool came back with, and whether it failed, in order.
pub(crate) fn results(events: &[SessionEvent]) -> Vec<(String, bool)> {
    let blocks = events.iter().filter_map(|event| match &event.body {
        EventBody::AgentUser { blocks, .. } => Some(blocks),
        _ => None,
    });
    blocks
        .flatten()
        .filter_map(|block| match block {
            ContentBlock::CallResult { output, is_error, .. } => {
                Some((client_core::calls::typed::output(output), *is_error))
            }
            _ => None,
        })
        .collect()
}

/// Sends the chat a prompt, and returns what its calls came back with, once it's the `turn`th to end.
pub(crate) fn run(client: &Client, chat: &SessionId, script: &str, turn: usize) -> Vec<(String, bool)> {
    client.session_prompt(chat, script, None).unwrap();
    let events = wait_for_turn_end(client, chat, turn);
    let since = events.iter().rposition(|event| matches!(event.body, EventBody::UserPrompt { .. })).unwrap();
    results(&events[since..])
}

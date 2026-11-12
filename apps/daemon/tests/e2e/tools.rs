//! The worker's tools, which an agent calls over MCP, and the hub answers, here with a pull request from a stand-in
//! `gh`.

#![cfg(test)]

use client_core::Client;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::SessionId;

use crate::support::gh::{OPEN, StandInGh};
use crate::support::{Stack, is_alive, parent_of, started_pid, wait_for_turn_end, wait_until};

/// What each of the turn's calls to a tool came back with, and whether it failed, in order.
fn results(events: &[SessionEvent]) -> Vec<(String, bool)> {
    let blocks = events.iter().filter_map(|event| match &event.body {
        EventBody::AgentUser { blocks, .. } => Some(blocks),
        _ => None,
    });
    blocks
        .flatten()
        .filter_map(|block| match block {
            ContentBlock::ToolResult { text, is_error, .. } => Some((text.clone(), *is_error)),
            _ => None,
        })
        .collect()
}

/// Where the agent's CLI was last told the worker's tools are, and the header that carries its token, from the copy
/// the fake agent keeps. Read rather than compared as text, since a hub started again in a run of every package's tests
/// can be a build with other features, which writes the same file with its keys in another order.
fn last_told(stack: &Stack) -> (String, String) {
    let told: serde_json::Value =
        serde_json::from_slice(&std::fs::read(stack.agent_state().join("last-mcp-config")).unwrap()).unwrap();
    let server = &told["mcpServers"]["roundhouse"];
    (server["url"].as_str().unwrap().to_string(), server["headers"]["Authorization"].as_str().unwrap().to_string())
}

/// Sends the chat a prompt, and returns what its calls came back with, once it's the `turn`th to end.
fn run(client: &Client, chat: &SessionId, script: &str, turn: usize) -> Vec<(String, bool)> {
    client.session_prompt(chat, script, None).unwrap();
    let events = wait_for_turn_end(client, chat, turn);
    let since = events.iter().rposition(|event| matches!(event.body, EventBody::UserPrompt { .. })).unwrap();
    results(&events[since..])
}

#[test]
fn an_agent_asking_where_its_pull_request_stands_hears_what_the_hub_read_from_github() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    gh.say(OPEN);
    let client = stack.client();
    let (_, chat) = stack.workspace(&client);

    let answered = run(&client, &chat.id, "mcp:get_pull_request", 1);

    let [(text, false)] = answered.as_slice() else { panic!("one call that worked: {answered:?}") };
    assert!(text.starts_with("Pull request #7, \"T\""), "{text}");
    assert!(text.contains("https://example.com/7"), "{text}");
}

#[test]
fn an_agent_started_after_the_hub_restarts_reaches_the_tools_where_it_did_with_the_same_token() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    gh.say(OPEN);
    let client = stack.client();
    let (_, chat) = stack.workspace(&client);
    let answered = run(&client, &chat.id, "mcp:get_pull_request", 1);
    assert!(!answered[0].1, "{answered:?}");
    let before = last_told(&stack);
    let worker = parent_of(started_pid(&wait_for_turn_end(&client, &chat.id, 1)).unwrap());

    stack.kill_hub();
    // Its worker stops with it, and its port is the one to have again, though its connections may still be closing.
    wait_until("the worker to exit", || !is_alive(worker));
    stack.start_hub();
    let client = stack.client();
    let answered = run(&client, &chat.id, "mcp:get_pull_request", 2);

    assert_eq!(last_told(&stack), before, "the same address and token");
    let [(text, false)] = answered.as_slice() else { panic!("one call that worked: {answered:?}") };
    assert!(text.starts_with("Pull request #7"), "{text}");
}

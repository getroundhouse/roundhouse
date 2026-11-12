//! The worker's tools, which an agent calls over MCP, and the hub answers, here with a pull request from a stand-in
//! `gh`.

#![cfg(test)]

use client_core::{Client, IdleStop, stop_hub_if_idle};
use protocol::event::{ContentBlock, EventBody, ExitReason, SessionEvent, TaskKind};
use protocol::model::{SessionId, SessionStatus};

use crate::support::gh::{MERGED, OPEN, StandInGh};
use crate::support::{
    Stack, assistant_text, is_alive, latest_started_pid, parent_of, read_all, started_pid, wait_for_status,
    wait_for_turn_end, wait_until,
};

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

    // As when the machine goes down, so a new worker starts, and its port is the one to have again, though its
    // connections may still be closing. A hub that only crashed leaves its worker serving the tools.
    stack.kill_hub_and_workers();
    wait_until("the worker to exit", || !is_alive(worker));
    stack.start_hub();
    let client = stack.client();
    let answered = run(&client, &chat.id, "mcp:get_pull_request", 2);

    assert_eq!(last_told(&stack), before, "the same address and token");
    let [(text, false)] = answered.as_slice() else { panic!("one call that worked: {answered:?}") };
    assert!(text.starts_with("Pull request #7"), "{text}");
}

/// A hub that crashes leaves its worker serving the tools, so the agent the next hub carries on with reaches them as it
/// did, and the next hub answers what the worker asks for it.
#[test]
fn the_agent_the_next_hub_carries_on_with_reaches_the_tools_as_it_did() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    gh.say(OPEN);
    let client = stack.client();
    let (_, chat) = stack.workspace(&client);
    let answered = run(&client, &chat.id, "mcp:get_pull_request", 1);
    assert!(!answered[0].1, "{answered:?}");
    let agent = started_pid(&wait_for_turn_end(&client, &chat.id, 1)).unwrap();
    drop(client);

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    let answered = run(&client, &chat.id, "mcp:get_pull_request", 2);

    let [(text, false)] = answered.as_slice() else { panic!("one call that worked: {answered:?}") };
    assert!(text.starts_with("Pull request #7"), "{text}");
    assert_eq!(latest_started_pid(&read_all(&client, &chat.id)), Some(agent), "the agent the last hub started");
}

/// The call comes back at once, saying it's in the background, and the turn ends with the wait listed there. Once a
/// read finds the pull request merged, the agent picks up again by itself, from what its CLI asked of the task, and
/// hears how the wait ended.
#[test]
fn an_agent_waiting_on_its_pull_request_hears_once_it_has_merged() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    gh.say(OPEN);
    let client = stack.client();
    let (workspace, chat) = stack.workspace(&client);

    client.session_prompt(&chat.id, "mcp:wait_for_pull_request", None).unwrap();
    let mut answered = Vec::new();
    wait_until("the call to come back", || {
        answered = results(&read_all(&client, &chat.id));
        !answered.is_empty()
    });
    let [(text, false)] = answered.as_slice() else { panic!("one call that worked: {answered:?}") };
    assert!(text.contains("moved to the background"), "it was answered with a task: {text}");
    wait_for_status(&client, &chat.id, SessionStatus::Background);
    let waiting = client.session_get(&chat.id).unwrap();
    let listed: Vec<_> = waiting.background_tasks.into_iter().map(|t| (t.description, t.kind)).collect();
    assert_eq!(listed, [(Some("roundhouse/wait_for_pull_request".to_string()), TaskKind::Wait)]);
    assert_eq!(waiting.wait.map(|wait| wait.number), Some(7), "the chat's record has the hub's wait");
    // The next hub keeps the wait, so it doesn't keep the hub from stopping to update it.
    assert_eq!(client.hub_status().unwrap().running_sessions, 0);
    gh.say(MERGED);
    client.workspace_pull_request(&workspace.id).unwrap();

    let events = wait_for_turn_end(&client, &chat.id, 2);
    let said = assistant_text(&events);
    assert!(said.contains("The wait on pull request #7 has ended: it's merged."), "{said}");
    assert_eq!(client.session_get(&chat.id).unwrap().wait, None, "and none once it has ended");
}

/// The person calls the wait off while the agent's CLI runs. It hears as it next asks after its task, takes the task
/// off its list as one that failed, and picks up again by itself, saying what the hub last said of it.
#[test]
fn an_agent_whose_wait_the_person_calls_off_hears_and_the_chat_goes_idle() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    gh.say(OPEN);
    let client = stack.client();
    let (_, chat) = stack.workspace(&client);
    client.session_prompt(&chat.id, "mcp:wait_for_pull_request", None).unwrap();
    wait_for_status(&client, &chat.id, SessionStatus::Background);
    let wait = client.session_get(&chat.id).unwrap().wait.unwrap();

    let called_off = client.session_call_off_wait(&chat.id, &wait.id).unwrap();
    let events = wait_for_turn_end(&client, &chat.id, 2);

    assert_eq!(
        called_off.wait, None,
        "the hub's is over at once, and the CLI takes its copy off the list once it hears"
    );
    let said = assistant_text(&events);
    assert!(said.contains("The person stopped this wait."), "{said}");
    wait_for_status(&client, &chat.id, SessionStatus::Idle);
    assert!(client.session_get(&chat.id).unwrap().background_tasks.is_empty());
}

/// The wait is the hub's, so a hub started again has it, however the last one went, with the chat at work in the
/// background and the wait listed. Once the new hub's own read finds the pull request merged, the agent's CLI hears as
/// it next asks after its task, and says how it ended. A hub that dies leaves the agent running, so the next hub
/// carries on with it. One that stops, or goes with the machine, takes the agent with it, so the next hub starts it
/// again, whose CLI asks after its task as it starts.
fn a_wait_lasts_across_a_restart(stop: impl FnOnce(&mut Stack), carried_on: bool) {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    gh.say(OPEN);
    let client = stack.client();
    let (_, chat) = stack.workspace(&client);
    client.session_prompt(&chat.id, "mcp:wait_for_pull_request", None).unwrap();
    wait_for_status(&client, &chat.id, SessionStatus::Background);
    let agent = started_pid(&read_all(&client, &chat.id)).unwrap();
    drop(client);

    // Held, so no hub reads it until the new one is up, and merged only once the last one has gone, since a read that
    // one began before the hold, as the turn ended, would find it merged and end the wait. The new hub reads it merged
    // by itself, as the first it reads, once it's been looked at.
    gh.hold();
    stop(&mut stack);
    gh.say(MERGED);
    stack.start_hub();
    let client = stack.client();
    let waiting = client.session_get(&chat.id).unwrap();
    let still = (waiting.status, waiting.background_tasks.len(), waiting.queue_held);
    assert_eq!(still, (SessionStatus::Background, 1, false), "nothing it was doing was cut off");
    gh.release();

    let events = wait_for_turn_end(&client, &chat.id, 2);
    let said = assistant_text(&events);
    assert!(said.contains("The wait on pull request #7 has ended: it's merged."), "{said}");
    let went =
        events.iter().any(|event| matches!(event.body, EventBody::AgentExited { reason: ExitReason::HubRestart, .. }));
    let same = latest_started_pid(&events) == Some(agent);
    assert_eq!((went, same), (!carried_on, carried_on), "whether the agent went with the hub");
}

#[test]
fn a_wait_lasts_across_the_hub_dying_and_starting_again() {
    a_wait_lasts_across_a_restart(Stack::kill_hub, true);
}

/// As the machine going down takes the hub, and its workers with it, so the next hub finds the agent gone.
#[test]
fn a_wait_lasts_across_the_hub_going_with_its_workers() {
    a_wait_lasts_across_a_restart(Stack::kill_hub_and_workers, false);
}

/// As `roundhouse hub stop` stops it, with its workers and their agents, which records what they say as they go.
#[test]
fn a_wait_lasts_across_the_hub_stopping_and_starting_again() {
    let stop = |stack: &mut Stack| {
        stack.client().hub_shutdown().unwrap();
        let mut hub = stack.hub.take().unwrap();
        wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
    };
    a_wait_lasts_across_a_restart(stop, false);
}

/// As the app stops an older hub to update it, only if nothing would notice, which a chat whose only work is a wait
/// doesn't.
#[test]
fn a_wait_lasts_across_the_hub_stopping_for_an_update() {
    let stop = |stack: &mut Stack| {
        let stopped = stop_hub_if_idle(&stack.client(), &stack.socket(), stack.hub_pid()).unwrap();
        assert_eq!(stopped, IdleStop::Stopped, "a wait holds up no update");
        stack.hub.take().unwrap().wait().unwrap();
    };
    a_wait_lasts_across_a_restart(stop, false);
}

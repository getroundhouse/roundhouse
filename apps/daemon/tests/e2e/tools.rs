//! The worker's tools, which an agent calls over MCP, and the hub answers, here with a pull request the hub reads from
//! a stand-in for GitHub, and the comments on the workspace's diff.

#![cfg(test)]

use client_core::{Client, IdleStop, stop_hub_if_idle};
use protocol::event::{ContentBlock, EventBody, ExitReason, SessionEvent, TaskKind};
use protocol::model::{DiffSide, SessionId, SessionStatus};

use hub::GithubPullRequestState::{Merged, Open};

use crate::support::github::{StandInGithub, workspace_on_github};
use crate::support::{
    Stack, assistant_text, is_alive, latest_started_pid, parent_of, read_all, started_pid, wait_for_status,
    wait_for_turn_end, wait_until,
};

const READ: &str = r#"mcp:get_pull_requests {"pull_requests":[7]}"#;
const WAIT: &str = r#"mcp:wait_for_pull_requests {"pull_requests":[7],"until":"merged"}"#;

/// What each of the turn's calls to a tool came back with, and whether it failed, in order.
fn results(events: &[SessionEvent]) -> Vec<(String, bool)> {
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
    let github = StandInGithub::start();
    github.say(7, Open);
    let (stack, _gh) = github.stack();
    let client = stack.client();
    let (_, chat) = workspace_on_github(&stack, &client);

    let answered = run(&client, &chat.id, READ, 1);

    let [(text, false)] = answered.as_slice() else { panic!("one call that worked: {answered:?}") };
    assert!(text.contains("#7: Change 7"), "{text}");
    assert!(text.contains("https://github.com/example/project/pull/7"), "{text}");
}

#[test]
fn an_agent_reading_the_diffs_comments_hears_each_one_the_person_left() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, chat) = stack.workspace(&client);
    client.comment_add(&workspace.id, "a.txt", DiffSide::New, 2, "Why this word?", None).unwrap();

    let answered = run(&client, &chat.id, "mcp:get_diff_comments {}", 1);

    let [(text, false)] = answered.as_slice() else { panic!("one call that worked: {answered:?}") };
    assert!(text.contains("- `a.txt:2`: Why this word?"), "{text}");
}

#[test]
fn an_agent_started_after_the_hub_restarts_reaches_the_tools_where_it_did_with_the_same_token() {
    let github = StandInGithub::start();
    github.say(7, Open);
    let (mut stack, _gh) = github.stack();
    let client = stack.client();
    let (_, chat) = workspace_on_github(&stack, &client);
    let answered = run(&client, &chat.id, READ, 1);
    assert!(!answered[0].1, "{answered:?}");
    let before = last_told(&stack);
    let worker = parent_of(started_pid(&wait_for_turn_end(&client, &chat.id, 1)).unwrap());

    // As when the machine goes down, so a new worker starts, and its port is the one to have again, though its
    // connections may still be closing. A hub that only crashed leaves its worker serving the tools.
    stack.kill_hub_and_workers();
    wait_until("the worker to exit", || !is_alive(worker));
    stack.start_hub();
    let client = stack.client();
    let answered = run(&client, &chat.id, READ, 2);

    assert_eq!(last_told(&stack), before, "the same address and token");
    let [(text, false)] = answered.as_slice() else { panic!("one call that worked: {answered:?}") };
    assert!(text.contains("#7: Change 7"), "{text}");
}

/// A hub that crashes leaves its worker serving the tools, so the agent the next hub carries on with reaches them as it
/// did, and the next hub answers what the worker asks for it.
#[test]
fn the_agent_the_next_hub_carries_on_with_reaches_the_tools_as_it_did() {
    let github = StandInGithub::start();
    github.say(7, Open);
    let (mut stack, _gh) = github.stack();
    let client = stack.client();
    let (_, chat) = workspace_on_github(&stack, &client);
    let answered = run(&client, &chat.id, READ, 1);
    assert!(!answered[0].1, "{answered:?}");
    let agent = started_pid(&wait_for_turn_end(&client, &chat.id, 1)).unwrap();
    drop(client);

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    let answered = run(&client, &chat.id, READ, 2);

    let [(text, false)] = answered.as_slice() else { panic!("one call that worked: {answered:?}") };
    assert!(text.contains("#7: Change 7"), "{text}");
    assert_eq!(latest_started_pid(&read_all(&client, &chat.id)), Some(agent), "the agent the last hub started");
}

/// The call comes back at once, saying it's in the background, and the turn ends with the wait listed there. Once a
/// read finds the pull request merged, the agent picks up again by itself, from what its CLI asked of the task, and
/// hears how the wait ended.
#[test]
fn an_agent_waiting_on_its_pull_request_hears_once_it_has_merged() {
    let github = StandInGithub::start();
    github.say(7, Open);
    let (stack, _gh) = github.stack();
    let client = stack.client();
    let (_, chat) = workspace_on_github(&stack, &client);

    client.session_prompt(&chat.id, WAIT, None).unwrap();
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
    assert_eq!(listed, [(Some("roundhouse/wait_for_pull_requests".to_string()), TaskKind::Wait)]);
    assert_eq!(waiting.waits.first().map(|wait| wait.number), Some(7), "the chat's record has the hub's wait");
    // The next hub keeps the wait, so it doesn't keep the hub from stopping to update it.
    assert_eq!(client.hub_status().unwrap().running_sessions, 0);
    github.say(7, Merged);
    client.session_prompt(&chat.id, READ, None).unwrap();

    let events = wait_for_turn_end(&client, &chat.id, 3);
    let said = assistant_text(&events);
    assert!(said.contains("every watched PR has merged"), "{said}");
    assert_eq!(client.session_get(&chat.id).unwrap().waits, [], "and none once it has ended");
}

/// The person calls the wait off while the agent's CLI runs. It hears as it next asks after its task, takes the task
/// off its list as one that failed, and picks up again by itself, saying what the hub last said of it.
#[test]
fn an_agent_whose_wait_the_person_calls_off_hears_and_the_chat_goes_idle() {
    let github = StandInGithub::start();
    github.say(7, Open);
    let (stack, _gh) = github.stack();
    let client = stack.client();
    let (_, chat) = workspace_on_github(&stack, &client);
    client.session_prompt(&chat.id, WAIT, None).unwrap();
    wait_for_status(&client, &chat.id, SessionStatus::Background);
    let [wait] = client.session_get(&chat.id).unwrap().waits.try_into().unwrap();

    let called_off = client.session_call_off_wait(&chat.id, &wait.id).unwrap();
    let events = wait_for_turn_end(&client, &chat.id, 2);

    assert_eq!(
        called_off.waits,
        [],
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
    let github = StandInGithub::start();
    github.say(7, Open);
    let (mut stack, _gh) = github.stack();
    let client = stack.client();
    let (_, chat) = workspace_on_github(&stack, &client);
    client.session_prompt(&chat.id, WAIT, None).unwrap();
    wait_for_status(&client, &chat.id, SessionStatus::Background);
    let agent = started_pid(&read_all(&client, &chat.id)).unwrap();
    drop(client);

    // Merged only once the last hub has gone, since a read it began as the turn ended would find it merged and end the
    // wait. Held from then, so the new hub can't read it until it's been looked at, and then reads it merged by itself,
    // as the first it reads. Not before the last hub has gone, since that read, held, would keep its worker from
    // stopping until the read timed out, or the hub gave up on the worker, 15 s on either way.
    stop(&mut stack);
    github.hold();
    github.say(7, Merged);
    stack.start_hub();
    let client = stack.client();
    let waiting = client.session_get(&chat.id).unwrap();
    let still = (waiting.status, waiting.background_tasks.len(), waiting.queue_held);
    assert_eq!(still, (SessionStatus::Background, 1, false), "nothing it was doing was cut off");
    github.release();

    let events = wait_for_turn_end(&client, &chat.id, 2);
    let said = assistant_text(&events);
    assert!(said.contains("every watched PR has merged"), "{said}");
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

/// As the app stops an older hub to update it, only if that cuts nothing off, which a chat whose only work is a wait
/// doesn't.
#[test]
fn a_wait_lasts_across_the_hub_stopping_for_an_update() {
    let stop = |stack: &mut Stack| {
        let client = stack.client();
        // The previous client's socket has closed, but the hub may not have seen it go yet. Wait only for that,
        // so the assertion below still catches a wait being counted as work that would keep the hub running.
        wait_until("the previous client to disconnect", || client.hub_status().unwrap().clients == 1);
        let stopped = stop_hub_if_idle(&client, &stack.socket(), stack.hub_pid()).unwrap();
        assert_eq!(stopped, IdleStop::Stopped, "a wait holds up no update");
        stack.hub.take().unwrap().wait().unwrap();
    };
    a_wait_lasts_across_a_restart(stop, false);
}

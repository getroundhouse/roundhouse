//! A worker an earlier hub left running that's another build, as after an update, which the next hub carries on with
//! while anything in its workspace is at work, and then replaces with its own.

#![cfg(test)]

use std::time::Duration;

use protocol::event::{EventBody, ExitReason};
use protocol::model::SessionStatus;

use crate::carrying_on::restart;
use crate::support::{
    Stack, assistant_text, is_alive, latest_started_pid, parent_of, read_all, started_pid, wait_for_status,
    wait_for_turn_end, wait_until,
};

/// As an update has it: the hub restarts as a newer build than the workers it leaves, and every worker the next hub
/// starts is its own build.
fn update(stack: &mut Stack) {
    stack.remove_env(worker::TEST_BUILD);
    restart(stack);
    stack.start_hub();
}

#[test]
fn a_worker_from_another_build_is_replaced_once_nothing_in_its_workspace_is_at_work() {
    let mut stack = Stack::start_with(vec![(worker::TEST_BUILD, "older".to_string())]);
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let done = stack.path("done");
    client.session_prompt(&session.id, &format!("background:{}", done.display()), None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Background);
    let older = parent_of(started_pid(&read_all(&client, &session.id)).unwrap());
    drop(client);

    update(&mut stack);
    // Its agent has work in the background, so the next hub carries on with the worker the agent is on. Long enough
    // for the two looks that would have stopped it with nothing at work.
    std::thread::sleep(Duration::from_secs(12));
    assert!(is_alive(older), "the worker from the older build was stopped while its agent was at work");
    std::fs::write(&done, "").unwrap();
    let client = stack.client();
    wait_for_turn_end(&client, &session.id, 2);
    wait_until("the worker from the older build to stop", || !is_alive(older));

    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 3);
    assert_ne!(parent_of(latest_started_pid(&events).unwrap()), older, "an agent on a worker of the hub's build");
    assert!(assistant_text(&events).contains("background:"), "the conversation went on: {events:#?}");
    let exits: Vec<_> = events
        .iter()
        .filter_map(|event| match event.body {
            EventBody::AgentExited { reason, .. } => Some(reason),
            _ => None,
        })
        .collect();
    assert_eq!(exits, [ExitReason::Requested], "stopped as asked, which a chat says nothing of");
}

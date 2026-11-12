//! A prompt sent while the agent works: it's recorded at once, and the agent takes it when it can.

#![cfg(test)]

use client_core::Client;
use protocol::client::HubNotice;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Seq, Session, SessionId, SessionStatus};

use crate::support::{Stack, assistant_text, next_notice, parent_of, read_all, signal, wait_for_turn_end, wait_until};

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

/// A prompt sent while the agent runs a long tool, which it hasn't taken yet.
/// A prompt sent mid-turn, once the agent has read it. Recording it isn't enough, since the hub delivers it after: an
/// agent that went away first would have the prompt delivered to the one that replaces it, which takes it.
fn waiting_prompt(client: &Client, session_id: &SessionId) -> Seq {
    client.session_prompt(session_id, "hang:60000", None).unwrap();
    wait_for_tool(client, session_id);
    let (steered, _) = client.session_prompt(session_id, "say:never", None).unwrap();
    // The CLI says it has queued a prompt as it reads one.
    wait_until("the agent to read the prompt", || {
        read_all(client, session_id).iter().any(|e| {
            e.seq > steered && matches!(&e.body, EventBody::AgentOther { kind, .. } if kind == "command_lifecycle")
        })
    });
    steered
}

fn wait_for_drop(client: &Client, session_id: &SessionId, prompt: Seq) {
    wait_until("the prompt to be dropped", || {
        read_all(client, session_id)
            .iter()
            .any(|e| matches!(e.body, EventBody::PromptDropped { prompt: p } if p == prompt))
    });
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

/// The record says what the agent has yet to take, which is what a client tells from it whether a compaction can be
/// asked, and the feed tells of the agent taking it, though that changes no status.
#[test]
fn the_feed_tells_of_a_prompt_sent_mid_turn_until_the_agent_takes_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:1500", None).unwrap();
    wait_for_tool(&client, &session.id);
    let feed = client.feed_subscribe().unwrap();

    let (steered, _) = client.session_prompt(&session.id, "say:steered", None).unwrap();

    let told = |what: &str, wanted: &dyn Fn(&Session) -> bool| loop {
        if let HubNotice::Session(told) = next_notice(&feed, what)
            && told.id == session.id
            && wanted(&told)
        {
            break told;
        }
    };
    let waiting = told("the prompt on its way", &|told| told.untaken.as_deref() == Some(&[steered][..]));
    assert!(waiting.compaction_waits());
    let taken = told("the prompt taken", &|told| told.untaken.as_deref() == Some(&[][..]));
    assert_eq!(taken.status, SessionStatus::Running, "before its turn has ended");
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

fn agent_pid(client: &Client, session_id: &SessionId) -> u32 {
    read_all(client, session_id)
        .iter()
        .find_map(|e| match e.body {
            EventBody::AgentStarted { pid, .. } => Some(pid),
            _ => None,
        })
        .unwrap()
}

#[test]
fn a_prompt_the_agent_hadnt_taken_when_it_died_is_dropped() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let steered = waiting_prompt(&client, &session.id);

    signal(agent_pid(&client, &session.id), libc::SIGKILL);

    wait_for_drop(&client, &session.id, steered);
}

#[test]
fn a_prompt_the_agent_hadnt_taken_when_its_worker_was_lost_is_dropped() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let steered = waiting_prompt(&client, &session.id);

    signal(parent_of(agent_pid(&client, &session.id)), libc::SIGKILL);

    wait_for_drop(&client, &session.id, steered);
}

#[test]
fn a_prompt_the_agent_hadnt_taken_when_it_went_with_its_hub_is_dropped() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let steered = waiting_prompt(&client, &session.id);

    stack.kill_hub_and_workers();
    stack.start_hub();

    wait_for_drop(&stack.client(), &session.id, steered);
}

/// The next hub knows the agent it carries on with hadn't taken it, so it's dropped if the agent's worker is lost first,
/// which leaves the agent no way to say so, as it would have been with the hub there.
#[test]
fn a_prompt_the_agent_hadnt_taken_when_its_hub_went_away_is_dropped_if_its_worker_is_lost_first() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let steered = waiting_prompt(&client, &session.id);
    let worker = parent_of(agent_pid(&client, &session.id));
    drop(client);

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    signal(worker, libc::SIGKILL);

    wait_for_drop(&client, &session.id, steered);
}

/// The agent the next hub carries on with takes it, once its tool has finished, as it would have with the hub there.
#[test]
fn a_prompt_the_agent_hadnt_taken_when_its_hub_went_away_is_taken_once_the_next_hub_carries_on() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let (first, _) = client.session_prompt(&session.id, "hang:1500", None).unwrap();
    wait_for_tool(&client, &session.id);
    let (steered, _) = client.session_prompt(&session.id, "say:steered", None).unwrap();
    wait_until("the agent to read the prompt", || {
        read_all(&client, &session.id).iter().any(|e| {
            e.seq > steered && matches!(&e.body, EventBody::AgentOther { kind, .. } if kind == "command_lifecycle")
        })
    });
    drop(client);

    stack.kill_hub();
    stack.start_hub();

    let client = stack.client();
    let events = wait_for_turn_end(&client, &session.id, 1);
    let taken = |prompt: Seq| {
        events.iter().filter(|e| matches!(e.body, EventBody::PromptTaken { prompt: p } if p == prompt)).count()
    };
    assert_eq!((taken(first), taken(steered)), (1, 1), "{events:#?}");
    assert!(!events.iter().any(|e| matches!(e.body, EventBody::PromptDropped { .. })), "{events:#?}");
    assert_eq!(assistant_text(&events), "steered");
}

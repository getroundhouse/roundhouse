//! Reading and following the agent's own events apart from a subagent's, and the hub's record of each subagent.

#![cfg(test)]

use std::time::Instant;

use client_core::session_log::{By, Row, SessionLog};
use client_core::{Client, OpenSession, RecordsFeed, Subscription, SubscriptionItem, View};
use protocol::client::HubNotice;
use protocol::event::{ContentBlock, EventBody, SessionEvent, SubagentOutcome, TaskKind};
use protocol::model::{ModelName, Place, SessionId, Subagent, SubagentStatus, ToolUseId};
use protocol::tail::Tail;

use crate::support::{Follower, Stack, TIMEOUT, read_all, wait_for_turn_end, wait_until};

/// The call that started the subagent, once it's in the log.
fn subagent_call(client: &Client, session_id: &SessionId) -> ToolUseId {
    let mut call = None;
    wait_until("the subagent's call", || {
        call = read_all(client, session_id).iter().find_map(|event| match &event.body {
            EventBody::Assistant { blocks, .. } => blocks.iter().find_map(|block| match block {
                ContentBlock::ToolUse { id, name, .. } if name == "Agent" => Some(id.clone()),
                _ => None,
            }),
            _ => None,
        });
        call.is_some()
    });
    call.unwrap()
}

/// Checks that `events` are all from `subagent`, or the agent's own, in place order from the first.
fn assert_placed(events: &[SessionEvent], subagent: Option<&ToolUseId>) {
    for (place, event) in (1..).zip(events) {
        assert_eq!((event.subagent.as_ref(), event.place), (subagent, Place::new(place)), "{event:?}");
    }
}

fn reported(follower: &Follower) -> bool {
    follower.events.iter().any(|event| {
        matches!(&event.body, EventBody::Assistant { blocks, .. }
            if matches!(blocks.as_slice(), [ContentBlock::Text { text, .. }] if text == "There's a README here."))
    })
}

#[test]
fn the_agents_own_events_and_a_subagents_are_read_and_followed_apart() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "agent:Look around", None).unwrap();
    let call = subagent_call(&client, &session.id);
    let (own, subagent) = (client.session_subscribe_places(&session.id, None, Place::new(1)).unwrap(), {
        client.session_subscribe_places(&session.id, Some(&call), Place::new(1)).unwrap()
    });
    let (mut agent, mut work) = (Follower::default(), Follower::default());

    work.drain_until(&subagent, "the subagent to write", |work| work.tail_text().contains("There's"));
    work.drain_until(&subagent, "the subagent's report", reported);
    wait_for_turn_end(&client, &session.id, 1);
    let (read, last) = client.session_read_places(&session.id, None, Place::new(1), 1000).unwrap();
    agent.drain_until(&own, "the agent's own events", |agent| {
        assert!(!agent.tail_text().contains("README"), "the agent's tail holds only what it writes");
        agent.events.len() >= read.len()
    });

    assert_placed(&work.events, Some(&call));
    assert_eq!(work.events.len(), 3, "its call, the call's result, and its report");
    assert_placed(&agent.events, None);
    assert!(agent.events.iter().any(|event| matches!(event.body, EventBody::TurnEnded { .. })));
    let about_it = |event: &&SessionEvent| match &event.body {
        EventBody::SubagentStarted { call: started, .. } | EventBody::SubagentEnded { call: started, .. } => {
            *started == call
        }
        _ => false,
    };
    let [started, ended] = agent.events.iter().filter(about_it).collect::<Vec<_>>()[..] else {
        panic!("the agent says once that it started and once that it ended: {:?}", agent.events)
    };
    assert!(matches!(&started.body, EventBody::SubagentStarted { prompt: Some(prompt), in_background: true, .. }
        if prompt == "Look around."));
    assert!(
        matches!(&ended.body, EventBody::SubagentEnded { outcome: SubagentOutcome::Finished, report: Some(report), .. }
        if report == "There's a README here.")
    );
    assert_eq!((&agent.events[..read.len()], last), (read.as_slice(), Place::new(u64::try_from(read.len()).unwrap())));
    let (rest, last) = client.session_read_places(&session.id, Some(&call), Place::new(2), 1000).unwrap();
    assert_eq!((rest.as_slice(), last), (&work.events[1..], Place::new(3)));
}

/// Reads a new subscription as far as the tail it starts from, which comes once it has caught up on the log.
fn first_tail(subscription: &Subscription) -> Tail {
    loop {
        match subscription.items.recv_timeout(TIMEOUT) {
            Ok(SubscriptionItem::Tail(tail)) => return tail,
            Ok(_) => {}
            Err(e) => panic!("timed out waiting for the tail: {e}"),
        }
    }
}

#[test]
fn a_follower_that_joins_as_a_subagent_writes_starts_from_only_what_it_follows() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "agent:Look around", None).unwrap();
    let call = subagent_call(&client, &session.id);
    let first = client.session_subscribe_places(&session.id, Some(&call), Place::new(1)).unwrap();
    Follower::default().drain_until(&first, "the subagent to write", |work| work.tail_text().contains("There's"));

    let (own, late) = (client.session_subscribe_places(&session.id, None, Place::new(1)).unwrap(), {
        client.session_subscribe_places(&session.id, Some(&call), Place::new(1)).unwrap()
    });

    let own = first_tail(&own);
    assert!(own.messages.iter().all(|message| message.parent_tool_use_id.is_none()), "{own:?}");
    let late = Follower { events: Vec::new(), tail: first_tail(&late) };
    assert!(late.tail_text().starts_with("There's "), "what it has written so far: {:?}", late.tail);
}

#[test]
fn a_subagent_in_the_foreground_cut_off_as_it_writes_leaves_its_follower_nothing_half_written() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "agent-foreground:Look around", None).unwrap();
    let call = subagent_call(&client, &session.id);
    let subagent = client.session_subscribe_places(&session.id, Some(&call), Place::new(1)).unwrap();
    let mut work = Follower::default();
    work.drain_until(&subagent, "the subagent to write", |work| work.tail_text().contains("There's"));

    client.session_cancel(&session.id).unwrap();

    // The end of the turn ends what the subagent was writing, though its follower never sees that event.
    work.drain_until(&subagent, "what the subagent was writing to go", |work| work.tail.is_empty());
    wait_for_turn_end(&client, &session.id, 1);
    assert!(!reported(&work));
    assert_placed(&work.events, Some(&call));
}

/// The agent names the call a subagent's task runs for only a line after it first lists the task, so the session's
/// record has to follow the list said again with the call, which is how a client opens the subagent's tab from its
/// line among the work in the background.
#[test]
fn a_subagent_in_the_background_is_listed_with_the_call_that_started_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let done = stack.path("done");
    client.session_prompt(&session.id, &format!("agent-until:{}:Look around", done.display()), None).unwrap();
    let call = subagent_call(&client, &session.id);
    let listed = || {
        let tasks = client.session_get(&session.id).unwrap().background_tasks;
        tasks.into_iter().map(|task| (task.kind, task.call)).collect::<Vec<_>>()
    };

    wait_until("its task to name its call", || listed() == [(TaskKind::Subagent, Some(call.clone()))]);

    let events = read_all(&client, &session.id);
    let started = events
        .iter()
        .position(|event| matches!(&event.body, EventBody::SubagentStarted { call: started, .. } if *started == call))
        .expect("the subagent's start");
    let next = &events[started + 1].body;
    let names_it = matches!(next, EventBody::AgentTasks { tasks, .. } if tasks[0].call.as_ref() == Some(&call));
    assert!(names_it, "the list again, straight after its start: {next:?}");
    std::fs::write(&done, "").unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    assert!(listed().is_empty(), "it leaves the list as it ends");
}

#[test]
fn the_hub_keeps_a_record_of_each_subagent_and_says_on_the_feed_as_it_changes() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let feed = client.feed_subscribe().unwrap();
    client.session_prompt(&session.id, "agent:Look around", None).unwrap();
    let call = subagent_call(&client, &session.id);

    let deadline = Instant::now() + TIMEOUT;
    let mut notices = Vec::new();
    while !notices.last().is_some_and(|subagent: &Subagent| subagent.status != SubagentStatus::Working) {
        match feed.items.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(SubscriptionItem::Notice(notice)) => {
                if let HubNotice::Subagent(subagent) = *notice {
                    notices.push(subagent);
                }
            }
            Ok(_) => {}
            Err(e) => panic!("timed out waiting for the subagent to end, having heard {notices:?}: {e}"),
        }
    }
    wait_for_turn_end(&client, &session.id, 1);
    let (subagents, rev) = client.session_subagents(&session.id).unwrap();

    let [subagent] = subagents.as_slice() else { panic!("one subagent: {subagents:?}") };
    assert_eq!((&subagent.call, subagent.status, subagent.calls), (&call, SubagentStatus::Finished, 1));
    assert_eq!((subagent.prompt.as_deref(), subagent.in_background), (Some("Look around."), true));
    assert_eq!(subagent.latest_call.as_ref().map(|call| call.name.as_str()), Some("Bash"));
    assert_eq!(subagent.report.as_deref(), Some("There's a README here."));
    assert_eq!(subagent.model.as_ref().map(ModelName::as_str), Some("fake-model"), "as its replies named it");
    assert_eq!(notices[0].model, None, "before it replied");
    assert!(subagent.rev <= rev);
    // Each notice's calls, whether its newest call had heard back, and whether it had said anything since.
    let heard: Vec<_> = notices
        .iter()
        .map(|subagent| {
            let answered = subagent.recent.calls.last().is_some_and(|made| made.result.is_some());
            (subagent.status, subagent.calls, answered, subagent.recent.since.is_some())
        })
        .collect();
    let (working, finished) = (SubagentStatus::Working, SubagentStatus::Finished);
    let told = [
        (working, 0, false, false),
        (working, 1, false, false),
        (working, 1, true, false),
        (working, 1, true, true),
        (finished, 1, false, false),
    ];
    assert_eq!(heard, told, "as it started, made its call, heard back, said what it found, and ended");
    assert!(notices.windows(2).all(|pair| pair[0].rev < pair[1].rev));
    assert_eq!(notices.last(), Some(subagent), "the last it heard is what's listed");
    assert!(client.session_subagents(&"missing".into()).is_err());
}

/// Every event the open view holds, if it holds all of them, in order.
fn held(log: &SessionLog) -> Option<Vec<SessionEvent>> {
    (0..log.row_count())
        .map(|row| match log.row(row) {
            Row::Event(event) => Some(event.clone()),
            Row::Unloaded => None,
        })
        .collect()
}

#[test]
fn a_view_holds_only_the_agents_own_events_or_one_subagents_and_the_feed_keeps_the_subagents_record() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let feed = RecordsFeed::start(&client);
    client.session_prompt(&session.id, "agent:Look around", None).unwrap();
    let call = subagent_call(&client, &session.id);

    let own = OpenSession::start_view(&client, &session, View::Own);
    let work = OpenSession::start_view(&client, &session, View::Subagent(call.clone()));
    wait_for_turn_end(&client, &session.id, 1);

    let hub_has = |subagent| client.session_read_places(&session.id, subagent, Place::new(1), 1000).unwrap().0;
    wait_until("the agent's own events", || own.read(held).is_some_and(|events| events == hub_has(None)));
    wait_until("the subagent's", || work.read(held).is_some_and(|events| events == hub_has(Some(&call))));
    assert_eq!((own.read(SessionLog::by), work.read(SessionLog::by)), (By::Place, By::Place));
    assert!(own.read(held).unwrap().iter().all(|event| event.subagent.is_none()));
    // Asked for only once the feed is following and the subagent has finished, so its record can come only from
    // the list the feed makes when it's asked.
    wait_until("the feed to follow", || feed.connected());
    // One the hub doesn't have, which comes before every real one, can't keep the rest from being listed.
    feed.follow_subagents(&"!missing".into());
    feed.follow_subagents(&session.id);
    let finished = |feed: &RecordsFeed| {
        let record = feed.snapshot().subagent(&session.id, &call).map(|record| Subagent::clone(record));
        record.filter(|record| record.status == SubagentStatus::Finished)
    };
    wait_until("the feed to keep its record", || finished(&feed).is_some());
    assert_eq!(finished(&feed).unwrap().report.as_deref(), Some("There's a README here."));
    // Nor stop the feed when every session is listed again on a new connection.
    feed.reconnect(&stack.client());
    wait_until("the feed to follow again", || feed.connected());
    assert!(feed.take_errors().is_empty(), "{:?}", feed.take_errors());
}

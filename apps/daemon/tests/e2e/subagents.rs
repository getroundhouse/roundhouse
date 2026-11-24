//! Reading and following the agent's own events apart from a subagent's, through a real adapter and a socket, and a
//! client's views and its copy of each subagent's record. What the hub keeps of each subagent, and sends a follower of
//! one, is the hub's, in `hub::sessions::tests::subagents`, `hub::sessions::tests::background` and `server::pump::tests`.

#![cfg(test)]

use client_core::session_log::{Row, SessionLog};
use client_core::{Client, OpenSession, RecordsFeed, Subscription, View};
use protocol::capability::calls::Call;
use protocol::client::subscriptions::{Follow, LogFrom};
use protocol::event::{ContentBlock, EventBody, SessionEvent, TaskOutcome};
use protocol::model::{Place, SessionId, Subagent, SubagentId, SubagentStatus, ToolUseId};

use crate::support::{Follower, Gate, Stack, read_all, wait_for_turn_end, wait_until};

/// The call that started the subagent, once it's in the log.
fn subagent_call(client: &Client, session_id: &SessionId) -> ToolUseId {
    let mut call = None;
    wait_until("the subagent's call", || {
        call = read_all(client, session_id).iter().find_map(|event| match &event.body {
            EventBody::Assistant { blocks, .. } => blocks.iter().find_map(|block| match block {
                ContentBlock::Call { id, call: Call::Subagent(_) } => Some(id.clone()),
                _ => None,
            }),
            _ => None,
        });
        call.is_some()
    });
    call.unwrap()
}

/// The call that started the subagent, and the subagent, as the agent named it when it said it started, once that's
/// in the log. The CLI names it by the task it runs as, which isn't the call.
fn subagent(client: &Client, session_id: &SessionId) -> (ToolUseId, SubagentId) {
    let call = subagent_call(client, session_id);
    let mut id = None;
    wait_until("the subagent's start", || {
        id = read_all(client, session_id).iter().find_map(|event| match &event.body {
            EventBody::SubagentStarted { call: started, .. } if *started == call => event.body.subagent_started(),
            _ => None,
        });
        id.is_some()
    });
    let id = id.unwrap();
    assert_ne!(id, SubagentId::of_call(&call), "a test of a subagent named by its call can't tell the two apart");
    (call, id)
}

/// Checks that `events` are all from `subagent`, or the agent's own, in place order from the first.
fn assert_placed(events: &[SessionEvent], subagent: Option<&SubagentId>) {
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

/// Follows the agent's own events in session `id`, or the subagent `subagent`'s, from the first.
fn follow_places(client: &Client, id: &SessionId, subagent: Option<&SubagentId>) -> Subscription {
    let from = LogFrom::Place { subagent: subagent.cloned(), place: Place::new(1) };
    client.follow(Follow::Session { id: id.clone(), from: Some(from) }).unwrap()
}

#[test]
fn the_agents_own_events_and_a_subagents_are_read_and_followed_apart() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let report = Gate::new(&stack, "report");
    client.session_prompt(&session.id, &format!("agent-until:{report}:Look around"), None).unwrap();
    let (call, id) = subagent(&client, &session.id);
    let (own, subagent) = (follow_places(&client, &session.id, None), follow_places(&client, &session.id, Some(&id)));
    let (mut agent, mut work) = (Follower::default(), Follower::default());

    work.drain_until(&subagent, "the subagent to write", |work| work.tail_text().contains("There's"));
    report.open();
    work.drain_until(&subagent, "the subagent's report", reported);
    wait_for_turn_end(&client, &session.id, 1);
    let (read, last) = client.session_read_places(&session.id, None, Place::new(1), 1000).unwrap();
    agent.drain_until(&own, "the agent's own events", |agent| {
        assert!(!agent.tail_text().contains("README"), "the agent's tail holds only what it writes");
        agent.events.len() >= read.len()
    });

    assert_placed(&work.events, Some(&id));
    assert_eq!(work.events.len(), 3, "its call, the call's result, and its report");
    assert_placed(&agent.events, None);
    assert!(agent.events.iter().any(|event| matches!(event.body, EventBody::TurnEnded { .. })));
    let about_it = |event: &&SessionEvent| match &event.body {
        EventBody::SubagentStarted { call: started, .. } => *started == call,
        EventBody::SubagentEnded { subagent: ended, .. } => *ended == id,
        _ => false,
    };
    let [started, ended] = agent.events.iter().filter(about_it).collect::<Vec<_>>()[..] else {
        panic!("the agent says once that it started and once that it ended: {:?}", agent.events)
    };
    assert!(matches!(&started.body, EventBody::SubagentStarted { prompt: Some(prompt), in_background: true, .. }
        if prompt == "Look around."));
    assert!(
        matches!(&ended.body, EventBody::SubagentEnded { outcome: TaskOutcome::Finished, report: Some(report), .. }
        if report == "There's a README here.")
    );
    assert_eq!((&agent.events[..read.len()], last), (read.as_slice(), Place::new(u64::try_from(read.len()).unwrap())));
    let (rest, last) = client.session_read_places(&session.id, Some(&id), Place::new(2), 1000).unwrap();
    assert_eq!((rest.as_slice(), last), (&work.events[1..], Place::new(3)));
    // A client names a subagent by the call that started it until it has the subagent's record.
    let by_its_call = Some(SubagentId::of_call(&call));
    let (named, _) = client.session_read_places(&session.id, by_its_call.as_ref(), Place::new(1), 1000).unwrap();
    assert_eq!(named, work.events, "read by its call");
    let followed = follow_places(&client, &session.id, by_its_call.as_ref());
    let mut by_call = Follower::default();
    by_call.drain_until(&followed, "its events, by its call", |by_call| by_call.events.len() >= work.events.len());
    assert_eq!(by_call.events, work.events, "followed by its call");
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
    client.session_prompt(&session.id, "agent-calls:1:Look around", None).unwrap();
    let (_, id) = subagent(&client, &session.id);

    let own = OpenSession::start(&client, &feed, &session, View::Own);
    let work = OpenSession::start(&client, &feed, &session, View::Subagent(id.clone()));
    wait_for_turn_end(&client, &session.id, 1);

    let hub_has = |subagent| client.session_read_places(&session.id, subagent, Place::new(1), 1000).unwrap().0;
    wait_until("the agent's own events", || own.read(held).is_some_and(|events| events == hub_has(None)));
    wait_until("the subagent's", || work.read(held).is_some_and(|events| events == hub_has(Some(&id))));
    assert!(own.read(held).unwrap().iter().all(|event| event.subagent.is_none()));
    // The views follow the session, and its records with it, which the feed keeps, as each view's subscription lists
    // them first, and then tells of each change.
    wait_until("the feed to follow", || feed.connected());
    let finished = |feed: &RecordsFeed| {
        let record = feed.snapshot().subagent(&session.id, &id).map(|record| Subagent::clone(record));
        record.filter(|record| record.status == SubagentStatus::Finished)
    };
    wait_until("the feed to keep its record", || finished(&feed).is_some());
    assert_eq!(finished(&feed).unwrap().report.as_deref(), Some("There's a README here."));
    // Nor stop the feed on a new connection.
    feed.reconnect(&stack.client());
    wait_until("the feed to follow again", || feed.connected());
    assert!(feed.take_errors().is_empty(), "{:?}", feed.take_errors());
}

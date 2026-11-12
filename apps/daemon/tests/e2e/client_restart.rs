//! Surviving a client restart.

#![cfg(test)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use client_core::session_log::{Row, SessionLog};
use client_core::{OpenSession, SubscriptionItem};
use protocol::client::ClientRequest;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::SessionStatus;
use protocol::tail::Tail;

use crate::support::{
    Follower, Stack, TIMEOUT, assert_gapless, assistant_text, is_alive, wait_for_status, wait_for_turn_end,
    wait_for_turn_end_while_progressing, wait_until,
};

#[test]
fn work_continues_with_no_client_and_a_new_client_catches_up_with_no_gaps() {
    let stack = Stack::start();
    let (session_id, seen, agent_pid) = {
        let client = stack.client();
        let (_, session) = stack.workspace(&client);
        client.session_prompt(&session.id, "slow:30:chunk", None).unwrap();
        let subscription = client.session_subscribe(&session.id, 1).unwrap();
        let mut follower = Follower::default();
        follower.drain_until(&subscription, "streaming text", |f| f.tail_text().contains("chunk3"));
        let agent_pid = follower
            .events
            .iter()
            .find_map(|e| match &e.body {
                EventBody::AgentStarted { pid, .. } => Some(*pid),
                _ => None,
            })
            .unwrap();
        (session.id, follower.events.len() as u64, agent_pid)
        // The client disconnects here, mid-turn.
    };
    let watcher = stack.client();
    wait_until("the hub to see the client leave", || watcher.hub_status().unwrap().clients == 1);
    assert_eq!(watcher.session_get(&session_id).unwrap().status, SessionStatus::Running);
    assert!(is_alive(agent_pid), "the agent must outlive its client");

    let client = stack.client();
    let events = wait_for_turn_end(&client, &session_id, 1);
    assert_gapless(&events);
    assert!(assistant_text(&events).contains("chunk29"), "the turn finished with nobody attached");

    // Subscribing from the last seq seen delivers exactly the rest.
    let subscription = client.session_subscribe(&session_id, seen + 1).unwrap();
    let mut follower = Follower::default();
    let total = events.len();
    follower.drain_until(&subscription, "the rest of the log", |f| f.events.len() as u64 + seen >= total as u64);
    assert_eq!(follower.events.first().unwrap().seq, seen + 1);
    let mut stitched: Vec<u64> = (1..=seen).collect();
    stitched.extend(follower.events.iter().map(|e| e.seq));
    assert_eq!(stitched, (1..=total as u64).collect::<Vec<_>>());

    // The session is still usable, on the same agent process.
    client.session_prompt(&session_id, "say:still here", None).unwrap();
    let events = wait_for_turn_end(&client, &session_id, 2);
    assert!(assistant_text(&events).ends_with("still here"));
    assert!(!events.iter().skip(total).any(|e| matches!(e.body, EventBody::AgentStarted { .. })));
}

#[test]
fn a_client_attaching_mid_message_sees_the_partial_text_at_once() {
    let stack = Stack::start();
    let first = stack.client();
    let (_, session) = stack.workspace(&first);
    first.session_prompt(&session.id, "slow:40:part", None).unwrap();
    let live = first.session_subscribe(&session.id, 1).unwrap();
    let mut watcher = Follower::default();
    watcher.drain_until(&live, "some streaming", |f| f.tail_text().contains("part5"));

    let second = stack.client();
    let subscription = second.session_subscribe(&session.id, 1).unwrap();
    let mut late = Follower::default();
    // Its first tail should already hold what was streamed, without waiting for more.
    late.drain_until(&subscription, "the tail snapshot", |f| f.tail_text().contains("part5"));

    // And after that, its view stays identical to a client that was there from the start.
    late.drain_until(&subscription, "the turn to end", |f| {
        f.events.iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. }))
    });
    watcher.drain_until(&live, "the turn to end", |f| {
        f.events.iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. }))
    });
    let seqs = |f: &Follower| f.events.iter().map(|e| e.seq).collect::<Vec<_>>();
    let common = seqs(&late).len().min(seqs(&watcher).len());
    assert_eq!(seqs(&late)[..common], seqs(&watcher)[..common]);
    assert_eq!(assistant_text(&late.events), assistant_text(&watcher.events));
    assert!(late.tail.is_empty() && watcher.tail.is_empty());
}

#[test]
fn every_chunk_reaches_the_tail_exactly_once_while_streaming() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "slow:8:x", None).unwrap();
    let subscription = client.session_subscribe(&session.id, 1).unwrap();
    let mut follower = Follower::default();
    follower.drain_until(&subscription, "the last chunk", |f| f.tail_text().contains("x7"));
    assert_eq!(follower.tail_text(), "x0 x1 x2 x3 x4 x5 x6 x7 ");
}

/// Sets a flag even if the test is panicking, so a scoped thread watching it still finishes.
struct SetOnDrop<'a>(&'a AtomicBool);

impl Drop for SetOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

#[test]
fn clients_attaching_during_a_burst_never_see_a_chunk_twice_or_miss_one() {
    // Each attach is one chance to catch a race, and the attaches go up with the chunks rather than
    // with time, so fewer chunks would let a narrow race through more often. A busy machine can take
    // longer than `TIMEOUT` over this many.
    const CHUNKS: u32 = 40_000;
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let expected: String = (0..CHUNKS).map(|i| i.to_string() + " ").collect();

    // Attach and detach over and over while deltas pour in. A chunk could land both in a client's
    // snapshot and in the live stream after it, or in neither. Whenever it's checked, a client's
    // tail must be an exact prefix of the text being written: a lagging client can be short, but
    // nothing else can differ.
    let done = AtomicBool::new(false);
    // The most text any attacher has held, so the wait for the turn goes on for as long as it grows.
    let furthest = AtomicUsize::new(0);
    let (snapshots_mid_stream, attaches) = std::thread::scope(|scope| {
        let attachers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    let mut snapshots_mid_stream = 0u32;
                    let mut attaches = 0u32;
                    while !done.load(Ordering::Relaxed) {
                        let client = stack.client();
                        let subscription = client.session_subscribe(&session.id, 1).unwrap();
                        attaches += 1;
                        let mut tail = Tail::default();
                        for _ in 0..50 {
                            let item = match subscription.items.recv_timeout(Duration::from_millis(50)) {
                                Ok(item) => item,
                                Err(flume::RecvTimeoutError::Timeout) => break,
                                Err(flume::RecvTimeoutError::Disconnected) => panic!("lost the hub"),
                            };
                            match item {
                                SubscriptionItem::Delta(delta) => tail.apply(&delta),
                                SubscriptionItem::Tail(snapshot) => {
                                    snapshots_mid_stream += u32::from(!snapshot.is_empty());
                                    tail = snapshot;
                                }
                                SubscriptionItem::Event(event) => tail.on_event(event.seq, event.run, &event.body),
                                SubscriptionItem::Notice(_) => {}
                            }
                        }
                        let held: String =
                            tail.messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect();
                        assert!(
                            expected.starts_with(&held),
                            "tail diverged after {} of {} bytes",
                            common_prefix(&expected, &held),
                            held.len()
                        );
                        furthest.fetch_max(held.len(), Ordering::Relaxed);
                    }
                    (snapshots_mid_stream, attaches)
                })
            })
            .collect();
        let finish = SetOnDrop(&done);
        client.session_prompt(&session.id, &format!("burst:{CHUNKS}"), None).unwrap();
        let events = wait_for_turn_end_while_progressing(&client, &session.id, 1, || furthest.load(Ordering::Relaxed));
        assert_eq!(assistant_text(&events), expected, "the durable message is the whole text");
        drop(finish);
        attachers
            .into_iter()
            .map(|attacher| attacher.join().unwrap())
            .fold((0, 0), |totals, one| (totals.0 + one.0, totals.1 + one.1))
    });
    // Otherwise this test proved nothing.
    assert!(snapshots_mid_stream >= 10, "only {snapshots_mid_stream} of {attaches} attaches landed mid-stream");
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

#[test]
fn a_client_that_stalled_during_a_message_ends_with_nothing_left_in_its_tail() {
    use protocol::client::{ClientFrame, HubFrame};
    use std::io::{BufRead, BufReader, Write};
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    // A client that subscribes, then doesn't read its socket until everything has finished.
    let mut stream = std::os::unix::net::UnixStream::connect(stack.socket()).unwrap();
    let requests = [
        ClientRequest::Hello {
            protocol_version: protocol::client::CLIENT_PROTOCOL_VERSION,
            client: "stalled".into(),
            oldest_protocol_version: None,
        },
        ClientRequest::SessionSubscribe { id: session.id.clone(), from_seq: 1, subscription_id: 7 },
    ];
    for (id, request) in requests.into_iter().enumerate() {
        stream.write_all(&protocol::frame::encode(&ClientFrame { id: id as u64, request }).unwrap()).unwrap();
    }
    // Many short messages put events and deltas side by side in the live stream, and then the agent
    // dies mid-message. An event read ahead of the deltas before it would let them land after the
    // agent's exit, and nothing would ever clear them. Sized to overflow the connection's queue, so
    // the hub falls behind, without overflowing the live buffer, which would resync instead.
    let mut script: Vec<String> = (0..500).map(|i| format!("say:message {i}")).collect();
    script.extend(["partial:cut off".to_string(), "crash:3".to_string()]);
    client.session_prompt(&session.id, &script.join("; "), None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);

    stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let mut reader = BufReader::new(stream);
    let mut tail = Tail::default();
    let mut line = String::new();
    let mut ended = false;
    loop {
        line.clear();
        if reader.read_line(&mut line).is_err() || line.is_empty() {
            break;
        }
        match serde_json::from_str::<HubFrame>(&line).unwrap() {
            HubFrame::Event { event, .. } => {
                ended |= matches!(event.body, EventBody::AgentExited { .. });
                tail.on_event(event.seq, event.run, &event.body);
            }
            HubFrame::Delta { delta, .. } => tail.apply(&delta),
            HubFrame::Tail { tail: snapshot, .. } => tail = snapshot,
            HubFrame::Reply { .. } | HubFrame::Notice { .. } => {}
        }
    }
    assert!(ended, "the client should have read through the agent's exit");
    assert!(tail.is_empty(), "stale partial text would stay on screen: {} message(s)", tail.messages.len());
}

#[test]
fn status_is_already_current_when_an_event_is_delivered() {
    // A client that sees the agent go idle and then asks for status must not read an old value:
    // with nothing more coming, it would wait forever.
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let subscription = client.session_subscribe(&session.id, 1).unwrap();
    let mut idles = 0;
    client.session_prompt(&session.id, "say:0", None).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while idles < 200 {
        let wait = deadline.saturating_duration_since(Instant::now());
        let SubscriptionItem::Event(event) = subscription.items.recv_timeout(wait).expect("more events") else {
            continue;
        };
        // The event that moves the status from running to idle.
        if matches!(event.body, EventBody::TurnEnded { .. }) {
            assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle, "after {idles} turns");
            idles += 1;
            client.session_prompt(&session.id, &format!("say:{idles}"), None).unwrap();
        }
    }
}

/// Every event the open session holds, if it holds all of them, in order.
fn held(log: &SessionLog) -> Option<Vec<SessionEvent>> {
    (0..log.row_count())
        .map(|row| match log.row(row) {
            Row::Event(event) => Some(event.clone()),
            Row::Unloaded | Row::Pending(_) => None,
        })
        .collect()
}

#[test]
fn an_open_session_carries_on_from_where_it_got_to_on_a_new_connection() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let open = OpenSession::start(&client, &session);
    client.session_prompt(&session.id, "say:before", None).unwrap();
    wait_until("the first turn", || open.read(held).is_some_and(|events| assistant_text(&events).contains("before")));

    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    open.reconnect(&client);
    client.session_prompt(&session.id, "say:after", None).unwrap();

    // Everything the hub has, with nothing missing.
    let hub_has = || client.session_read(&session.id, 1, 1000, false).unwrap().0;
    wait_until("the second turn", || {
        open.read(held).is_some_and(|events| assistant_text(&events).contains("after") && events == hub_has())
    });
}

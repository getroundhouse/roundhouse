//! That the store keeps each subagent's record as `protocol`'s `Subagent::recorded_from` makes it of the same events,
//! which a fake hub makes its records by, so the two can't answer differently. The rules themselves are the store's
//! other tests'.

use super::super::*;
use crate::store::now_ms;
use crate::store::tests::store_with_session;
use protocol::event::TaskOutcome;

use super::{answer, ended, exited, message, reading, record, reply, started};

/// A start under the ID its agent gives it, by `call`.
fn start(call: &str, id: &str) -> EventBody {
    let mut start = started(call);
    if let EventBody::SubagentStarted { subagent, .. } = &mut start {
        *subagent = Some(id.into());
    }
    start
}

fn said(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into(), images: Vec::new() }
}

#[test]
fn the_store_keeps_each_subagents_record_as_protocol_makes_it_of_the_same_events() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let essay = "a".repeat(5_000);
    let mut log = Vec::new();
    let mut say = |body: EventBody| log.push(record(&store, &body).0);
    say(started("agent"));
    say(start("toolu_9", "thread-1"));
    say(EventBody::AgentUser { subagent: Some("agent".into()), blocks: vec![said("Find it.")] });
    say(reply(Some("agent"), "claude-haiku-5-5", vec![said(&essay), reading("read")]));
    say(answer(Some("agent"), "read", &essay));
    say(EventBody::SubagentContext { subagent: "thread-1".into(), tokens: 1_200 });
    say(EventBody::SubagentContext { subagent: "thread-1".into(), tokens: 1_200 });
    say(ended("agent", TaskOutcome::Finished, Some("Found it.")));
    say(message(Some("agent"), vec![said("Late."), reading("late")]));
    // So it starts again later than the other started, which puts it after the other.
    let thread_started = subagent_started_at(&store, "thread-1");
    waiting::thread::until(std::time::Duration::from_secs(10), "the clock stood still", || now_ms() > thread_started);
    say(start("toolu_send", "agent"));
    say(start("toolu_9", "thread-2"));
    say(exited());
    say(started("third"));

    let (kept, _) = store.list_subagents(&"s".into()).unwrap();

    let unrevised = |subagents: Vec<Subagent>| -> Vec<Subagent> {
        subagents.into_iter().map(|subagent| Subagent { rev: Rev::default(), ..subagent }).collect()
    };
    let made = Subagent::recorded_from(&log);
    let ids: Vec<SubagentId> = kept.iter().map(Subagent::id).collect();
    assert_eq!(ids, [SubagentId::from("thread-1"), "agent".into(), "third".into()], "by when each last started");
    assert_eq!(unrevised(kept), unrevised(made));
}

fn subagent_started_at(store: &Store, id: &str) -> i64 {
    store.get_subagent(&"s".into(), &id.into()).unwrap().unwrap().started_at_ms
}

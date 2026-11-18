//! The rule's rows for a session, and for its subagents.

use protocol::event::{ContentBlock, EventBody};
use protocol::fixtures;
use protocol::model::{DraftToKeep, QueuedPrompt, Rev, Seq, SessionStatus};
use protocol::prompt::Prompt;

use super::super::tests::notice;
use super::super::{Queued, StatusChange, Store, TurnEdge};
use super::rule::Said;

pub(super) fn session(store: &Store) -> Rev {
    store.get_session(&"s".into()).unwrap().session.rev
}

/// A prompt, and the end of its turn.
pub(super) fn replied(store: &Store) {
    let ends = StatusChange {
        status: SessionStatus::Idle,
        turn: TurnEdge::Ends(Some(protocol::event::TurnOutcome::Success)),
        background_tasks: None,
    };
    store.append(&"s".into(), None, &notice("prompt"), None, None, Some(SessionStatus::Running.into())).unwrap();
    store.append(&"s".into(), None, &notice("done"), None, None, Some(ends.into())).unwrap();
}

pub(super) fn read(store: &Store) -> Said {
    Said::Changed(store.mark_read(&"s".into(), Seq::new(u64::MAX)).unwrap().changed)
}

pub(super) fn replied_and_read(store: &Store) {
    replied(store);
    read(store);
}

pub(super) fn unread(store: &Store) -> Said {
    Said::Changed(store.mark_unread(&"s".into()).unwrap().changed)
}

pub(super) fn draft(store: &Store) -> Said {
    Said::Changed(store.set_draft(&"s".into(), &DraftToKeep::new("same")).unwrap().changed)
}

pub(super) fn drafted(store: &Store) {
    draft(store);
}

pub(super) fn no_draft(store: &Store) -> Said {
    Said::Changed(store.set_draft(&"s".into(), &DraftToKeep::default()).unwrap().changed)
}

pub(super) fn queue(store: &Store) -> Said {
    let prompt = QueuedPrompt { client_msg_id: "a".into(), prompt: Prompt::new("first") };
    let Queued::Done(queuing) = store.queue_prompt(&"s".into(), &prompt).unwrap() else { panic!("refused") };
    Said::Changed(queuing.reading.changed)
}

pub(super) fn queued(store: &Store) {
    queue(store);
}

pub(super) fn unqueue(store: &Store) -> Said {
    Said::Changed(store.unqueue_prompt(&"s".into(), &"a".into()).unwrap().changed)
}

pub(super) fn subagent(store: &Store) -> Rev {
    store.get_subagent(&"s".into(), &"agent".into()).unwrap().unwrap().rev
}

/// Whether recording `body` changed a subagent's record.
fn recorded(store: &Store, body: &EventBody) -> Said {
    Said::Changed(!store.append(&"s".into(), None, body, None, None, None).unwrap().subagents.is_empty())
}

pub(super) fn started(store: &Store) {
    recorded(store, &fixtures::subagent_started("agent", false));
}

pub(super) fn model(store: &Store) -> Said {
    let reply = EventBody::Assistant {
        message_id: None,
        subagent: Some("agent".into()),
        model: Some("claude-haiku-5-5".into()),
        blocks: vec![ContentBlock::Thinking { text: "x".into() }],
        first_index: None,
    };
    recorded(store, &reply)
}

pub(super) fn holds(store: &Store) -> Said {
    recorded(store, &EventBody::SubagentContext { subagent: "agent".into(), tokens: 1_200 })
}

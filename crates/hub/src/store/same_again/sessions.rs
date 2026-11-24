//! The rule's rows for a session, for its subagents, and for how its replies were heard.

use protocol::capability::Capabilities;
use protocol::event::{ContentBlock, EventBody};
use protocol::fixtures;
use protocol::model::{DraftToKeep, Ending, QueuedPrompt, Rev, RunCapabilities, RunId, Seq, SessionStatus, SettledBy};
use protocol::prompt::Prompt;

use super::super::tests::notice;
use super::super::{Queued, StatusChange, Store, TurnEdge};
use super::rule::Said;
use protocol::prompt::PromptFrom;

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
    Said::Changed(store.set_draft(&"s".into(), &DraftToKeep::new("same"), None).unwrap().changed)
}

pub(super) fn drafted(store: &Store) {
    draft(store);
}

pub(super) fn no_draft(store: &Store) -> Said {
    Said::Changed(store.set_draft(&"s".into(), &DraftToKeep::default(), None).unwrap().changed)
}

pub(super) fn queue(store: &Store) -> Said {
    let prompt = QueuedPrompt { client_msg_id: "a".into(), prompt: Prompt::new("first"), from: PromptFrom::Person };
    let Queued::Done(queuing) = store.queue_prompt(&"s".into(), &prompt).unwrap() else { panic!("refused") };
    Said::Changed(queuing.reading.changed)
}

pub(super) fn queued(store: &Store) {
    queue(store);
}

pub(super) fn unqueue(store: &Store) -> Said {
    Said::Changed(store.unqueue_prompt(&"s".into(), &"a".into()).unwrap().reading.changed)
}

/// As a hub carrying on with a run puts them back, and again as the run's next launch finds it running.
pub(super) fn claims(store: &Store) -> Said {
    let claims = RunCapabilities::new(RunId::new(1), Capabilities::default());
    Said::Changed(store.set_claims(&"s".into(), &claims).unwrap())
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

/// As a device says again how a reply ended, after a lost reply.
pub(super) fn reply_heard(store: &Store) -> Said {
    Said::Changed(
        store.keep_reply_heard(&"s".into(), &"m".into(), Ending::ToTheEnd, SettledBy::Device).unwrap().is_some(),
    )
}

pub(super) fn reply_heard_rev(store: &Store) -> Rev {
    let heard = store.list_session_records(&"s".into()).unwrap().replies_heard;
    heard.first().expect("one reply heard").rev
}

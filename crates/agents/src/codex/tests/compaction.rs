//! A compaction someone asked of Codex, which it carries out in a turn of its own, with no message of the person's.

use protocol::event::{CompactedBy, CompactionOutcome, EventBody, TurnOutcome};
use protocol::model::Seq;
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::harness::Harness;
use super::script::{item, message, turn};
use crate::testing::Asking;
use crate::{AgentOutput, AgentProcess};

/// Codex's compaction in the turn `id`, starting, or ending once it's `done`, 400 ms on.
fn compaction(id: &str, done: bool) -> Value {
    let completed = done.then_some(1_400);
    let compaction = json!({"type":"contextCompaction","id":"compact-1","startedAtMs":1_000,"completedAtMs":completed});
    item("thread", id, compaction, done)
}

/// Codex's answer to the prompt, a turn `id` that says hello.
fn answering(id: &str) -> Vec<Value> {
    vec![
        json!({"result":{"turn":{"id":id}}}),
        turn("thread", id, "inProgress"),
        item("thread", id, json!({"type":"userMessage"}), true),
        message("thread", id, "Hello"),
        turn("thread", id, "completed"),
        json!({"method":"test/finished","params":{}}),
    ]
}

/// What the adapter records of prompts, compactions and turns' ends until the fake app server says it's finished.
async fn until_finished(h: &mut Harness) -> Vec<EventBody> {
    let mut said = Vec::new();
    loop {
        let AgentOutput::Event { body, .. } = h.next().await else { continue };
        if matches!(&body, EventBody::AgentOther { kind, .. } if kind == "test/finished") {
            return said;
        }
        if matches!(
            body,
            EventBody::PromptTaken { .. }
                | EventBody::PromptDropped { .. }
                | EventBody::AgentCompacting { .. }
                | EventBody::AgentCompacted { .. }
                | EventBody::TurnEnded { .. }
        ) {
            said.push(body);
        }
    }
}

fn only_compacted(end: &EventBody) -> Option<(TurnOutcome, bool)> {
    let EventBody::TurnEnded { outcome, only_compacted, .. } = end else { return None };
    Some((*outcome, *only_compacted))
}

#[tokio::test]
async fn a_compaction_asked_of_codex_is_taken_as_its_turn_starts_and_named_on_what_it_compacted() {
    let compacting = vec![
        json!({"result":{}}),
        turn("thread", "compacting", "inProgress"),
        compaction("compacting", false),
        compaction("compacting", true),
        turn("thread", "compacting", "completed"),
        json!({"method":"test/finished","params":{}}),
    ];
    let mut h = Harness::new(&json!([
        {"on":"turn/start","actions":answering("first")},
        {"on":"thread/compact/start","params":{"threadId":"thread"},"actions":compacting},
    ]));
    let agent = h.started().await;
    agent.prompt(Seq::FIRST, &Prompt::new("Say hello")).await.unwrap();
    until_finished(&mut h).await;

    agent.compact(Seq::new(3), None).await.unwrap();
    let said = until_finished(&mut h).await;
    agent.stop().await;

    let outcome = CompactionOutcome::Compacted {
        by: CompactedBy::Asked,
        before_tokens: None,
        after_tokens: None,
        took_ms: Some(400),
        summary: None,
    };
    let compacted = EventBody::AgentCompacted { asked: Some(Seq::new(3)), keeping: None, outcome };
    assert_eq!(
        said[..3],
        [EventBody::PromptTaken { prompt: Seq::new(3) }, EventBody::AgentCompacting { written_chars: None }, compacted]
    );
    assert_eq!(said[3..].iter().map(only_compacted).collect::<Vec<_>>(), [Some((TurnOutcome::Success, true))]);
}

/// Codex refusing it says so, and leaves nothing to take the next turn for the compaction's.
#[tokio::test]
async fn a_compaction_codex_refuses_is_refused_and_the_next_turn_is_the_next_prompts() {
    let mut h = Harness::new(&json!([
        {"on":"thread/compact/start","actions":[{"error":{"code":-32600,"message":"busy"}}]},
        {"on":"turn/start","actions":answering("next")},
    ]));
    let agent = h.started().await;

    let refused = agent.compact(Seq::new(3), None).await;
    agent.prompt(Seq::new(4), &Prompt::new("Say hello")).await.unwrap();
    let said = until_finished(&mut h).await;
    agent.stop().await;

    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(said[0], EventBody::PromptTaken { prompt: Seq::new(4) });
    assert_eq!(said[1..].iter().map(only_compacted).collect::<Vec<_>>(), [Some((TurnOutcome::Success, false))]);
}

/// Codex reads nothing to keep, so it isn't sent any, and one asked to keep something is refused before it's sent.
#[tokio::test]
async fn codex_is_asked_to_compact_with_nothing_to_keep() {
    let mut h = Harness::new(&json!([]));
    let agent = h.started().await;

    let keeping = agent.compact(Seq::new(3), Some("The API")).await;
    agent.stop().await;

    assert!(keeping.is_err(), "{keeping:?}");
}

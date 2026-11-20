//! When a line is said for the agent, and what the stand-in is asked.

use std::path::PathBuf;

use protocol::capability::calls::Call;
use protocol::event::{ContentBlock, EventBody, TurnOutcome};
use protocol::fixtures::turn_ended;
use protocol::model::Seq;
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use tokio::sync::mpsc;

use super::Starting;
use super::lines::{BUSY, Covering, THINKING};

fn covering() -> (Covering, mpsc::Sender<crate::AgentOutput>) {
    let (output, _) = mpsc::channel(1);
    let starting = Starting { bin: PathBuf::from("claude"), login: None, environment: process::Environment::NONE };
    (Covering::new(starting, &output), output)
}

/// What the agent itself, or `call`'s subagent, records.
fn says(call: Option<&str>, block: ContentBlock) -> EventBody {
    EventBody::Assistant {
        message_id: Some("m1".into()),
        subagent: call.map(Into::into),
        model: None,
        blocks: vec![block],
        first_index: None,
    }
}

fn words(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into(), images: Vec::new() }
}

/// What the agent itself, or `call`'s subagent, streams.
fn streams(call: Option<&str>, change: DeltaChange) -> StreamDelta {
    StreamDelta { run: None, message_id: "m2".into(), subagent: call.map(Into::into), change }
}

#[test]
fn only_a_prompt_said_out_loud_is_covered_and_a_busy_agent_has_less_time_to_answer() {
    let (covering, _output) = covering();

    assert_eq!(covering.asking(Seq::new(1), "Fix the test", false), None, "typed");
    let (asked, wait) = covering.asking(Seq::new(2), "How's it going?", true).unwrap();
    assert_eq!(wait, THINKING, "it's thinking about what was said");
    assert!(asked.starts_with("The person just said: \"How's it going?\""), "{asked}");
    assert!(
        asked.contains("- the person said: Fix the test"),
        "what was typed before is what it's been doing: {asked}"
    );

    covering.heard(&EventBody::PromptTaken { prompt: Seq::new(2) });
    let (asked, wait) = covering.asking(Seq::new(3), "And the docs?", true).unwrap();
    assert_eq!(wait, BUSY, "it's on a turn");
    assert!(asked.contains("You're in the middle of something else."), "{asked}");
}

/// Thinking, a call and a subagent's words are the agent still busy. Its own words answer, and so does its turn ending.
#[test]
fn the_agents_own_words_or_its_turn_ending_answer_and_nothing_else_does() {
    let (covering, _output) = covering();
    let busy: [&dyn Fn(&Covering); 4] = [
        &|c| c.heard(&says(None, ContentBlock::Thinking { text: "Which test?".into() })),
        &|c| c.heard(&says(Some("toolu-1"), words("Found it."))),
        &|c| c.writing(&streams(None, DeltaChange::Thinking { index: 0, text: "Hmm".into() })),
        &|c| c.writing(&streams(Some("toolu-1"), DeltaChange::Text { index: 0, text: "Found".into() })),
    ];
    let answers: [&dyn Fn(&Covering); 4] = [
        &|c| c.heard(&says(None, words("Found it."))),
        &|c| {
            c.heard(&EventBody::PromptTaken { prompt: Seq::new(21) });
            c.heard(&turn_ended(TurnOutcome::Success));
        },
        &|c| c.writing(&streams(None, DeltaChange::Text { index: 0, text: "Found".into() })),
        &|c| c.writing(&streams(None, DeltaChange::BlockStart { index: 0, kind: BlockKind::Text, tool_name: None })),
    ];
    for (n, still_busy) in busy.iter().enumerate() {
        covering.asking(Seq::new(10 + n as u64), "Done yet?", true);
        still_busy(&covering);
        assert!(covering.unanswered(Seq::new(10 + n as u64)), "busy {n}");
    }
    for (n, answer) in answers.iter().enumerate() {
        covering.asking(Seq::new(20 + n as u64), "Done yet?", true);
        answer(&covering);
        assert!(!covering.unanswered(Seq::new(20 + n as u64)), "answer {n}");
    }
}

/// A line is said once, and only for the newest prompt said out loud.
#[test]
fn a_prompt_is_covered_once_and_a_newer_one_takes_its_place() {
    let (covering, _output) = covering();
    covering.asking(Seq::new(1), "Done yet?", true);
    covering.asking(Seq::new(2), "Actually, stop", true);

    assert!(!covering.unanswered(Seq::new(1)), "an older prompt");
    assert!(covering.unanswered(Seq::new(2)));
    assert!(!covering.unanswered(Seq::new(2)), "once");
}

#[test]
fn the_stand_in_is_told_what_the_agent_did_in_a_line_each() {
    let (covering, _output) = covering();
    let command: Call =
        serde_json::from_value(serde_json::json!({"kind": "command", "command": "cargo test -p hub"})).unwrap();
    covering.heard(&says(None, ContentBlock::Call { id: "toolu-2".into(), call: command }));
    covering.heard(&says(None, words("The pump\ntests fail.")));

    let (asked, _) = covering.asking(Seq::new(1), "Why?", true).unwrap();

    assert!(asked.ends_with("- called command: cargo test -p hub\n- said: The pump tests fail."), "{asked}");
}

/// A prompt sent while a turn runs can be taken only once that turn ends, so that end isn't the agent answering it.
#[test]
fn the_end_of_the_turn_before_the_prompt_was_taken_doesnt_answer_it() {
    let (covering, _output) = covering();
    covering.heard(&EventBody::PromptTaken { prompt: Seq::new(1) });
    covering.asking(Seq::new(2), "And the docs?", true);

    covering.heard(&turn_ended(TurnOutcome::Success));
    assert!(covering.unanswered(Seq::new(2)), "the turn before it ended");

    covering.asking(Seq::new(3), "And the docs?", true);
    covering.heard(&EventBody::PromptTaken { prompt: Seq::new(3) });
    covering.heard(&turn_ended(TurnOutcome::Success));
    assert!(!covering.unanswered(Seq::new(3)), "its own turn ended");
}

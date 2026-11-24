//! When a wait is opened, when a line is said in it, when it closes, and what the stand-in is asked.

use std::path::PathBuf;

use protocol::capability::Report;
use protocol::capability::calls::Call;
use protocol::capability::output::{CoverLine, CoverWait};
use protocol::event::{AgentRunState, ContentBlock, EventBody, TurnOutcome};
use protocol::fixtures::turn_ended;
use protocol::model::{InteractionMode, Seq};
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use tokio::sync::mpsc;

use super::Starting;
use super::waits::{BUSY, Covering, THINKING};
use crate::AgentOutput;

/// The cover lines of an agent launched in `mode`, with a spare that never starts, since its CLI isn't there.
fn launched(mode: InteractionMode) -> (Covering, mpsc::Sender<AgentOutput>) {
    let (output, _) = mpsc::channel(1);
    let starting = Starting { bin: PathBuf::from("not-claude"), login: None, environment: process::Environment::NONE };
    (Covering::new(starting, mode, &output), output)
}

fn on() -> (Covering, mpsc::Sender<AgentOutput>) {
    launched(InteractionMode::Conversation)
}

/// What the agent itself, or `call`'s subagent, records.
fn says(call: Option<&str>, block: ContentBlock) -> AgentOutput {
    AgentOutput::event(
        EventBody::Assistant {
            message_id: Some("m1".into()),
            subagent: call.map(Into::into),
            model: None,
            blocks: vec![block],
            first_index: None,
        },
        None,
    )
}

fn heard(body: EventBody) -> AgentOutput {
    AgentOutput::event(body, None)
}

fn words(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into(), images: Vec::new() }
}

/// What the agent itself, or `call`'s subagent, streams.
fn streams(call: Option<&str>, change: DeltaChange) -> AgentOutput {
    AgentOutput::Delta(StreamDelta { run: None, message_id: "m2".into(), subagent: call.map(Into::into), change })
}

fn line(wait: u32) -> Report {
    Report::Covering(CoverLine { wait: CoverWait(wait), number: 1, text: "Hold on.".into() })
}

/// Only something said out loud while the mode is on opens a wait, and a busy agent has less time to answer.
#[tokio::test]
async fn only_what_was_said_out_loud_in_the_mode_opens_a_wait_and_a_busy_agent_has_less_time() {
    let (covering, _output) = on();

    assert!(covering.opening(Seq::new(1), "Fix the test", false).is_none(), "typed");
    let (wait, asked, time, _) = covering.opening(Seq::new(2), "How's it going?", true).unwrap();
    assert_eq!((wait, time), (CoverWait(1), THINKING), "it's thinking about what was said");
    assert!(asked.starts_with("The person just said: \"How's it going?\""), "{asked}");
    assert!(asked.contains("- the person said: Fix the test"), "what was typed is what it's been doing: {asked}");

    covering.took_in(&heard(EventBody::AgentState { state: AgentRunState::Running }));
    let (wait, asked, time, _) = covering.opening(Seq::new(3), "And the docs?", true).unwrap();
    assert_eq!((wait, time), (CoverWait(2), BUSY), "it's on a turn, whoever started it");
    assert!(asked.contains("You're in the middle of something else."), "{asked}");
    covering.took_in(&heard(EventBody::AgentState { state: AgentRunState::Idle }));
    let (_, _, time, _) = covering.opening(Seq::new(4), "Done?", true).unwrap();
    assert_eq!(time, THINKING, "it's done with its turn");
    covering.took_in(&heard(EventBody::AgentInit {
        agent_session_id: None,
        model: None,
        cwd: None,
        mode: None,
        agent_version: None,
    }));
    let (_, _, time, _) = covering.opening(Seq::new(6), "And now?", true).unwrap();
    assert_eq!(time, BUSY, "a turn started straight after another, with no change of state between");

    let (off, _output) = launched(InteractionMode::Typing);
    assert!(off.opening(Seq::new(5), "Anyone there?", true).is_none(), "the mode is off");
}

/// Once the agent has taken what was said, its own words answer it, and so does that turn ending, and the CLI
/// dropping it ends the wait too. Thinking, a subagent's words, and words written before it was taken, which are of
/// what the agent was busy with, don't. The wait closes, saying so if a line was said in it.
#[tokio::test]
async fn the_agents_own_words_or_its_turn_ending_close_the_wait_and_nothing_else_does() {
    let taken = || heard(EventBody::PromptTaken { prompt: Seq::new(1) });
    let busy = [
        vec![taken(), says(None, ContentBlock::Thinking { text: "Which test?".into() })],
        vec![taken(), says(Some("toolu-1"), words("Found it."))],
        vec![taken(), streams(None, DeltaChange::Thinking { index: 0, text: "Hmm".into() })],
        vec![taken(), streams(Some("toolu-1"), DeltaChange::Text { index: 0, text: "Found".into() })],
        vec![says(None, words("Now the tests."))],
        vec![streams(None, DeltaChange::Text { index: 0, text: "Now".into() })],
        vec![heard(EventBody::PromptDropped { prompt: Seq::new(9) })],
    ];
    let answers = [
        vec![taken(), says(None, words("Found it."))],
        vec![taken(), heard(turn_ended(TurnOutcome::Success))],
        vec![taken(), streams(None, DeltaChange::Text { index: 0, text: "Found".into() })],
        vec![taken(), streams(None, DeltaChange::BlockStart { index: 0, kind: BlockKind::Text, tool_name: None })],
        vec![heard(EventBody::PromptDropped { prompt: Seq::new(1) })],
    ];
    for (n, still_busy) in busy.iter().enumerate() {
        let (covering, _output) = on();
        covering.opening(Seq::new(1), "Done yet?", true);
        let closed: Vec<_> = still_busy.iter().filter_map(|item| covering.took_in(item)).collect();
        assert!(closed.is_empty(), "busy {n}");
        assert_eq!(covering.unanswered(CoverWait(1), "Hold on.".into()), Some(line(1)), "busy {n}");
    }
    for (n, answer) in answers.iter().enumerate() {
        let (covering, _output) = on();
        covering.opening(Seq::new(1), "Done yet?", true);
        covering.unanswered(CoverWait(1), "Hold on.".into());
        let closed: Vec<_> = answer.iter().filter_map(|item| covering.took_in(item)).collect();
        assert_eq!(closed, [Report::Covered { wait: CoverWait(1) }], "answer {n}");
        assert_eq!(covering.unanswered(CoverWait(1), "Hold on.".into()), None, "answer {n}");
    }
}

/// A wait has one line, and the newest thing said opens one of its own in place of any open, which closes, saying so
/// if a line was said in it. One closed with nothing said in it closes quietly.
#[tokio::test]
async fn a_wait_has_one_line_and_a_newer_one_takes_its_place() {
    let (covering, _output) = on();
    covering.opening(Seq::new(1), "Done yet?", true);
    let (_, _, _, closed) = covering.opening(Seq::new(2), "Actually, stop", true).unwrap();
    assert_eq!(closed, None, "nothing was said in it");

    assert_eq!(covering.unanswered(CoverWait(1), "Hold on.".into()), None, "an older wait");
    assert_eq!(covering.unanswered(CoverWait(2), "Hold on.".into()), Some(line(2)));
    assert_eq!(covering.unanswered(CoverWait(2), "Hold on.".into()), None, "once");
    let (_, _, _, closed) = covering.opening(Seq::new(3), "And then?", true).unwrap();
    assert_eq!(closed, Some(Report::Covered { wait: CoverWait(2) }));
}

/// Turning the mode off closes the wait, since nobody is listening, and no line is said after, or opened, until it's
/// turned on again.
#[tokio::test]
async fn turning_the_mode_off_closes_the_wait() {
    let (covering, _output) = on();
    covering.opening(Seq::new(1), "Done yet?", true);
    covering.unanswered(CoverWait(1), "Hold on.".into());

    assert_eq!(covering.turned(InteractionMode::Typing), Some(Report::Covered { wait: CoverWait(1) }));
    assert!(covering.opening(Seq::new(2), "Still there?", true).is_none(), "nobody is listening");
    covering.turned(InteractionMode::Conversation);
    let (wait, ..) = covering.opening(Seq::new(3), "Still there?", true).unwrap();
    assert_eq!(covering.turned(InteractionMode::Typing), None, "nothing was said in it");
    assert_eq!(covering.unanswered(wait, "Hold on.".into()), None, "said after the mode went off");
}

#[tokio::test]
async fn the_stand_in_is_told_what_the_agent_did_in_a_line_each() {
    let (covering, _output) = on();
    let command: Call =
        serde_json::from_value(serde_json::json!({"kind": "command", "command": "cargo test -p hub"})).unwrap();
    covering.took_in(&says(None, ContentBlock::Call { id: "toolu-2".into(), call: command }));
    covering.took_in(&says(None, words("The pump\ntests fail.")));

    let (_, asked, _, _) = covering.opening(Seq::new(1), "Why?", true).unwrap();

    assert!(asked.ends_with("- called command: cargo test -p hub\n- said: The pump tests fail."), "{asked}");
}

/// Something said while a turn runs can be taken only once that turn ends, so that end isn't the agent answering it.
#[tokio::test]
async fn the_end_of_the_turn_before_it_was_taken_doesnt_close_the_wait() {
    let (covering, _output) = on();
    covering.took_in(&heard(EventBody::PromptTaken { prompt: Seq::new(1) }));
    covering.opening(Seq::new(2), "And the docs?", true);

    covering.took_in(&heard(turn_ended(TurnOutcome::Success)));
    assert_eq!(covering.unanswered(CoverWait(1), "Hold on.".into()), Some(line(1)), "the turn before it ended");

    covering.opening(Seq::new(3), "And the docs?", true);
    covering.took_in(&heard(EventBody::PromptTaken { prompt: Seq::new(3) }));
    covering.took_in(&heard(turn_ended(TurnOutcome::Success)));
    assert_eq!(covering.unanswered(CoverWait(2), "Hold on.".into()), None, "its own turn ended");
}

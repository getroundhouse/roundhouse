//! Checks the adapter against the real `claude` CLI's work in the background. `../fixtures/claude_background.jsonl`
//! was captured by `tools/smoke/capture_claude_fixture.py --background`: a command the agent left running, which
//! finished once its turn had ended, and the turn the agent started by itself to take that in, then another, still
//! running when SIGTERM stopped the CLI. Rerun it after a CLI upgrade. The hub keeps a session at work from the last of
//! that work going until the agent says what it does next, so a failure here means it may wait for nothing.

#![cfg(test)]

use agents::claude::{Normalizer, Parsed};
use protocol::event::{AgentRunState, EventBody};

const BACKGROUND: &str = include_str!("../fixtures/claude_background.jsonl");

fn events() -> Vec<EventBody> {
    let mut normalizer = Normalizer::default();
    BACKGROUND
        .lines()
        .filter_map(|line| match normalizer.line(line).parsed {
            Parsed::Event(body) | Parsed::EventWithImages { body, .. } => Some(body),
            _ => None,
        })
        .collect()
}

/// The first of what comes after `at` that says how the agent stands: running, idle, a turn starting or ending.
fn next_word_on_the_agent(events: &[EventBody], at: usize) -> Option<&EventBody> {
    events[at + 1..].iter().find(|body| {
        matches!(body, EventBody::AgentState { .. } | EventBody::AgentInit { .. } | EventBody::TurnEnded { .. })
    })
}

/// Where each list that left nothing running is, with whether a turn had ended before it, since the last started.
fn emptied(events: &[EventBody]) -> Vec<(usize, &EventBody, bool)> {
    let mut between_turns = false;
    let mut found = Vec::new();
    for (at, body) in events.iter().enumerate() {
        match body {
            EventBody::TurnEnded { .. } => between_turns = true,
            EventBody::AgentInit { .. } | EventBody::AgentState { state: AgentRunState::Running } => {
                between_turns = false;
            }
            EventBody::AgentTasks { tasks, .. } if tasks.is_empty() => found.push((at, body, between_turns)),
            _ => {}
        }
    }
    found
}

#[test]
fn once_its_work_finishes_between_turns_the_agent_says_next_that_its_running_again() {
    let events = events();
    let (at, list, between_turns) = emptied(&events)[0];

    assert!(between_turns, "the command finished during a turn");
    assert!(matches!(list, EventBody::AgentTasks { stopped, .. } if stopped.is_empty()), "{list:?}");
    let next = next_word_on_the_agent(&events, at);
    assert!(matches!(next, Some(EventBody::AgentState { state: AgentRunState::Running })), "{next:?}");
}

/// As the worker stops an agent, with work still going. The CLI exits next, which the capture doesn't hold.
#[test]
fn sigterm_stops_the_agents_work_and_says_so_before_it_exits() {
    let events = events();
    let &(at, list, between_turns) = emptied(&events).last().unwrap();

    assert!(between_turns);
    assert!(matches!(list, EventBody::AgentTasks { stopped, .. } if stopped.len() == 1), "{list:?}");
    assert_eq!(next_word_on_the_agent(&events, at), None, "it said more before it exited");
}

#[test]
fn every_line_is_understood_or_deliberately_kept() {
    let mut normalizer = Normalizer::default();
    for (i, line) in BACKGROUND.lines().enumerate() {
        if let Parsed::Event(EventBody::AgentOther { kind, .. }) = normalizer.line(line).parsed {
            let needs_handling = ["unparsed", "assistant", "user", "result", "stream_event"];
            assert!(!needs_handling.contains(&kind.as_str()), "line {i}: {kind}");
        }
    }
}

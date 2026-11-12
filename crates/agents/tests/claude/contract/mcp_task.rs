//! A call to a tool answered with an MCP task, as the worker's `wait_for_pull_request` is, in the real CLI's lines in
//! `../../fixtures/claude_mcp_task.jsonl`, then the same conversation resumed after SIGTERM, as a hub starts an agent
//! again once its wait ends, in `claude_mcp_task_resumed.jsonl`. Each is read as its own process's, as each launch
//! has a normalizer of its own.

use agents::claude::{Normalizer, Parsed};
use protocol::event::{EventBody, TaskKind};

const STOPPED: &str = include_str!("../../fixtures/claude_mcp_task.jsonl");
const RESUMED: &str = include_str!("../../fixtures/claude_mcp_task_resumed.jsonl");

fn events(lines: &str) -> Vec<EventBody> {
    let mut normalizer = Normalizer::default();
    lines
        .lines()
        .filter_map(|line| match normalizer.line(line).parsed {
            Parsed::Event(body) | Parsed::EventWithImages { body, .. } => Some(body),
            _ => None,
        })
        .collect()
}

/// Where each turn ends, and each list of what's in the background, in order, as `Ended` and the list's kinds.
#[derive(Debug, PartialEq)]
enum Step {
    Listed(Vec<TaskKind>),
    Ended,
}

fn steps(events: &[EventBody]) -> Vec<Step> {
    events
        .iter()
        .filter_map(|event| match event {
            EventBody::AgentTasks { tasks, .. } => Some(Step::Listed(tasks.iter().map(|task| task.kind).collect())),
            EventBody::TurnEnded { .. } => Some(Step::Ended),
            _ => None,
        })
        .collect()
}

/// Each task the tool answered with is a wait from the list that first has it, which comes before the turn ends.
/// Once it completes, the list empties, and the CLI takes a turn by itself, to say how it ended.
#[test]
fn a_task_the_wait_tool_answered_with_is_a_wait_and_its_end_starts_a_turn() {
    let steps = steps(&events(STOPPED));

    let waiting = Step::Listed(vec![TaskKind::Wait]);
    assert_eq!(
        steps,
        [waiting, Step::Ended, Step::Listed(vec![]), Step::Ended, Step::Listed(vec![TaskKind::Wait]), Step::Ended]
    );
}

/// Started again with `--resume`, the CLI lists the task again before anything else, as a wait, though the call that
/// started it was the stopped process's, then takes a turn by itself once it completes.
#[test]
fn a_resumed_cli_lists_its_wait_again_first_and_takes_a_turn_once_it_ends() {
    let events = events(RESUMED);

    assert!(matches!(&events[0], EventBody::AgentTasks { .. }), "first: {:?}", events[0]);
    let steps = steps(&events);
    assert_eq!(steps, [Step::Listed(vec![TaskKind::Wait]), Step::Listed(vec![]), Step::Ended]);
}

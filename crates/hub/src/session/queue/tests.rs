use protocol::event::{AgentRunState, EventBody, ExitReason, TurnOutcome};
use protocol::model::SessionStatus;

use super::*;

fn prompt() -> EventBody {
    EventBody::UserPrompt { text: "x".into(), client_msg_id: None, attachments: Vec::new() }
}

fn idle() -> EventBody {
    EventBody::AgentState { state: AgentRunState::Idle }
}

fn ended(outcome: TurnOutcome) -> EventBody {
    EventBody::TurnEnded {
        outcome,
        result: None,
        errors: Vec::new(),
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
    }
}

fn ready(watch: &QueueWatch) -> bool {
    watch.ready(SessionStatus::Idle, false)
}

fn exited(reason: ExitReason) -> EventBody {
    EventBody::AgentExited { reason, code: None, signal: None, stderr_tail: vec![] }
}

#[test]
fn the_queue_waits_until_the_agent_has_taken_every_prompt_it_was_sent() {
    let mut watch = QueueWatch::default();
    watch.set_untaken(vec![1, 2]);
    assert!(!ready(&watch));

    watch.set_untaken(vec![2]);
    assert!(!ready(&watch), "one still waiting");
    watch.set_untaken(Vec::new());
    assert!(ready(&watch));
    assert!(!watch.ready(SessionStatus::Running, false) && !watch.ready(SessionStatus::Idle, true));
}

#[test]
fn an_idle_with_a_prompt_the_agent_has_yet_to_take_is_from_before_it() {
    let mut watch = QueueWatch::default();
    assert!(!watch.idle_is_stale());

    watch.set_untaken(vec![3]);

    assert!(watch.idle_is_stale());
}

/// What recording an event that takes the status `from` one `to` another does to the watch, as `record` does it.
fn record(watch: &mut QueueWatch, body: &EventBody, from: SessionStatus, to: SessionStatus) {
    if let Some(held) = watch.held_after(body, from, to) {
        watch.hold(held);
    }
}

#[test]
fn a_stop_holds_the_queue_until_the_person_sends_a_prompt() {
    use SessionStatus::{Idle, Running};
    let mut watch = QueueWatch::default();
    record(&mut watch, &ended(TurnOutcome::Cancelled), Running, Idle);
    assert!(!ready(&watch));
    record(&mut watch, &idle(), Idle, Idle);
    assert_eq!(watch.held_after(&ended(TurnOutcome::Success), Running, Idle), None, "only a prompt lets it go");

    record(&mut watch, &prompt(), Idle, Running);
    record(&mut watch, &EventBody::PromptTaken { prompt: 3 }, Running, Running);
    record(&mut watch, &ended(TurnOutcome::Success), Running, Idle);
    assert!(ready(&watch));
}

#[test]
fn an_agent_gone_mid_turn_holds_the_queue_and_once_let_go_its_sent_to_a_new_one() {
    use SessionStatus::{Interrupted, Running};
    let mut watch = QueueWatch::default();
    let exited = exited(ExitReason::Unexpected);
    record(&mut watch, &exited, Running, Interrupted);
    assert!(!watch.ready(Interrupted, false));
    assert_eq!(watch.held_after(&exited, Interrupted, Interrupted), None, "only going away holds it");

    watch.hold(false);
    assert!(watch.ready(Interrupted, false));
    assert!(!QueueWatch::starting(true).ready(Interrupted, false), "a hub that finds it held keeps it held");
}

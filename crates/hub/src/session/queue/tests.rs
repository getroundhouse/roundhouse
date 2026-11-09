use protocol::event::{AgentRunState, EventBody, ExitReason, TurnOutcome};
use protocol::model::{RunId, SessionStatus};

use super::*;

const RUN: Origin = Origin::Run { run: RunId::new(7), current: true };
const EARLIER: Origin = Origin::Run { run: RunId::new(6), current: false };

fn prompt() -> EventBody {
    EventBody::UserPrompt { text: "x".into(), client_msg_id: None, attachments: Vec::new(), tracked: true }
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
fn an_agent_that_says_what_it_takes_is_left_to_say_it() {
    let mut watch = QueueWatch::default();
    watch.heard(RUN, &EventBody::PromptTaken { prompt: 1 });
    watch.set_untaken(vec![3]);

    assert!(watch.idle_is_stale(RUN), "the idle was from before the prompt");
    for body in [idle(), ended(TurnOutcome::Success), exited(ExitReason::Unexpected)] {
        assert!(watch.settled_before(RUN, &body).is_empty(), "{body:?}");
    }
}

#[test]
fn the_hub_says_an_agent_that_never_says_what_it_takes_had_them_all_by_the_end_of_its_turn() {
    let mut watch = QueueWatch::default();
    watch.set_untaken(vec![1, 2]);
    let taken = vec![EventBody::PromptTaken { prompt: 1 }, EventBody::PromptTaken { prompt: 2 }];

    assert!(!watch.idle_is_stale(RUN));
    for body in [ended(TurnOutcome::Success), idle(), exited(ExitReason::Unexpected)] {
        assert_eq!(watch.settled_before(RUN, &body), taken, "{body:?}");
    }
    assert!(watch.settled_before(EARLIER, &idle()).is_empty(), "a replaced agent doesn't speak for the session");
    assert!(watch.settled_before(Origin::Hub, &idle()).is_empty());
    assert!(watch.settled_before(RUN, &prompt()).is_empty());
}

#[test]
fn the_hub_says_a_lost_worker_dropped_what_its_agent_hadnt_taken() {
    let mut watch = QueueWatch::default();
    watch.heard(RUN, &EventBody::PromptTaken { prompt: 1 });
    watch.set_untaken(vec![3]);

    let settled = watch.settled_before(RUN, &exited(ExitReason::WorkerLost));

    assert_eq!(settled, [EventBody::PromptDropped { prompt: 3 }]);
}

/// What recording an event that takes the status `from` one `to` another does to the watch, as `record` does it.
fn record(watch: &mut QueueWatch, origin: Origin, body: &EventBody, from: SessionStatus, to: SessionStatus) {
    if let Some(held) = watch.held_after(body, from, to) {
        watch.hold(held);
    }
    watch.heard(origin, body);
}

#[test]
fn a_stop_holds_the_queue_until_the_person_sends_a_prompt() {
    use SessionStatus::{Idle, Running};
    let mut watch = QueueWatch::default();
    record(&mut watch, RUN, &ended(TurnOutcome::Cancelled), Running, Idle);
    assert!(!ready(&watch));
    record(&mut watch, RUN, &idle(), Idle, Idle);
    assert_eq!(watch.held_after(&ended(TurnOutcome::Success), Running, Idle), None, "only a prompt lets it go");

    record(&mut watch, Origin::Hub, &prompt(), Idle, Running);
    record(&mut watch, RUN, &EventBody::PromptTaken { prompt: 3 }, Running, Running);
    record(&mut watch, RUN, &ended(TurnOutcome::Success), Running, Idle);
    assert!(ready(&watch));
}

#[test]
fn an_agent_gone_mid_turn_holds_the_queue_and_once_let_go_its_sent_to_a_new_one() {
    use SessionStatus::{Interrupted, Running};
    let mut watch = QueueWatch::default();
    let exited = exited(ExitReason::Unexpected);
    record(&mut watch, RUN, &exited, Running, Interrupted);
    assert!(!watch.ready(Interrupted, false));
    assert_eq!(watch.held_after(&exited, Interrupted, Interrupted), None, "only going away holds it");

    watch.hold(false);
    assert!(watch.ready(Interrupted, false));
    assert!(!QueueWatch::starting(true).ready(Interrupted, false), "a hub that finds it held keeps it held");
}

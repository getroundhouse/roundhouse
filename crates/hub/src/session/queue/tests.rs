use protocol::event::{AgentRunState, EventBody, ExitReason, TurnOutcome};
use protocol::model::{RunId, Seq, SessionStatus};

use super::*;

const RUN: Origin = Origin::Run { run: RunId::new(7), current: true };
const EARLIER: Origin = Origin::Run { run: RunId::new(6), current: false };

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
    }
}

fn ready(watch: &QueueWatch) -> bool {
    watch.ready(SessionStatus::Idle, false)
}

#[test]
fn the_queue_waits_until_the_agent_has_taken_every_prompt_it_was_sent() {
    let mut watch = QueueWatch::default();
    watch.track(Origin::Hub, 1, &prompt());
    watch.track(Origin::Hub, 2, &prompt());
    assert!(!ready(&watch));

    watch.track(RUN, 3, &EventBody::PromptTaken { prompt: 1 });
    assert!(!ready(&watch), "one still waiting");
    watch.track(RUN, 4, &EventBody::PromptDropped { prompt: 2 });
    assert!(ready(&watch));
    assert!(!watch.ready(SessionStatus::Running, false) && !watch.ready(SessionStatus::Idle, true));
}

#[test]
fn an_idle_from_an_agent_that_says_what_it_takes_doesnt_count_for_a_prompt_it_hasnt() {
    let mut watch = QueueWatch::default();
    watch.track(Origin::Hub, 1, &prompt());
    watch.track(RUN, 2, &EventBody::PromptTaken { prompt: 1 });
    watch.track(Origin::Hub, 3, &prompt());

    assert!(watch.idle_is_stale(RUN));
    watch.track(RUN, 4, &idle());
    assert!(!ready(&watch), "the idle was from before the prompt");
    let exited =
        EventBody::AgentExited { reason: ExitReason::Unexpected, code: None, signal: None, stderr_tail: vec![] };
    watch.track(RUN, 5, &exited);
    assert!(ready(&watch), "an agent that's gone never will take it");
}

#[test]
fn an_idle_from_an_agent_that_never_says_what_it_takes_has_taken_them_all() {
    let mut watch = QueueWatch::default();
    watch.track(Origin::Hub, 1, &prompt());

    assert!(!watch.idle_is_stale(RUN));
    watch.track(EARLIER, 2, &idle());
    assert!(!ready(&watch), "a replaced agent doesn't speak for the session");
    watch.track(RUN, 3, &idle());
    assert!(ready(&watch));
}

/// What recording an event that takes the status `from` one `to` another does to the watch, as `record` does it.
fn record(watch: &mut QueueWatch, origin: Origin, seq: Seq, body: &EventBody, from: SessionStatus, to: SessionStatus) {
    if let Some(held) = watch.held_after(body, from, to) {
        watch.hold(held);
    }
    watch.track(origin, seq, body);
}

#[test]
fn a_stop_holds_the_queue_until_the_person_sends_a_prompt() {
    use SessionStatus::{Idle, Running};
    let mut watch = QueueWatch::default();
    record(&mut watch, RUN, 1, &ended(TurnOutcome::Cancelled), Running, Idle);
    assert!(!ready(&watch));
    record(&mut watch, RUN, 2, &idle(), Idle, Idle);
    assert_eq!(watch.held_after(&ended(TurnOutcome::Success), Running, Idle), None, "only a prompt lets it go");

    record(&mut watch, Origin::Hub, 3, &prompt(), Idle, Running);
    record(&mut watch, RUN, 4, &EventBody::PromptTaken { prompt: 3 }, Running, Running);
    record(&mut watch, RUN, 5, &ended(TurnOutcome::Success), Running, Idle);
    assert!(ready(&watch));
}

#[test]
fn an_agent_gone_mid_turn_holds_the_queue_and_once_let_go_its_sent_to_a_new_one() {
    use SessionStatus::{Interrupted, Running};
    let mut watch = QueueWatch::default();
    let exited =
        EventBody::AgentExited { reason: ExitReason::Unexpected, code: None, signal: None, stderr_tail: vec![] };
    record(&mut watch, RUN, 1, &exited, Running, Interrupted);
    assert!(!watch.ready(Interrupted, false));
    assert_eq!(watch.held_after(&exited, Interrupted, Interrupted), None, "only going away holds it");

    watch.hold(false);
    assert!(watch.ready(Interrupted, false));
    assert!(!QueueWatch::starting(true).ready(Interrupted, false), "a hub that finds it held keeps it held");
}

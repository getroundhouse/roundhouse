use protocol::event::{AgentRunState, EventBody, ExitReason, NoticeLevel, SessionEvent, TurnOutcome};
use protocol::model::Seq;

use super::*;

fn event(seq: Seq, body: EventBody) -> SessionEvent {
    SessionEvent { session_id: "s".into(), seq, ts_ms: 0, run: None, body, raw: None }
}

fn prompt() -> EventBody {
    EventBody::UserPrompt { text: "steer".into(), client_msg_id: None, attachments: Vec::new() }
}

fn notice() -> EventBody {
    EventBody::Notice { level: NoticeLevel::Info, message: "meanwhile".into() }
}

/// A log of these bodies, from seq 1, all loaded.
fn log(bodies: Vec<EventBody>) -> SessionLog {
    let mut log = SessionLog::new(0);
    for (at, body) in (1..).zip(bodies) {
        log.apply_event(event(at, body));
    }
    log
}

#[test]
fn a_prompt_waits_until_the_agent_says_it_took_or_dropped_it() {
    let waiting = log(vec![prompt(), notice(), EventBody::PromptTaken { prompt: 9 }]);
    let taken = log(vec![prompt(), notice(), EventBody::PromptTaken { prompt: 1 }]);
    let dropped = log(vec![prompt(), EventBody::PromptDropped { prompt: 1 }]);

    assert_eq!(waiting.prompt_state(1), PromptState::Waiting, "another prompt's news isn't this one's");
    assert_eq!(taken.prompt_state(1), PromptState::Taken);
    assert_eq!(dropped.prompt_state(1), PromptState::Dropped);
    let sent_again = log(vec![prompt(), EventBody::PromptDropped { prompt: 1 }, EventBody::PromptTaken { prompt: 1 }]);
    assert_eq!(sent_again.prompt_state(1), PromptState::Taken, "another agent took it after all");
}

#[test]
fn an_agent_that_ends_its_turn_goes_idle_or_exits_has_every_prompt() {
    let ended = EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: Vec::new(),
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
    };
    let exited =
        EventBody::AgentExited { reason: ExitReason::Unexpected, code: None, signal: None, stderr_tail: vec![] };

    for after in [ended, EventBody::AgentState { state: AgentRunState::Idle }, exited] {
        assert_eq!(log(vec![prompt(), after.clone()]).prompt_state(1), PromptState::Taken, "{after:?}");
    }
    let running = log(vec![prompt(), EventBody::AgentState { state: AgentRunState::Running }]);
    assert_eq!(running.prompt_state(1), PromptState::Waiting);
}

#[test]
fn starting_a_turn_takes_a_prompt_only_for_an_agent_that_never_says_so() {
    let init = || EventBody::AgentInit {
        agent_session_id: None,
        model: None,
        cwd: None,
        permission_mode: None,
        agent_version: None,
    };
    let silent = log(vec![prompt(), init()]);
    let says = log(vec![prompt(), EventBody::PromptTaken { prompt: 9 }, init()]);

    assert_eq!(silent.prompt_state(1), PromptState::Taken);
    assert_eq!(says.prompt_state(1), PromptState::Waiting);
}

#[test]
fn a_prompt_far_behind_what_is_loaded_was_taken_long_ago() {
    let mut log = SessionLog::new(0);
    log.apply_event(event(1, prompt()));
    log.apply_event(event(3, notice()));

    assert_eq!(log.prompt_state(1), PromptState::Taken, "seq 2 isn't loaded");
}

#[test]
fn what_the_agent_says_about_a_prompt_counts_over_an_idle_or_a_turn_end_from_before_it() {
    let ended = |outcome| EventBody::TurnEnded {
        outcome,
        result: None,
        errors: Vec::new(),
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
    };
    let idle = || EventBody::AgentState { state: AgentRunState::Idle };
    let dropped = EventBody::PromptDropped { prompt: 1 };

    let after_a_late_idle = log(vec![prompt(), idle(), dropped.clone()]);
    let after_a_stopped_turn = log(vec![prompt(), ended(TurnOutcome::Cancelled), idle(), dropped.clone()]);
    let after_the_next_turn = log(vec![prompt(), ended(TurnOutcome::Success), ended(TurnOutcome::Success), dropped]);

    assert_eq!(after_a_late_idle.prompt_state(1), PromptState::Dropped);
    assert_eq!(after_a_stopped_turn.prompt_state(1), PromptState::Dropped);
    assert_eq!(after_the_next_turn.prompt_state(1), PromptState::Taken, "it would have said by then");
    assert_eq!(log(vec![prompt(), idle()]).prompt_state(1), PromptState::Taken, "until it says otherwise");
}

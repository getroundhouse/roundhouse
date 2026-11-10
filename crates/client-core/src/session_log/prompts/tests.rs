use protocol::event::{AgentRunState, EventBody, ExitReason, NoticeLevel, SessionEvent, TurnOutcome};
use protocol::model::{Place, Seq};

use super::*;

fn event(seq: Seq, body: EventBody) -> SessionEvent {
    SessionEvent {
        session_id: "s".into(),
        seq,
        ts_ms: 0,
        run: None,
        subagent: None,
        place: Place::default(),
        body,
        raw: None,
    }
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
fn a_prompt_waits_until_the_log_says_it_was_taken_or_dropped() {
    let waiting = log(vec![prompt(), notice(), EventBody::PromptTaken { prompt: 9 }]);
    let taken = log(vec![prompt(), notice(), EventBody::PromptTaken { prompt: 1 }]);
    let dropped = log(vec![prompt(), EventBody::PromptDropped { prompt: 1 }]);

    assert_eq!(log(vec![prompt()]).prompt_state(1), PromptState::Waiting, "just recorded");
    assert_eq!(waiting.prompt_state(1), PromptState::Waiting, "another prompt's news isn't this one's");
    assert_eq!(taken.prompt_state(1), PromptState::Taken);
    assert_eq!(dropped.prompt_state(1), PromptState::Dropped);
    let sent_again = log(vec![prompt(), EventBody::PromptDropped { prompt: 1 }, EventBody::PromptTaken { prompt: 1 }]);
    assert_eq!(sent_again.prompt_state(1), PromptState::Taken, "another agent took it after all");
}

#[test]
fn only_what_the_log_says_about_the_prompt_settles_it() {
    let ended = EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: Vec::new(),
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
    };
    let exited =
        EventBody::AgentExited { reason: ExitReason::Unexpected, code: None, signal: None, stderr_tail: vec![] };

    for after in [ended.clone(), EventBody::AgentState { state: AgentRunState::Idle }, exited] {
        assert_eq!(log(vec![prompt(), after.clone()]).prompt_state(1), PromptState::Waiting, "{after:?}");
    }
    let late = log(vec![prompt(), ended.clone(), ended, EventBody::PromptDropped { prompt: 1 }]);
    assert_eq!(late.prompt_state(1), PromptState::Dropped, "however many turns end first");
}

#[test]
fn a_prompt_far_behind_what_is_loaded_was_settled_long_ago() {
    let mut log = SessionLog::new(0);
    log.apply_event(event(1, prompt()));
    log.apply_event(event(3, notice()));

    assert_eq!(log.prompt_state(1), PromptState::Taken, "seq 2 isn't loaded");
}

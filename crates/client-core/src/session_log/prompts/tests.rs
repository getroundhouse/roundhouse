use protocol::event::{AgentRunState, EventBody, ExitReason, NoticeLevel, SessionEvent, TurnOutcome};
use protocol::model::SettledAt;
use protocol::model::{Place, Seq, seq_index};
use protocol::prompt::Prompt;

use super::*;
use crate::session_log::Row;

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
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("steer") }
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
        models: Vec::new(),
        tokens: None,
        only_compacted: false,
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

fn taken(prompt: Seq) -> EventBody {
    EventBody::PromptTaken { prompt }
}

fn thought() -> EventBody {
    EventBody::Assistant {
        message_id: None,
        parent_tool_use_id: None,
        model: None,
        blocks: vec![protocol::event::ContentBlock::Thinking { text: "So".into() }],
        first_index: None,
    }
}

fn turn_ended() -> EventBody {
    EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: Vec::new(),
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
        models: Vec::new(),
        tokens: None,
        only_compacted: false,
    }
}

/// The event recorded at `seq`, which is loaded.
fn at(log: &SessionLog, seq: Seq) -> &SessionEvent {
    let Row::Event(event) = log.row(seq_index(seq) - 1) else { panic!("{seq} isn't loaded") };
    event
}

fn seqs(prompts: &[&SessionEvent]) -> Vec<Seq> {
    prompts.iter().map(|prompt| prompt.seq).collect()
}

/// Sent while the agent thinks: the thought is recorded after the prompt, but the agent reads the prompt only after
/// it, so the prompt is drawn under it, at the end until it's taken, and then where it was taken.
#[test]
fn a_prompt_sent_mid_turn_is_drawn_at_the_end_and_then_where_the_agent_took_it() {
    let mut log = log(vec![prompt(), taken(1), prompt()]);
    assert_eq!(log.prompt_at(3), PromptAt::End);
    assert_eq!(seqs(&log.waiting_prompts()), [3]);
    assert_eq!(log.prompt_at(1), PromptAt::Settled(1), "the first was taken straight away");

    log.apply_event(event(4, thought()));
    assert_eq!(log.prompt_at(3), PromptAt::End, "still under what the agent was writing");
    log.apply_event(event(5, taken(3)));

    assert_eq!(log.prompt_at(3), PromptAt::Settled(4));
    assert!(log.waiting_prompts().is_empty());
    let drawn = |seq: Seq| log.prompt_settled_in(at(&log, seq)).map(|prompt| prompt.seq);
    assert_eq!(drawn(5), Some(3), "the event that took it draws it");
    assert_eq!(drawn(2), Some(1));
    assert_eq!(drawn(4), None, "nothing else does");
}

#[test]
fn a_prompt_dropped_and_then_taken_is_drawn_where_it_was_taken() {
    let log = log(vec![prompt(), notice(), EventBody::PromptDropped { prompt: 1 }, notice(), taken(1)]);

    assert_eq!(log.prompt_at(1), PromptAt::Settled(4));
    assert_eq!(log.prompt_settled_in(at(&log, 3)), None, "the drop it was taken after doesn't draw it");
}

/// The hub recorded no prompt as taken before it could, so in an older log every prompt is unsettled. Each stays in
/// its own row rather than gathering at the end, and so does one older than a prompt the agent took, in a chat that
/// went on after the hub could.
#[test]
fn a_prompt_older_than_one_that_was_settled_stays_in_its_own_row() {
    let older = log(vec![prompt(), thought(), turn_ended(), prompt(), thought()]);
    assert_eq!([1, 4].map(|seq| older.prompt_at(seq)), [PromptAt::Own; 2], "none was settled");
    assert!(older.waiting_prompts().is_empty());

    let mut went_on = older;
    went_on.apply_event(event(6, turn_ended()));
    went_on.apply_event(event(7, prompt()));
    went_on.apply_event(event(8, taken(7)));
    went_on.apply_event(event(9, prompt()));

    assert_eq!([1, 4].map(|seq| went_on.prompt_at(seq)), [PromptAt::Own; 2]);
    assert_eq!(went_on.prompt_at(7), PromptAt::Settled(7));
    assert_eq!(seqs(&went_on.waiting_prompts()), [9], "only the one sent since");
}

/// Sent as the agent writes its last reply, which is recorded after it, and taken once that turn has ended, so it waits
/// at the end through the turn's end rather than going above the reply until then.
#[test]
fn a_prompt_sent_during_the_last_reply_waits_at_the_end_until_its_taken() {
    let mut log = log(vec![prompt(), taken(1), prompt(), thought(), turn_ended()]);
    assert_eq!(log.prompt_at(3), PromptAt::End);

    log.apply_event(event(6, taken(3)));

    assert_eq!(log.prompt_at(3), PromptAt::Settled(5), "after the turn's end");
}

/// A newer hub reads by place where the one before it couldn't, so where a prompt was taken arrives again, at its
/// place.
#[test]
fn a_prompt_is_drawn_where_it_was_taken_after_the_log_starts_again_by_place() {
    let mut log = log(vec![prompt(), taken(1)]);
    log.read_by_place();
    log.reaches(2);

    for (seq, body) in [(1, prompt()), (2, taken(1))] {
        log.apply_event(SessionEvent { place: Place::new(seq), ..event(seq, body) });
    }

    assert_eq!(log.prompt_at(1), PromptAt::Settled(1));
    assert_eq!(
        log.prompt_settled_in(at(&log, 2)).map(|prompt| prompt.seq),
        Some(1),
        "and the row that took it draws it"
    );
}

/// The hub's list can say where a prompt was taken before the event that took it arrives, and until then it stays at
/// the end, rather than in a row there isn't yet.
#[test]
fn a_prompt_the_hub_lists_as_taken_stays_at_the_end_until_the_event_that_took_it_arrives() {
    let mut log = log(vec![prompt(), taken(1), prompt()]);
    let line = protocol::model::PromptLine::new(3, "steer", 0).settled_at(SettledAt { seq: 5, place: Place::new(5) });
    log.list_prompts(vec![line]);

    assert_eq!(log.prompt_at(3), PromptAt::End);
    assert_eq!(seqs(&log.waiting_prompts()), [3]);
    log.apply_event(event(4, thought()));
    log.apply_event(event(5, taken(3)));
    assert_eq!(log.prompt_at(3), PromptAt::Settled(4));
    assert!(log.waiting_prompts().is_empty());
}

#[test]
fn a_prompt_far_behind_what_is_loaded_is_drawn_in_its_own_row() {
    let mut log = SessionLog::new(0);
    log.apply_event(event(1, prompt()));
    log.apply_event(event(3, notice()));

    assert_eq!(log.prompt_at(1), PromptAt::Own, "seq 2 isn't loaded");
    assert!(log.waiting_prompts().is_empty());
}

/// A view measures again only the rows it's told changed, so a prompt that moves tells it of each row it leaves and
/// goes to.
#[test]
fn a_prompt_that_moves_changes_the_rows_it_leaves_and_goes_to() {
    let mut log = log(vec![prompt(), taken(1), prompt(), thought()]);
    let _ = log.take_changes();

    log.apply_event(event(5, taken(3)));
    assert_eq!(log.take_changes().rows, Some(2..6), "its own row, where it's drawn now, and the tail it left");
    // By place, with the prompt far enough back not to be loaded, so its own row says nothing.
    let mut dropped = SessionLog::by_place(0);
    let placed = |place, seq, body| SessionEvent { place: Place::new(place), ..event(seq, body) };
    for (place, body) in [(1, notice()), (2, EventBody::PromptDropped { prompt: 5 }), (3, notice())] {
        dropped.apply_event(placed(place, place + 9, body));
    }
    let _ = dropped.take_changes();
    dropped.apply_event(placed(4, 13, taken(5)));
    assert_eq!(dropped.take_changes().rows, Some(1..5), "the row that dropped it, which drew it until then");
}

#[test]
fn a_page_that_brings_a_prompt_or_where_it_was_taken_changes_the_other_row_too() {
    let mut log = SessionLog::new(600);
    let page = |range: std::ops::Range<u64>| range.map(|seq| event(seq, notice())).collect::<Vec<_>>();
    let mut later = page(251..501);
    later[1] = event(252, taken(250));
    log.focus_on(260);
    log.install_page(251, later);
    let _ = log.take_changes();
    let mut earlier = page(1..251);
    earlier[249] = event(250, prompt());

    log.install_page(1, earlier);

    assert_eq!(log.take_changes().rows, Some(0..252), "the page, and the row that took the prompt in it");
    assert_eq!(log.prompt_at(250), PromptAt::Settled(251));
}

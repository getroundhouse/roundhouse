use protocol::event::{CompactedBy, CompactionOutcome, EventBody};
use protocol::model::Place;
use protocol::prompt::Prompt;

use super::*;
use crate::session_log::PromptAt;
use crate::session_log::tests::page;

fn prompt(seq: Seq, text: &str) -> SessionEvent {
    let body = EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new(text) };
    protocol::fixtures::event(seq, body)
}

fn line(seq: Seq) -> PromptLine {
    PromptLine::of(&prompt(seq, "a prompt")).unwrap()
}

fn seqs(log: &SessionLog) -> Vec<Seq> {
    log.prompt_lines().map(PromptLine::seq).collect()
}

#[test]
fn a_prompt_recorded_after_the_list_joins_it_in_order() {
    let mut log = SessionLog::new(Seq::new(600));
    log.list_prompts(vec![line(Seq::new(3))]);

    log.apply_event(prompt(Seq::new(601), "a prompt"));

    assert_eq!(seqs(&log), [Seq::new(3), Seq::new(601)]);
}

#[test]
fn a_prompt_both_listed_and_given_is_there_once_and_changes_nothing_the_second_time() {
    let mut log = SessionLog::new(Seq::new(600));
    log.apply_event(prompt(Seq::new(601), "a prompt"));
    let changes = log.outline_changes();

    log.list_prompts(vec![line(Seq::new(3)), line(Seq::new(601))]);

    assert_eq!(seqs(&log), [Seq::new(3), Seq::new(601)]);
    assert_eq!(log.outline_changes(), changes + 1, "only the prompt it didn't know of is a change");
}

#[test]
fn a_prompt_stays_listed_after_its_page_is_dropped() {
    let mut log = SessionLog::new(Seq::new(3000));
    let mut first = page(1..251);
    first[9] = prompt(Seq::new(10), "far back");
    log.focus_on(0);
    log.install_page(Position::of_seq(Seq::new(1)), first);

    log.focus_on(2999);
    for start in (251..3000).step_by(250) {
        log.install_page(Position::of_seq(Seq::new(start)), page(start..start + 250));
    }

    assert!(matches!(log.row(9), crate::session_log::Row::Unloaded), "its page is gone");
    assert_eq!(seqs(&log), [Seq::new(10)]);
}

#[test]
fn the_list_is_wanted_until_the_hub_answers_and_after_a_failure_only_once_its_retried() {
    let mut log = SessionLog::new(Seq::default());
    assert!(log.wants_prompt_list());

    log.prompt_list_failed();
    assert!(!log.wants_prompt_list(), "a failure isn't asked again at once");
    log.retry_failed();
    assert!(log.wants_prompt_list());

    log.list_prompts(Vec::new());
    log.retry_failed();
    assert!(!log.wants_prompt_list(), "a list once answered is never asked for again");

    let mut older = SessionLog::new(Seq::default());
    older.prompts_unlisted();
    assert!(!older.wants_prompt_list());
}

#[test]
fn a_prompt_is_drawn_at_its_place_among_the_agents_own_events_and_in_the_whole_log_at_its_seq() {
    let placed = |seq, place| line(seq).placed(Place::new(place));
    let mut own = SessionLog::by_place(Place::default());
    own.reaches(Place::new(4));
    let mut whole = SessionLog::new(Seq::new(12));

    // After a subagent's events, so its place isn't its `seq`.
    assert_eq!(own.prompt_row(&placed(Seq::new(10), 4)), 3);
    assert_eq!(whole.prompt_row(&placed(Seq::new(10), 4)), 9);
    assert_eq!(
        own.prompt_row(&line(Seq::new(3))),
        2,
        "a line with no place, from a hub that lists none, goes by its seq"
    );
    assert_eq!(own.prompt_row(&line(Seq::new(10))), 3, "as near as the log goes");
    assert_eq!(
        whole.prompt_row(&line(Seq::default())),
        0,
        "a hub numbers events from 1, so one it numbers 0 goes first"
    );
    for log in [&mut own, &mut whole] {
        log.apply_event(SessionEvent { place: Place::new(5), ..prompt(Seq::new(12), "a prompt") });
        log.apply_event(SessionEvent { place: Place::new(6), ..taken(Seq::new(13), Seq::new(12)) });
    }
    let given = |log: &SessionLog| log.prompt_row(log.prompt_lines().last().unwrap());
    assert_eq!((given(&own), given(&whole)), (5, 12), "and so does one the log is given, where it was taken");
}

fn taken(seq: Seq, prompt: Seq) -> SessionEvent {
    SessionEvent { body: EventBody::PromptTaken { prompt }, ..self::prompt(seq, "") }
}

/// A prompt the agent reads mid-turn is drawn where it took it, after what it did before then, and until then at the
/// chat's end, after what it's still writing. The outline jumps to where it's drawn.
#[test]
fn a_prompt_is_drawn_at_the_end_until_the_agent_takes_it_and_then_where_it_took_it() {
    let mut log = SessionLog::new(Seq::default());
    log.apply_event(prompt(Seq::new(1), "a prompt"));
    log.apply_event(taken(Seq::new(2), Seq::new(1)));
    log.apply_event(prompt(Seq::new(3), "sent mid-turn"));
    let at = |log: &SessionLog| log.prompt_row(log.prompt_lines().last().unwrap());

    assert_eq!(at(&log), 3, "at the end, in the tail, after the log's rows");
    log.apply_event(SessionEvent {
        body: EventBody::AgentState { state: protocol::event::AgentRunState::Running },
        ..prompt(Seq::new(4), "")
    });
    assert_eq!(at(&log), 4, "the end moves on as the log does");
    let changes = log.outline_changes();
    log.apply_event(taken(Seq::new(5), Seq::new(3)));
    assert_eq!(at(&log), 4, "where it was taken");
    assert!(log.outline_changes() > changes, "and the outline hears it moved");
    assert_eq!(log.prompt_row(log.prompt_lines().next().unwrap()), 1, "the first too");
}

/// The hub says where each prompt was settled, so the outline can jump to one whose events aren't loaded, and the
/// prompt's own row draws nothing once it's loaded.
#[test]
fn a_prompt_the_hub_listed_as_settled_is_where_the_hub_says() {
    let mut log = SessionLog::new(Seq::new(600));
    let settled = protocol::model::SettledAt { seq: Seq::new(9), place: Place::new(7), how: None };
    log.list_prompts(vec![line(Seq::new(3)).placed(Place::new(2)).settled_at(settled)]);

    assert_eq!(log.prompt_row(log.prompt_lines().next().unwrap()), 8);
    assert_eq!(log.prompt_at(Seq::new(3)), PromptAt::Settled(8));
    log.apply_event(SessionEvent { body: EventBody::PromptDropped { prompt: Seq::new(3) }, ..prompt(Seq::new(5), "") });
    assert_eq!(
        log.prompt_at(Seq::new(3)),
        PromptAt::Settled(8),
        "a drop before the taking it heard of doesn't move it"
    );
}

#[test]
fn a_log_that_starts_again_by_place_asks_for_the_list_again_for_its_places() {
    let mut log = SessionLog::new(Seq::new(5));
    log.list_prompts(vec![line(Seq::new(3))]);
    let changes = log.outline_changes();

    log.read_by_place();

    assert!(log.wants_prompt_list(), "the hub before listed no places");
    assert!(log.outline_changes() > changes, "and its prompts are in other rows");
    assert_eq!(seqs(&log), [Seq::new(3)], "it keeps them until the list comes");
    log.list_prompts(vec![line(Seq::new(3)).placed(Place::new(2))]);
    log.read_whole(Seq::new(5));
    assert!(!log.wants_prompt_list(), "by seq, the list it has is right");
}

/// A compaction of the agent's own conversation at `seq` and `place`, or one that failed.
fn compaction(seq: Seq, place: u64, compacted: bool) -> SessionEvent {
    let outcome = if compacted {
        let tokens = (Some(467_499), Some(15_694));
        let (by, took_ms, summary) = (CompactedBy::Agent, None, None);
        CompactionOutcome::Compacted { by, before_tokens: tokens.0, after_tokens: tokens.1, took_ms, summary }
    } else {
        CompactionOutcome::Failed { error: None }
    };
    let body = EventBody::AgentCompacted { asked: None, keeping: None, outcome };
    SessionEvent { place: Place::new(place), body, ..prompt(seq, "") }
}

fn compacted(seq: Seq, place: u64) -> CompactionLine {
    CompactionLine::of(&compaction(seq, place, true)).unwrap()
}

fn compaction_seqs(log: &SessionLog) -> Vec<Seq> {
    log.compaction_lines().map(|line| line.seq).collect()
}

#[test]
fn a_compaction_both_listed_and_given_is_there_once_and_changes_nothing_the_second_time() {
    let mut log = SessionLog::new(Seq::new(600));
    log.apply_event(compaction(Seq::new(601), 601, true));
    let changes = log.outline_changes();

    log.list_compactions(vec![compacted(Seq::new(3), 3), compacted(Seq::new(601), 601)]);

    assert_eq!(compaction_seqs(&log), [Seq::new(3), Seq::new(601)]);
    assert_eq!(log.outline_changes(), changes + 1, "only the compaction it didn't know of is a change");
    assert_eq!(seqs(&log), Vec::<Seq>::new(), "and none is a prompt");
}

#[test]
fn a_compaction_is_drawn_at_its_place_among_the_agents_own_events_and_in_the_whole_log_at_its_seq() {
    let mut own = SessionLog::by_place(Place::default());
    own.reaches(Place::new(4));
    let whole = SessionLog::new(Seq::new(12));

    // After a subagent's events, so its place isn't its `seq`.
    assert_eq!(own.compaction_row(&compacted(Seq::new(10), 4)), 3);
    assert_eq!(whole.compaction_row(&compacted(Seq::new(10), 4)), 9);
}

/// A hub can be new enough to list prompts and too old to list compactions.
#[test]
fn the_compactions_list_is_wanted_apart_from_the_prompts() {
    let mut log = SessionLog::new(Seq::default());
    log.list_prompts(Vec::new());
    assert!(log.wants_compaction_list(), "listing prompts doesn't list compactions");

    log.compaction_list_failed();
    assert!(!log.wants_compaction_list(), "a failure isn't asked again at once");
    log.retry_failed();
    assert!(log.wants_compaction_list());
    log.compactions_unlisted();
    log.retry_failed();
    assert!(!log.wants_compaction_list(), "a hub too old to list them isn't asked again");

    let mut log = SessionLog::new(Seq::new(5));
    log.list_compactions(vec![compacted(Seq::new(3), 0)]);
    log.read_by_place();
    assert!(log.wants_compaction_list(), "the hub before listed no places");
}

#[test]
fn a_compaction_that_failed_isnt_listed() {
    let mut log = SessionLog::new(Seq::default());

    log.apply_event(compaction(Seq::new(2), 2, false));

    assert_eq!(log.compaction_lines().len(), 0);
    assert_eq!(log.outline_changes(), 0);
}

#[test]
fn only_prompts_are_listed() {
    let mut log = SessionLog::new(Seq::default());

    log.apply_event(SessionEvent { body: EventBody::PromptTaken { prompt: Seq::new(1) }, ..prompt(Seq::new(2), "") });

    assert_eq!(log.prompt_lines().len(), 0);
    let notice =
        EventBody::Notice { level: protocol::event::NoticeLevel::Info, message: "x".into(), parent_tool_use_id: None };
    let changes = log.outline_changes();
    log.apply_event(SessionEvent { body: notice, ..prompt(Seq::new(3), "") });
    assert_eq!(log.outline_changes(), changes, "nothing else changes the outline");
}

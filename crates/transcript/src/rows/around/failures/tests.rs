use client_core::session_log::SessionLog;
use protocol::capability::turns::OwnTurn;
use protocol::event::{Again, AgentTask, EndedTask, EventBody, SessionEvent, TaskKind, TaskOutcome, TurnOutcome};
use protocol::model::Seq;

use super::super::nearby;
use super::{Failures, SHOWN_TRIES, Started, Tries, redrawn_with};
use crate::fixtures::{calls, event, failed_request, log, prompt, turn_ended};
use crate::folds::{Fold, Folds};

/// A turn the agent started by itself as the command `what` finished.
fn picked_up(what: &str) -> EventBody {
    let task = AgentTask { description: Some(what.into()), ..AgentTask::new(what, TaskKind::Command) };
    EventBody::OwnTurn(OwnTurn::TaskEnded {
        ended: vec![EndedTask { task, outcome: TaskOutcome::Finished, wait: None }],
    })
}

/// The agent's own failed request, saying it repeats the one at `previous`, `times` in a row, as the hub does.
fn again(previous: u64, times: u32) -> EventBody {
    let mut failed = failed_request(None);
    let EventBody::RequestFailed { again, .. } = &mut failed else { unreachable!() };
    *again = Some(Again { previous: Seq::new(previous), times });
    failed
}

/// A turn that ended as a success with nothing said, as the CLI ends one it started by itself while limited.
fn came_to_nothing() -> EventBody {
    protocol::fixtures::turn_ended(TurnOutcome::Success)
}

/// The events, from `seq` 1, each a second after the one before.
fn log_of(events: Vec<EventBody>) -> SessionLog {
    log(events.into_iter().zip(1..).map(|(body, seq)| SessionEvent { ts_ms: at(seq), ..event(Seq::new(seq), body) }))
}

/// When the event at `seq` came, in [`log_of`].
fn at(seq: u64) -> i64 {
    i64::try_from(seq).unwrap() * 1_000
}

/// What the agent hears of its tasks, which starts no try.
fn tasks() -> EventBody {
    EventBody::AgentTasks { tasks: Vec::new(), stopped: Vec::new() }
}

/// How each event of `log` is drawn: `Card` with how many tries it says, `Later` when a later card says it, and `-`.
fn drawn(log: &SessionLog, folds: &Folds) -> Vec<String> {
    let seqs = 1..=u64::try_from(log.row_count()).unwrap();
    let events = seqs.filter_map(|seq| nearby::event_at(log, Seq::new(seq)));
    events
        .map(|event| match Failures::read(log, folds, event.seq, &event.body) {
            Failures::Card(tries) => format!("Card({}, {})", tries.times, tries.each.len()),
            Failures::InALaterCard => "Later".into(),
            Failures::Unrelated => "-".into(),
        })
        .collect()
}

fn card(log: &SessionLog, seq: u64, folds: &Folds) -> Tries {
    let event = nearby::event_at(log, Seq::new(seq)).unwrap();
    let Failures::Card(tries) = Failures::read(log, folds, event.seq, &event.body) else {
        panic!("{seq} isn't a card")
    };
    tries
}

fn started(tries: &Tries) -> Vec<String> {
    let said = |started: &Started| match started {
        Started::Tasks(ended) => ended.iter().map(|ended| ended.task.id.to_string()).collect::<Vec<_>>().join(", "),
        Started::Prompt => "prompt".into(),
        Started::Partway => "partway".into(),
        Started::ByItself => "by itself".into(),
    };
    tries.each.iter().map(|one| said(&one.started)).collect()
}

/// The CLI's sequence while limited: a prompt refused, then a turn the agent started by itself that was refused at
/// once, and another that came to nothing, then one refused again.
#[test]
fn failures_in_a_row_are_one_card_hosted_by_the_newest() {
    let log = log_of(vec![
        prompt("Build it"),   // 1
        failed_request(None), // 2
        turn_ended(),         // 3
        picked_up("build"),   // 4
        again(2, 2),          // 5
        turn_ended(),         // 6
        picked_up("links"),   // 7
        came_to_nothing(),    // 8
        picked_up("tests"),   // 9
        again(5, 3),          // 10
        turn_ended(),         // 11
    ]);

    let drawn = drawn(&log, &Folds::default());

    let expected = ["-", "Later", "-", "Later", "Later", "-", "Later", "Later", "Later", "Card(3, 3)", "-"];
    assert_eq!(drawn, expected, "the card hosts the row, and the turns between that came to nothing draw nothing");
    let tries = card(&log, 10, &Folds::default());
    assert_eq!(started(&tries), ["prompt", "build", "tests"], "the turn that came to nothing isn't a try");
    assert!(tries.says_each());
}

/// What isn't part of a row is drawn as it is: a turn the agent started by itself that did something, and one that came
/// to nothing with no failure after it.
#[test]
fn what_did_something_or_isnt_between_two_failures_is_drawn_as_it_is() {
    let log = log_of(vec![
        prompt("Build it"),   // 1
        failed_request(None), // 2
        turn_ended(),         // 3
        picked_up("build"),   // 4
        calls(&["c1"]),       // 5
        failed_request(None), // 6
        turn_ended(),         // 7
        picked_up("links"),   // 8
        came_to_nothing(),    // 9
    ]);

    let drawn = drawn(&log, &Folds::default());

    assert_eq!(drawn, ["-", "Card(1, 1)", "-", "-", "-", "Card(1, 1)", "-", "-", "-"]);
    let tries = card(&log, 6, &Folds::default());
    assert_eq!(started(&tries), ["partway"]);
    assert!(!tries.says_each(), "the work it was partway through is just above");
}

/// A turn the agent started by itself that was refused before it did anything has its line in the card, even once,
/// while one failure after a prompt has none, since the prompt is just above.
#[test]
fn a_turn_refused_before_it_did_anything_has_its_line_in_the_card() {
    let refused = log_of(vec![picked_up("build"), failed_request(None), turn_ended()]);
    let after_a_prompt = log_of(vec![prompt("Build it"), failed_request(None), turn_ended()]);

    assert_eq!(drawn(&refused, &Folds::default()), ["Later", "Card(1, 1)", "-"]);
    assert!(card(&refused, 2, &Folds::default()).says_each());
    assert!(!card(&after_a_prompt, 2, &Folds::default()).says_each());
}

/// A subagent's failure is in its tab, so it's no part of a row, and draws as it is.
#[test]
fn a_subagents_failure_is_no_part_of_a_card() {
    let log = log_of(vec![prompt("Build it"), failed_request(None), failed_request(Some("toolu-1")), again(2, 2)]);

    assert_eq!(drawn(&log, &Folds::default()), ["-", "Later", "-", "Card(2, 2)"]);
}

/// A prompt refused, then a turn the agent started by itself for each of `times - 1` tasks, each refused the same way.
fn refused(times: u32) -> SessionLog {
    let mut events = vec![prompt("Build it"), failed_request(None), turn_ended()];
    for times in 2..=times {
        let failed = u64::try_from(events.len()).unwrap() - 1;
        events.extend([picked_up(&format!("task {times}")), again(failed, times), turn_ended()]);
    }
    log_of(events)
}

/// The seq of the newest failure in a log from [`refused`].
fn newest(log: &SessionLog) -> u64 {
    u64::try_from(log.row_count()).unwrap() - 1
}

/// Past the newest few, the earlier tries fold, and open in place, and stay open as another try joins, since the fold
/// is the oldest failure's.
#[test]
fn the_earlier_tries_fold_past_the_newest_few() {
    let six = refused(6);
    let mut folds = Folds::default();

    let folded = card(&six, newest(&six), &folds);
    folds.choose(folded.fold.clone(), Fold::Earlier, true);
    let open = card(&six, newest(&six), &folds);
    let seven = refused(7);
    let joined = card(&seven, newest(&seven), &folds);

    assert_eq!((folded.times, folded.earlier()), (6, 2));
    assert_eq!(folded.shown().len(), SHOWN_TRIES);
    assert!(matches!(folded.shown()[0].started, Started::Tasks(_)), "the prompt is folded");
    assert_eq!(open.shown().len(), 6);
    assert!(matches!(open.shown()[0].started, Started::Prompt), "oldest first");
    assert_eq!(joined.shown().len(), 7, "still open");
}

/// The count is the hub's, however much of the row is loaded, and the card says the tries that are.
#[test]
fn the_count_is_the_hubs_and_the_lines_are_what_is_loaded() {
    let log = log([
        event(Seq::new(1), prompt("Build it")),
        event(Seq::new(3), turn_ended()),
        event(Seq::new(4), picked_up("build")),
        event(Seq::new(5), again(2, 5)),
        event(Seq::new(6), turn_ended()),
        event(Seq::new(7), picked_up("links")),
        event(Seq::new(8), again(5, 6)),
    ]);

    let tries = card(&log, 8, &Folds::default());

    assert_eq!((tries.times, started(&tries)), (6, vec!["build".to_string(), "links".to_string()]));
    assert!(tries.says_each());
    assert_eq!(tries.earlier(), 0, "nothing loaded to fold");
}

/// A try whose start isn't loaded, or isn't near enough, has no line, rather than a guess at what started it.
#[test]
fn a_try_whose_start_isnt_loaded_has_no_line() {
    let unloaded = log([event(Seq::new(1), prompt("Build it")), event(Seq::new(3), failed_request(None))]);
    let mut far = vec![prompt("Build it")];
    far.extend((0..70).map(|_| tasks()));
    far.push(failed_request(None));
    let far = log_of(far);

    let tries = card(&unloaded, 3, &Folds::default());
    let far_tries = card(&far, 72, &Folds::default());

    assert!(tries.each.is_empty() && !tries.says_each());
    assert!(far_tries.each.is_empty());
}

/// A try after the turn before ended, with nothing in its own turn to say why, started by itself, when its turn's first
/// event came.
#[test]
fn a_try_after_a_turns_end_was_picked_up_by_itself_as_its_turn_started() {
    let log = log_of(vec![prompt("Build it"), failed_request(None), turn_ended(), tasks(), again(2, 2)]);

    let tries = card(&log, 5, &Folds::default());

    assert_eq!(started(&tries), ["prompt", "by itself"]);
    assert_eq!(tries.each.iter().map(|one| one.at_ms).collect::<Vec<_>>(), [at(1), at(4)]);
}

/// A prompt sent as the agent picked up by itself is what the failure after it was a try of, so the turn's line stays
/// above, and a second turn the agent started by itself is where the try started, so only its line moves into the card.
#[test]
fn only_the_turn_a_try_started_in_moves_into_its_card() {
    let after_a_prompt = log_of(vec![picked_up("build"), prompt("And the links?"), failed_request(None)]);
    let after_another = log_of(vec![picked_up("build"), picked_up("links"), failed_request(None)]);

    assert_eq!(drawn(&after_a_prompt, &Folds::default()), ["-", "-", "Card(1, 1)"]);
    assert!(!card(&after_a_prompt, 3, &Folds::default()).says_each());
    assert_eq!(drawn(&after_another, &Folds::default()), ["-", "Later", "Card(1, 1)"]);
    assert_eq!(started(&card(&after_another, 3, &Folds::default())), ["links"]);
}

/// The hub's word on what a failure repeats comes from outside, so one that says it repeats itself, or a later one,
/// ends the card's tries rather than going round, and one farther back than a failure looks ahead is a card of its own.
#[test]
fn a_card_follows_only_what_goes_back_and_is_near() {
    let itself = log_of(vec![prompt("Build it"), failed_request(None), again(3, 2)]);
    let each_other = log_of(vec![prompt("Build it"), again(3, 2), again(2, 2)]);
    let mut far = vec![prompt("Build it"), failed_request(None)];
    far.extend((0..70).map(|_| tasks()));
    far.extend([picked_up("build"), again(2, 2)]);
    let far = log_of(far);

    assert_eq!(drawn(&itself, &Folds::default()), ["-", "Card(1, 1)", "Card(2, 1)"]);
    assert_eq!(drawn(&each_other, &Folds::default()), ["-", "Later", "Card(2, 2)"]);
    let drawn_far = drawn(&far, &Folds::default());
    assert_eq!((drawn_far[1].as_str(), drawn_far[73].as_str()), ("Card(1, 1)", "Card(2, 1)"));
}

/// A failure arriving draws again the rows before it, as far back as its card reads, and only those.
#[test]
fn a_failure_redraws_the_rows_before_it() {
    let mut events: Vec<EventBody> = (0..100).map(|_| picked_up("build")).collect();
    events.push(failed_request(None));
    events.extend((0..10).map(|_| picked_up("build")));
    let log = log_of(events);

    assert_eq!(redrawn_with(&log, 100..112), 36..112, "back as far as the card reads");
    assert_eq!(redrawn_with(&log, 99..100), 99..100, "nothing about a failure");
    assert_eq!(redrawn_with(&log, 100..101), 36..101, "not after it, which it changes nothing of");
    assert_eq!(redrawn_with(&log, 101..111), 101..111, "after one");
}

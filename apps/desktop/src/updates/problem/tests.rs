use super::*;
use crate::updates::source::SourceError;
use fetching::FetchError;

fn unreachable() -> Result<(), Problem> {
    Err(Problem { kind: Kind::Unreachable, detail: "connection refused".to_string() })
}

fn shown_kind(shown: &Shown) -> Option<Kind> {
    shown.problem.as_ref().map(|problem| problem.kind)
}

#[test]
fn being_out_of_reach_only_shows_once_it_has_lasted_a_day() {
    let dir = tempfile::tempdir().unwrap();
    let (out_of_reach, start) = (OutOfReach::in_dir(dir.path()), SystemTime::now());
    let mut shown = Shown::default();

    shown.record(unreachable(), out_of_reach.note(false, start));
    shown.record(unreachable(), out_of_reach.note(false, start + Duration::from_hours(23)));
    assert_eq!(shown_kind(&shown), None);
    shown.record(unreachable(), out_of_reach.note(false, start + Duration::from_hours(25)));
    assert_eq!(shown_kind(&shown), Some(Kind::Unreachable));
}

/// Someone who quits the app every evening would otherwise never hear that it can't reach the update server.
#[test]
fn being_out_of_reach_counts_across_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let start = SystemTime::now();
    OutOfReach::in_dir(dir.path()).note(false, start);

    let reopened = OutOfReach::in_dir(dir.path());
    let mut shown = Shown::default();
    shown.record(unreachable(), reopened.note(false, start + Duration::from_hours(25)));

    assert_eq!(shown_kind(&shown), Some(Kind::Unreachable));
}

#[test]
fn a_look_that_reaches_the_server_clears_the_problem_and_starts_the_day_again() {
    let dir = tempfile::tempdir().unwrap();
    let (out_of_reach, start) = (OutOfReach::in_dir(dir.path()), SystemTime::now());
    let mut shown = Shown::default();
    out_of_reach.note(false, start);
    shown.record(unreachable(), out_of_reach.note(false, start + Duration::from_hours(25)));

    shown.record(Ok(()), out_of_reach.note(true, start + Duration::from_hours(26)));
    out_of_reach.note(false, start + Duration::from_hours(27));
    shown.record(unreachable(), out_of_reach.note(false, start + Duration::from_hours(40)));

    assert_eq!(shown_kind(&shown), None);
}

#[test]
fn a_refused_update_shows_at_once() {
    let mut shown = Shown::default();
    let refused = Problem { kind: Kind::Refused(Refusal::Unsigned), detail: String::new() };

    shown.record(Err(refused.clone()), Duration::ZERO);
    shown.record(unreachable(), Duration::from_hours(1));

    assert_eq!(shown.problem, Some(refused));
}

/// Someone who checks while a problem shows is waiting on the check, so it's what shows until the check ends.
#[test]
fn a_check_under_way_is_said_over_a_problem_and_then_its_answer() {
    let problem = Problem { kind: Kind::Unreachable, detail: "connection refused".to_string() };
    let mut shown = Shown { problem: Some(problem.clone()), ..Shown::default() };
    assert_eq!(shown.said(), Some(Said::Problem(&problem)));

    shown.checking = true;
    assert_eq!(shown.said(), Some(Said::Checking));

    shown.checking = false;
    shown.answer = Some(Answer::UpToDate);
    assert_eq!(shown.said(), Some(Said::Answer(Answer::UpToDate)), "the answer is said over a problem");

    shown.problem = None;
    assert_eq!(shown.said(), Some(Said::Answer(Answer::UpToDate)));
}

#[test]
fn nothing_is_said_with_nothing_to_say() {
    assert_eq!(Shown::default().said(), None);
}

/// The kind of problem a fetch that failed so shows as.
fn shown_for(error: SourceError) -> Kind {
    problem(Step::Download, &velopack::Error::from(error)).kind
}

/// A server that isn't there, `FetchError::Request`, is `a_server_that_isnt_there_is_out_of_reach` in the updater's
/// tests.
#[test]
fn each_way_a_fetch_fails_shows_as_its_problem() {
    let (url, io) = (|| "https://example.com/package".to_string(), || std::io::Error::other("gone"));
    let fetch = |error| shown_for(SourceError::Fetch(error));

    assert_eq!(fetch(FetchError::Status { url: url(), status: 404 }), Kind::Unreachable);
    assert_eq!(fetch(FetchError::Read { url: url(), source: io() }), Kind::Unreachable);
    assert_eq!(fetch(FetchError::CutShort { url: url(), expected_bytes: 2, got_bytes: 1 }), Kind::Unreachable);
    assert_eq!(fetch(FetchError::TooLong { url: url(), limit_bytes: 1 }), Kind::Refused(Refusal::TooLong));
    assert_eq!(fetch(FetchError::Save { path: "/package".into(), source: io() }), Kind::Failed(Step::Download));
    let unreadable = serde_json::from_str::<u8>("not a feed").unwrap_err();
    assert_eq!(shown_for(SourceError::Unreadable(unreadable)), Kind::Failed(Step::Download));
}

use std::sync::Arc;

use protocol::model::{RecentWork, Rev, Subagent, SubagentStatus};

use super::Subagents;
use crate::records::Snapshot;

fn subagent(session: &str, call: &str, started_at_ms: i64, rev: u64) -> Subagent {
    Subagent {
        session_id: session.into(),
        call: call.into(),
        kind: None,
        description: None,
        prompt: None,
        in_background: false,
        model: None,
        status: SubagentStatus::Working,
        started_at_ms,
        ended_at_ms: None,
        calls: 0,
        latest_call: None,
        recent: RecentWork::default(),
        report: None,
        rev: Rev::new(rev),
    }
}

/// The calls that started a session's subagents, as a view would read them.
fn calls(subagents: &Subagents, session: &str) -> Vec<String> {
    let shown = Snapshot::new(Arc::default(), subagents.listed());
    shown.subagents(&session.into()).iter().map(|subagent| subagent.call.to_string()).collect()
}

/// The subagent `call` started, as a view would read it.
fn shown(subagents: &Subagents, session: &str, call: &str) -> Option<Subagent> {
    let shown = Snapshot::new(Arc::default(), subagents.listed());
    shown.subagent(&session.into(), &call.into()).map(|subagent| Subagent::clone(subagent))
}

#[test]
fn only_the_subagents_of_a_session_a_view_asked_for_are_kept() {
    let mut subagents = Subagents::default();

    assert!(!subagents.apply(subagent("s", "a", 1, 5)), "nobody asked");
    assert!(subagents.want("s".into()));
    assert!(!subagents.want("s".into()), "asked already");
    assert!(subagents.apply(subagent("s", "a", 1, 6)));
    assert!(!subagents.apply(subagent("t", "b", 1, 7)), "another session");

    assert_eq!(calls(&subagents, "s"), ["a"]);
    assert!(calls(&subagents, "t").is_empty());
}

#[test]
fn a_list_and_the_feed_keep_whichever_copy_is_newer_whatever_order_they_come_in() {
    let mut subagents = Subagents::default();
    subagents.want("s".into());
    let mut finished = subagent("s", "a", 1, 9);
    finished.status = SubagentStatus::Finished;

    assert!(subagents.apply(finished.clone()));
    assert!(subagents.apply_list(&"s".into(), vec![subagent("s", "a", 1, 4), subagent("s", "b", 0, 5)]), "b is new");
    assert!(!subagents.apply(subagent("s", "a", 1, 8)), "older than the one held");

    assert_eq!(shown(&subagents, "s", "a"), Some(finished));
    assert_eq!(calls(&subagents, "s"), ["b", "a"], "in the order they started");
}

#[test]
fn a_copy_with_no_revision_replaces_the_one_held_as_any_records_does() {
    let mut subagents = Subagents::default();
    subagents.want("s".into());
    subagents.apply(subagent("s", "a", 1, 5));
    let mut finished = subagent("s", "a", 1, 0);
    finished.status = SubagentStatus::Finished;

    assert!(subagents.apply(finished.clone()));
    assert!(!subagents.apply(finished.clone()), "the same copy again changes nothing");

    assert_eq!(shown(&subagents, "s", "a"), Some(finished));
}

#[test]
fn a_wanted_session_is_listed_once_a_connection_and_again_on_the_next() {
    let mut subagents = Subagents::default();
    subagents.want("s".into());
    assert_eq!(subagents.to_list(), ["s".into()]);

    subagents.apply_list(&"s".into(), Vec::new());
    assert!(subagents.to_list().is_empty());
    subagents.relist();
    assert_eq!(subagents.to_list(), ["s".into()]);

    subagents.skip("s".into());
    assert!(subagents.to_list().is_empty(), "the hub refused it, so it waits for the next connection");

    subagents.relist();
    subagents.unkept();
    assert!(subagents.to_list().is_empty() && !subagents.kept(), "a hub that keeps none has none to list");
    subagents.relist();
    assert!(subagents.kept() && subagents.to_list() == ["s".into()], "the next hub may keep them");
}

#[test]
fn a_session_no_view_shows_any_more_is_let_go_of() {
    let mut subagents = Subagents::default();
    subagents.want("s".into());
    subagents.apply(subagent("s", "a", 1, 5));

    subagents.forget(&"s".into());

    assert!(calls(&subagents, "s").is_empty() && subagents.to_list().is_empty());
    assert!(!subagents.apply(subagent("s", "a", 1, 6)), "nobody wants it now");
}

#[test]
fn a_session_let_go_of_while_it_was_being_listed_and_wanted_again_is_listed_again() {
    let mut subagents = Subagents::default();
    subagents.want("s".into());
    assert_eq!(subagents.to_list(), ["s".into()]);

    subagents.forget(&"s".into());
    assert!(!subagents.apply_list(&"s".into(), vec![subagent("s", "a", 1, 5)]), "nobody wants it as the list comes");
    subagents.want("s".into());

    assert_eq!(subagents.to_list(), ["s".into()], "so the list it took in is no longer good");
    assert!(calls(&subagents, "s").is_empty());
}

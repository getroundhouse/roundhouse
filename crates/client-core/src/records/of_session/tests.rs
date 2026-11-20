use std::sync::Arc;

use protocol::model::{Ending, Proposal, ReplyHeard, Rev, SettledBy, Subagent, SubagentStatus};

use super::{BySession, subagent_named};
use crate::records::Snapshot;

type Subagents = BySession<Subagent>;

fn subagent(session: &str, call: &str, started_at_ms: i64, rev: u64) -> Subagent {
    Subagent { session_id: session.into(), started_at_ms, rev: Rev::new(rev), ..protocol::fixtures::subagent(call) }
}

/// The calls that started a session's subagents, as a view would read them.
fn calls(subagents: &Subagents, session: &str) -> Vec<String> {
    let shown = Snapshot::new(Arc::default(), subagents.listed(), Arc::default(), Arc::default(), Arc::default());
    shown.subagents(&session.into()).iter().map(|subagent| subagent.call.to_string()).collect()
}

/// The subagent `call` started, as a view would read it.
fn shown(subagents: &Subagents, session: &str, call: &str) -> Option<Subagent> {
    let shown = Snapshot::new(Arc::default(), subagents.listed(), Arc::default(), Arc::default(), Arc::default());
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

/// A subagent is found by its ID, or else by the call that started it, as a tab opened before its record came names it,
/// and by its ID first, since that's what a subagent's events name.
#[test]
fn a_subagent_is_named_by_its_id_or_else_by_its_call() {
    let named = |id: &str, call: &str| Arc::new(Subagent { id: Some(id.into()), ..subagent("s", call, 1, 1) });
    let subagents = [named("task_1", "toolu_1"), named("toolu_2", "toolu_3"), named("task_2", "toolu_2")];
    let found = |name: &str| subagent_named(&subagents, &name.into()).map(|subagent| subagent.id().to_string());

    assert_eq!(found("task_1").as_deref(), Some("task_1"));
    assert_eq!(found("toolu_1").as_deref(), Some("task_1"), "by its call");
    assert_eq!(found("toolu_2").as_deref(), Some("toolu_2"), "by its ID first");
    assert_eq!(found("toolu_9"), None);
}

fn proposal(session: &str, id: &str, created_at_ms: i64, rev: u64) -> Proposal {
    Proposal { session_id: session.into(), created_at_ms, rev: Rev::new(rev), ..protocol::fixtures::proposal(id) }
}

/// Proposals are kept as subagents are, and read in the order they were made.
#[test]
fn a_sessions_proposals_are_kept_as_its_subagents_are_in_the_order_they_were_made() {
    let mut proposals = BySession::<Proposal>::default();
    proposals.want("s".into());
    let mut retitled = proposal("s", "p", 2, 9);
    retitled.current.proposed.title = "Search, again".into();

    assert!(proposals.apply(retitled.clone()));
    assert!(proposals.apply_list(&"s".into(), vec![proposal("s", "p", 2, 4), proposal("s", "q", 1, 5)]), "q is new");
    assert!(!proposals.apply(proposal("t", "r", 1, 6)), "another session");

    let shown = Snapshot::new(Arc::default(), Arc::default(), proposals.listed(), Arc::default(), Arc::default());
    let ids: Vec<_> = shown.proposals(&"s".into()).iter().map(|proposal| proposal.id.to_string()).collect();
    assert_eq!(ids, ["q", "p"]);
    assert_eq!(shown.proposal(&"s".into(), &"p".into()).map(|p| Proposal::clone(p)), Some(retitled));
}

/// How a session's replies were heard is kept as its proposals are, and found by reply.
#[test]
fn how_a_sessions_replies_were_heard_is_kept_as_its_proposals_are_and_found_by_reply() {
    let heard = |session: &str, message: &str, ending, rev| ReplyHeard {
        session_id: session.into(),
        message: message.into(),
        ending,
        settled_by: SettledBy::Device,
        rev: Rev::new(rev),
    };
    let mut kept = BySession::<ReplyHeard>::default();
    kept.want("s".into());

    assert!(kept.apply_list(&"s".into(), vec![heard("s", "m1", Ending::NotReadOut, 4)]));
    assert!(kept.apply(heard("s", "m2", Ending::ToTheEnd, 5)));
    assert!(!kept.apply(heard("t", "m3", Ending::ToTheEnd, 6)), "another session");

    let shown = Snapshot::new(Arc::default(), Arc::default(), Arc::default(), kept.listed(), Arc::default());
    let found = |message: &str| shown.reply_heard(&"s".into(), &message.into()).map(|heard| heard.ending);
    assert_eq!((found("m1"), found("m2"), found("m3")), (Some(Ending::NotReadOut), Some(Ending::ToTheEnd), None));
}

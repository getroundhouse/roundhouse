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
    let shown = Snapshot::new(
        Arc::default(),
        Arc::default(),
        subagents.listed(),
        Arc::default(),
        Arc::default(),
        Arc::default(),
    );
    shown.subagents(&session.into()).iter().map(|subagent| subagent.call.to_string()).collect()
}

/// The subagent `call` started, as a view would read it.
fn shown(subagents: &Subagents, session: &str, call: &str) -> Option<Subagent> {
    let shown = Snapshot::new(
        Arc::default(),
        Arc::default(),
        subagents.listed(),
        Arc::default(),
        Arc::default(),
        Arc::default(),
    );
    shown.subagent(&session.into(), &call.into()).map(|subagent| Subagent::clone(subagent))
}

#[test]
fn only_the_subagents_of_a_session_followed_are_kept() {
    let mut subagents = Subagents::default();

    assert!(!subagents.apply(subagent("s", "a", 1, 5)), "nobody follows it");
    subagents.want("s".into());
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
    let list = vec![subagent("s", "a", 1, 4), subagent("s", "b", 0, 5)];
    assert!(subagents.apply_list(&"s".into(), list, Rev::new(5)), "b is new");
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

/// A session's list says which of its subagents there are, so one held that it lacks is gone, unless the copy held is
/// newer than the list, as for any list.
#[test]
fn a_sessions_list_drops_what_it_lacks_unless_whats_held_is_newer() {
    let mut subagents = Subagents::default();
    subagents.want("s".into());
    subagents.apply(subagent("s", "a", 1, 5));
    subagents.apply(subagent("s", "c", 3, 8));

    assert!(subagents.apply_list(&"s".into(), vec![subagent("s", "b", 2, 6)], Rev::new(7)));

    assert_eq!(calls(&subagents, "s"), ["b", "c"], "a is gone, and c came after the list");
}

/// A session is followed once for each subscription that follows it, so its subagents are kept until the last lets go,
/// and nothing is kept of it after.
#[test]
fn a_session_is_let_go_of_once_nothing_follows_it() {
    let mut subagents = Subagents::default();
    subagents.want("s".into());
    subagents.want("s".into());
    subagents.apply(subagent("s", "a", 1, 5));

    let first = subagents.forget(&"s".into());
    let while_one_follows = calls(&subagents, "s");
    let last = subagents.forget(&"s".into());

    assert_eq!((first, while_one_follows, last), (false, vec!["a".to_string()], true));
    assert!(calls(&subagents, "s").is_empty());
    assert!(!subagents.apply(subagent("s", "a", 1, 6)), "nobody follows it now");
    assert!(!subagents.apply_list(&"s".into(), vec![subagent("s", "a", 1, 7)], Rev::new(7)), "nor its list");
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
    let list = vec![proposal("s", "p", 2, 4), proposal("s", "q", 1, 5)];
    assert!(proposals.apply_list(&"s".into(), list, Rev::new(5)), "q is new");
    assert!(!proposals.apply(proposal("t", "r", 1, 6)), "another session");

    let shown = Snapshot::new(
        Arc::default(),
        Arc::default(),
        Arc::default(),
        proposals.listed(),
        Arc::default(),
        Arc::default(),
    );
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

    assert!(kept.apply_list(&"s".into(), vec![heard("s", "m1", Ending::NotReadOut, 4)], Rev::new(4)));
    assert!(kept.apply(heard("s", "m2", Ending::ToTheEnd, 5)));
    assert!(!kept.apply(heard("t", "m3", Ending::ToTheEnd, 6)), "another session");

    let shown =
        Snapshot::new(Arc::default(), Arc::default(), Arc::default(), Arc::default(), kept.listed(), Arc::default());
    let found = |message: &str| shown.reply_heard(&"s".into(), &message.into()).map(|heard| heard.ending);
    assert_eq!((found("m1"), found("m2"), found("m3")), (Some(Ending::NotReadOut), Some(Ending::ToTheEnd), None));
}

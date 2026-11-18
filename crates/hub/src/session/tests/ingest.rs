//! What the session takes in from its agent's worker: what it tells the feed, what it has heard already, and what the
//! hub answers for the person.

use super::super::ingest::{heard_before, the_hub_allows};
use super::super::reports::says_something_new;
use super::super::*;

fn context(used_tokens: u64) -> ContextUsage {
    ContextUsage { used_tokens, max_tokens: 200_000, model_tokens: None, max_set_by: None, parts: Vec::new() }
}

#[test]
fn only_the_run_going_says_how_full_the_context_is_and_only_a_change_counts() {
    let (going, replaced) = (RunId::new(2), RunId::new(1));

    assert!(says_something_new(Some(going), None, going, &context(1)), "the first report");
    assert!(says_something_new(Some(going), Some(&context(1)), going, &context(2)));
    assert!(
        !says_something_new(Some(going), Some(&context(1)), going, &context(1)),
        "the feed isn't told what it knows"
    );
    assert!(!says_something_new(Some(going), Some(&context(1)), replaced, &context(2)), "a replaced run's");
    assert!(!says_something_new(None, Some(&context(1)), going, &context(2)), "a run that has exited");
}

/// As a worker sends a hub that connects to it what it sent the last hub, which may have recorded some of it.
#[test]
fn what_a_worker_sends_again_is_taken_in_once() {
    let (first, next) = (RunId::new(3), RunId::new(4));
    let mut recorded = Some((first, 5));
    let heard: Vec<_> = [4, 5, 6, 7].into_iter().map(|seq| heard_before(&mut recorded, first, seq)).collect();
    assert_eq!(heard, [true, true, false, false], "only what came after the last recorded");
    assert_eq!(recorded, Some((first, 7)));
    assert!(heard_before(&mut recorded, first, 7), "and each only once");
    assert!(!heard_before(&mut recorded, next, 2), "another run's is never the same, whatever its number");
    assert_eq!(recorded, Some((next, 2)));
    let mut never = None;
    assert!(!heard_before(&mut never, first, 1), "nothing is before the first");
}

#[test]
fn the_hub_allows_only_a_tools_use_asked_about_with_full_access_and_only_when_set_to() {
    use protocol::capability::approvals::ApprovalMode;
    use protocol::capability::approvals::{Asks, Request};
    use protocol::fixtures::{asking, plan, to_run};
    use protocol::fixtures::{full_access, plan_mode};

    let asked = |asks: Asks, always: bool| Request { asks, suggested: Vec::new(), always, why: None };
    for always in [true, false] {
        assert!(the_hub_allows(full_access(), &asked(to_run("ls"), always), true), "a call, asked always or not");
    }
    assert!(!the_hub_allows(full_access(), &asked(to_run("ls"), true), false), "only when set to");
    for mode in [plan_mode(), ApprovalMode::UNKNOWN] {
        assert!(!the_hub_allows(mode, &asked(to_run("ls"), true), true), "{mode:?} asks the person");
    }
    for asks in [asking(Vec::new()), plan("Do it"), Asks::Unknown] {
        assert!(!the_hub_allows(full_access(), &asked(asks.clone(), true), true), "{asks:?} is the person's to answer");
    }
}

/// A report is dropped unless its run claims what it needs, but for one a worker of protocol 4 sent as a frame of its
/// own, from before a run had to claim what a report needs.
#[tokio::test]
async fn a_report_its_run_doesnt_claim_is_dropped_but_from_a_worker_of_protocol_4() {
    use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker};
    use protocol::capability::{Capabilities, Claimed, Report};
    use protocol::model::RunCapabilities;

    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let run = RunId::new(2);
    let live = carried_on(&hub, &link, run);
    let held = |claims: Capabilities| live.state.lock().capabilities = Some(RunCapabilities::new(run, claims));
    let reported = async |used_tokens, held_to_claims| {
        let report = Report::Context { context: context(used_tokens) };
        live.send_item(Item::Report { run, report, seq: None, held_to_claims }).await;
        flushed(&live).await;
        live.state.lock().context.as_ref().map(|context| context.used_tokens)
    };

    held(Capabilities::default());
    assert_eq!(reported(1, true).await, None, "a run that doesn't claim it");
    assert_eq!(reported(2, false).await, Some(2), "from a worker of protocol 4");
    let mut usage = Capabilities::default();
    usage.context.usage = Some(Claimed {});
    held(usage);
    assert_eq!(reported(3, true).await, Some(3), "a run that claims it");
}

/// The newest to-do list the run reports is kept on the session's record, which a hub that starts again reads, and the
/// same list again leaves the record as it was.
#[tokio::test]
async fn the_agents_to_do_list_is_kept_on_the_sessions_record_and_only_a_change_moves_it() {
    use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker};
    use protocol::capability::output::{Todo, TodoList, TodoStatus};
    use protocol::capability::{Capabilities, Claimed, Report};
    use protocol::model::RunCapabilities;

    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let run = RunId::new(2);
    let live = carried_on(&hub, &link, run);
    let mut claims = Capabilities::default();
    claims.output.todos = Some(Claimed {});
    live.state.lock().capabilities = Some(RunCapabilities::new(run, claims));
    let todos = |status| {
        let todo = Todo { id: None, text: "Write it".into(), status, doing: None, priority: None, after: Vec::new() };
        TodoList { items: vec![todo], note: Some("One step".into()) }
    };
    let recorded = async |said: TodoList| {
        live.send_item(Item::Report { run, report: Report::Todos(said), seq: None, held_to_claims: true }).await;
        flushed(&live).await;
        let id = live.id.clone();
        hub.blocking(move |store| store.get_session(&id)).await.unwrap().session
    };

    let first = recorded(todos(TodoStatus::InProgress)).await;
    assert_eq!(first.todos, Some(todos(TodoStatus::InProgress)));
    assert_eq!(recorded(todos(TodoStatus::InProgress)).await.rev, first.rev, "the same list again");
    let done = recorded(todos(TodoStatus::Done)).await;
    assert_eq!(done.todos, Some(todos(TodoStatus::Done)));
    assert_ne!(done.rev, first.rev, "a change moves the record");
}

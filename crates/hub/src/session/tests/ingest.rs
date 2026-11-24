//! What the session takes in from its agent's worker: what it tells the feed, what it has heard already, and what the
//! hub answers for the person.

use super::super::ingest::{heard_before, the_hub_allows};
use super::super::reports::speaks_for_the_session;
use super::super::*;

fn context(used_tokens: u64) -> ContextUsage {
    ContextUsage { used_tokens, max_tokens: 200_000, model_tokens: None, max_set_by: None, parts: Vec::new() }
}

#[test]
fn only_the_run_going_speaks_for_the_session() {
    let (going, replaced) = (RunId::new(2), RunId::new(1));

    assert!(speaks_for_the_session(Some(going), going));
    assert!(!speaks_for_the_session(Some(going), replaced), "a replaced run's");
    assert!(!speaks_for_the_session(None, going), "a run that has exited");
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

/// What `observed` says, if it's known.
fn known<T>(observed: protocol::model::Observed<T>) -> Option<T> {
    match observed {
        protocol::model::Observed::Current { value } | protocol::model::Observed::AsOf { value, .. } => Some(value),
        protocol::model::Observed::NotKnown | protocol::model::Observed::Unknown => None,
    }
}

/// A report is dropped unless its run claims what it needs.
#[tokio::test]
async fn a_report_its_run_doesnt_claim_is_dropped() {
    use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker};
    use protocol::capability::{Capabilities, Claimed, Report};
    use protocol::model::{Observation, RunCapabilities};

    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let run = RunId::new(2);
    let live = carried_on(&hub, &link, run);
    let held = |claims: Capabilities| live.state.lock().capabilities = Some(RunCapabilities::new(run, claims));
    let reported = async |used_tokens| {
        let report = Report::Context { context: context(used_tokens) };
        live.send_item(Item::Report { run, report, seq: None }).await;
        flushed(&live).await;
        let listed = hub.session_records(live.id.clone()).await.unwrap().observations;
        listed.into_iter().find_map(|observation| {
            let Observation::SessionContext { context, .. } = observation else { return None };
            known(context).map(|context| context.used_tokens)
        })
    };

    held(Capabilities::default());
    assert_eq!(reported(1).await, None, "a run that doesn't claim it");
    let mut usage = Capabilities::default();
    usage.context.usage = Some(Claimed {});
    held(usage);
    assert_eq!(reported(3).await, Some(3), "a run that claims it");
}

/// The newest to-do list the run reports is kept as the session's observation, which fills its record for a client of
/// version 4, and the same list again moves neither, while a change moves the record's revision too.
#[tokio::test]
async fn the_agents_to_do_list_is_kept_as_an_observation_and_only_a_change_moves_the_sessions_record() {
    use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker};
    use protocol::capability::output::{Todo, TodoList, TodoStatus};
    use protocol::capability::{Capabilities, Claimed, Report};
    use protocol::model::{Observation, RunCapabilities};

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
        live.send_item(Item::Report { run, report: Report::Todos(said), seq: None }).await;
        flushed(&live).await;
        let id = live.id.clone();
        let mut session = hub.blocking(move |store| store.get_session(&id)).await.unwrap().session;
        hub.fill_old_forms(&mut session);
        let listed = hub.session_records(live.id.clone()).await.unwrap().observations;
        let kept = listed.into_iter().find_map(|observation| {
            let Observation::SessionTodos { todos, .. } = observation else { return None };
            known(todos)
        });
        (kept, session.todos, session.rev)
    };

    let (kept, filled, first) = recorded(todos(TodoStatus::InProgress)).await;
    assert_eq!((kept, filled), (Some(todos(TodoStatus::InProgress)), Some(todos(TodoStatus::InProgress))));
    assert_eq!(recorded(todos(TodoStatus::InProgress)).await.2, first, "the same list again");
    let (kept, filled, done) = recorded(todos(TodoStatus::Done)).await;
    assert_eq!((kept, filled), (Some(todos(TodoStatus::Done)), Some(todos(TodoStatus::Done))));
    assert!(done > first, "a change moves the record, for a client of version 4");
}

/// The mode the current run says it's in changes the session's record in the transaction that records the event, and
/// nothing else writes it, so no hub that stops between two writes can leave the log and the record saying different
/// modes. What a replaced run says, and a mode the record has already, change nothing, and the live session takes the
/// change too, so the next launch starts in it.
#[tokio::test]
async fn the_mode_the_agent_says_is_written_only_with_the_event_that_says_it() {
    use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker, store_of};
    use crate::store::columns::OneValue;
    use protocol::fixtures::plan_mode;

    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let run = RunId::new(2);
    let live = carried_on(&hub, &link, run);
    let store = store_of(&hub);
    // How far the log had gone as of each write of the session's mode, which only recording an event moves.
    let watch = "CREATE TEMP TABLE mode_writes (seq INTEGER);
        CREATE TEMP TRIGGER mode_written AFTER UPDATE OF mode ON sessions BEGIN
            INSERT INTO mode_writes VALUES (NEW.last_seq);
        END";
    store.holding_writes().execute_batch(watch).unwrap();
    let says = async |run, mode| {
        let body = EventBody::AgentMode { mode };
        live.send_item(Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None }).await;
        flushed(&live).await;
    };
    let before = store.get_session(&live.id).unwrap().session.mode;
    let mut feed = hub.subscribe_feed();

    says(RunId::new(1), plan_mode()).await;
    says(run, plan_mode()).await;
    says(run, plan_mode()).await;

    assert_ne!(before, plan_mode());
    let log = store.read_events(&live.id, Seq::FIRST, 100, u64::MAX, false).unwrap();
    let said: Vec<i64> = log
        .iter()
        .filter(|event| matches!(event.body, EventBody::AgentMode { .. }) && event.run == Some(run))
        .map(|event| event.seq.get().cast_signed())
        .collect();
    let written: Vec<i64> = {
        let writes = store.holding_writes();
        let mut statement = writes.prepare("SELECT seq FROM mode_writes ORDER BY rowid").unwrap();
        statement.query_map([], OneValue::read).unwrap().collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(said.len(), 2, "each is in the log");
    assert_eq!(written, [said[0]], "once, as the first from the current run was recorded");
    assert_eq!(store.get_session(&live.id).unwrap().session.mode, plan_mode());
    assert_eq!(live.state.lock().settings.mode, plan_mode(), "the next launch starts in it");
    let told = std::iter::from_fn(|| feed.try_recv().ok()).filter_map(|notice| {
        let protocol::client::HubNotice::Session(told) = &*notice else { return None };
        Some(told.mode)
    });
    assert_eq!(told.collect::<Vec<_>>(), [plan_mode()], "every client heard of it once");
}

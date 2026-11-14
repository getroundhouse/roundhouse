//! What a starting hub records of a session an earlier hub left at work, whose agent has gone, before it serves.

use protocol::event::{AgentTask, TaskKind};
use protocol::model::{SessionStatus, Wait, WaitUntil};
use protocol::prompt::Prompt;
use serde_json::json;

use super::*;
use crate::store::tests::store_with_session;
use crate::store::{Seen, SessionChange, StatusChange, TurnEdge};

#[test]
fn a_session_whose_agent_has_gone_is_interrupted_as_it_is_with_the_hub_there() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = SessionId::from("s");
    let prompt = EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("x") };
    let started = StatusChange { status: SessionStatus::Running, turn: TurnEdge::Starts, background_tasks: None };
    let prompted = store.append(&id, None, &prompt, None, None, Some(started.into())).unwrap();
    let asking = json!({"type": "approval_requested", "approval_id": "a", "tool_name": "Bash", "input": {}});
    let task = AgentTask { id: "t".into(), description: None, kind: TaskKind::Command, call: None };
    let waiting = StatusChange {
        status: SessionStatus::WaitingApproval,
        turn: TurnEdge::Neither,
        background_tasks: Some(vec![task]),
    };
    let run = Some(RunId::new(1));
    store.append(&id, run, &serde_json::from_value(asking).unwrap(), None, None, Some(waiting.into())).unwrap();

    interrupt(&store, &id, vec![prompted.event.seq]).unwrap();

    let session = store.get_session(&id).unwrap().session;
    assert_eq!(session.status, SessionStatus::Interrupted);
    assert!(session.background_tasks.is_empty(), "the agent took its work with it");
    assert!(session.queue_held, "it's for the person to say whether to go on");
    assert_eq!(session.turn_started_at_ms, Some(prompted.event.ts_ms));
    assert_eq!(session.reply_seq, Seq::default(), "a turn the agent was cut off from doesn't end");
    assert!(store.open_approvals(&id).unwrap().is_empty());
    assert!(store.untaken_prompts().unwrap().is_empty());
    let after = store.read_events(&id, Seq::new(prompted.event.seq.get() + 2), 10, u64::MAX, false).unwrap();
    let kinds: Vec<_> = after.iter().map(|event| event.body.kind()).collect();
    assert_eq!(kinds, ["prompt_dropped", "approval_resolved", "agent_exited"]);
}

/// Its agent went with the earlier hub's workers, but the wait is the hub's, so a chat whose only work was the wait is
/// still at work in the background, and one at work on anything else is cut off, as is one whose listed wait the hub
/// doesn't have open.
#[test]
fn a_chat_that_only_waited_is_left_waiting_and_one_at_work_is_interrupted() {
    let task = |id: &str, kind| AgentTask { id: id.into(), description: None, kind, call: None };
    let (waiting, command) = (task("t", TaskKind::Wait), task("u", TaskKind::Command));
    let in_the_background = |tasks: &[AgentTask]| StatusChange {
        background_tasks: Some(tasks.to_vec()),
        ..SessionStatus::Background.into()
    };
    let left = |waited: bool, status: StatusChange| {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        if waited {
            store.start_wait(&wait(), Seen::default()).unwrap();
        }
        let listed =
            EventBody::AgentTasks { tasks: status.background_tasks.clone().unwrap_or_default(), stopped: vec![] };
        let change =
            SessionChange { status: Some(status), queue_held: None, recorded: None, initialized: None, claims: None };
        store.append(&"s".into(), None, &listed, None, None, Some(change)).unwrap();
        interrupt(&store, &"s".into(), Vec::new()).unwrap();
        let session = store.get_session(&"s".into()).unwrap().session;
        (session.status, session.queue_held, session.background_tasks.len())
    };

    let only_the_wait = left(true, in_the_background(std::slice::from_ref(&waiting)));
    let and_a_command = left(true, in_the_background(&[waiting.clone(), command.clone()]));
    let a_command = left(true, in_the_background(std::slice::from_ref(&command)));
    let no_wait = left(false, in_the_background(std::slice::from_ref(&waiting)));
    let mid_turn = left(true, SessionStatus::Running.into());

    assert_eq!(only_the_wait, (SessionStatus::Background, false, 1), "the wait is still listed");
    let cut_off = (SessionStatus::Interrupted, true, 0);
    assert_eq!([and_a_command, a_command, no_wait, mid_turn], [cut_off; 4]);
}

fn wait() -> Wait {
    Wait {
        id: "a".into(),
        session_id: "s".into(),
        workspace_id: "w".into(),
        number: 7,
        call: None,
        until: WaitUntil::Done,
        watch: None,
        started_at_ms: 1,
        ended: None,
    }
}

/// Its agent went with the machine, say, and left no exit, so nothing else would take its claims off.
#[tokio::test]
async fn a_starting_hub_takes_the_claims_off_every_chat_whose_agent_it_didnt_find() {
    use protocol::capability::{Capabilities, Claimed};
    use protocol::model::RunCapabilities;

    let dir = tempfile::tempdir().unwrap();
    let hub = crate::hub::tests::hub_in(dir.path());
    let id = SessionId::from("s");
    let mut claims = Capabilities::default();
    claims.prompts.steer = Some(Claimed {});
    let capabilities = Some(Box::new(claims.clone()));
    let started = EventBody::AgentStarted { pid: 1, resumed: false, build: None, previous_build: None, capabilities };
    let claimed = RunCapabilities { run: RunId::new(1), claims: Box::new(claims) };
    let change = SessionChange { claims: Some(claimed), ..SessionChange::default() };
    hub.store.append(&id, Some(RunId::new(1)), &started, None, None, Some(change)).unwrap();
    assert!(hub.store.get_session(&id).unwrap().session.agent_capabilities.is_some());

    hub.carry_on(LeftAtWork::read(&hub.store).unwrap()).await;

    assert!(hub.store.get_session(&id).unwrap().session.agent_capabilities.is_none());
}

/// Its worker, of this protocol, says what the run it kept claims, which the record holds from then on, whatever it held
/// before. One of protocol 3 doesn't, and the record keeps what the run said as it initialized.
#[tokio::test]
async fn a_starting_hub_holds_a_run_it_carries_on_with_to_what_its_worker_says_it_claims() {
    use crate::hub::tests::{claimed, hub_in, stand_in_worker};
    use protocol::model::RunCapabilities;

    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let (id, run) = (SessionId::from("s"), RunId::new(3));
    let said = RunCapabilities { run, claims: Box::new(claimed()) };
    let on_record = || hub.store.get_session(&id).unwrap().session.agent_capabilities;

    let initialized = RunCapabilities { run, claims: Box::default() };
    hub.store.set_claims(&id, &initialized).unwrap();

    hub.carry_on_session(&id, link.id, run, Vec::new(), None).await;
    assert_eq!(on_record(), Some(initialized), "a worker of protocol 3 says nothing, and the record keeps them");
    hub.carry_on_session(&id, link.id, run, Vec::new(), Some(said.clone())).await;
    assert_eq!(on_record(), Some(said));
}

/// Its agent is running, which carrying on with it found, so its claims stay, as every other chat's go.
#[tokio::test]
async fn a_starting_hub_leaves_the_claims_of_a_chat_it_carried_on_with() {
    use crate::hub::tests::{carried_on, claimed, hub_in, stand_in_worker};
    use protocol::model::RunCapabilities;

    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let (id, run) = (SessionId::from("s"), RunId::new(1));
    let held = RunCapabilities::new(run, claimed());
    hub.store.set_claims(&id, &held).unwrap();
    carried_on(&hub, &link, run);

    hub.carry_on(LeftAtWork::read(&hub.store).unwrap()).await;

    assert_eq!(hub.store.get_session(&id).unwrap().session.agent_capabilities, Some(held));
}

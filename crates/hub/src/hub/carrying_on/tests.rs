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
    assert_eq!(session.reply_seq, 0, "a turn the agent was cut off from doesn't end");
    assert!(store.open_approvals(&id).unwrap().is_empty());
    assert!(store.untaken_prompts().unwrap().is_empty());
    let after = store.read_events(&id, prompted.event.seq + 2, 10, u64::MAX, false).unwrap();
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
        let change = SessionChange { status: Some(status), queue_held: None, recorded: None };
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
        started_at_ms: 1,
        ended: None,
    }
}

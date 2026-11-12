use protocol::event::{AgentTask, TaskKind};
use protocol::model::{Wait, WaitUntil};

use super::*;
use crate::store::Seen;
use crate::store::tests::store_with_session;

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

use protocol::event::{AgentTask, TaskKind};
use protocol::model::{Wait, WaitUntil};

use super::*;
use crate::store::Seen;
use crate::store::tests::store_with_session;

/// Its agent went with the earlier hub's workers, but the wait is the hub's, so a chat whose only work was the wait is
/// still at work in the background, and one at work on anything else is cut off.
#[test]
fn a_chat_that_only_waited_is_left_waiting_and_one_at_work_is_interrupted() {
    let task = AgentTask { id: "t".into(), description: None, kind: TaskKind::Command, call: None };
    let in_the_background = |tasks: &[AgentTask]| StatusChange {
        background_tasks: Some(tasks.to_vec()),
        ..SessionStatus::Background.into()
    };
    let two = [task.clone(), AgentTask { id: "u".into(), ..task.clone() }];
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

    let only_the_wait = left(true, in_the_background(std::slice::from_ref(&task)));
    let and_another = left(true, in_the_background(&two));
    let no_wait = left(false, in_the_background(std::slice::from_ref(&task)));
    let mid_turn = left(true, SessionStatus::Running.into());

    assert_eq!(only_the_wait, (SessionStatus::Background, false, 1), "the wait is still listed");
    let cut_off = (SessionStatus::Interrupted, true, 0);
    assert_eq!((and_another, no_wait, mid_turn), (cut_off, cut_off, cut_off));
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

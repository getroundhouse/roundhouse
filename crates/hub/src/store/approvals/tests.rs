use protocol::event::EventBody;
use protocol::model::{ApprovalId, OpenApproval};

use super::*;
use crate::store::tests::store_with_session;
use crate::store::{cancelled_approval, older, schema};

fn asked(id: &str) -> EventBody {
    EventBody::ApprovalRequested {
        approval_id: ApprovalId::from(id),
        tool_name: "Bash".into(),
        tool_use_id: None,
        input: serde_json::json!({}),
        description: None,
        suggestions: serde_json::Value::Null,
        asking: None,
    }
}

fn answered(id: &str) -> EventBody {
    cancelled_approval(ApprovalId::from(id))
}

fn append(store: &Store, body: &EventBody) -> Option<Vec<OpenApproval>> {
    store.append(&"s".into(), None, body, None, None, None).unwrap().approvals
}

fn open(at: &[(u64, &str)]) -> Vec<OpenApproval> {
    at.iter().map(|&(seq, id)| OpenApproval { seq, approval_id: ApprovalId::from(id) }).collect()
}

/// In the same commit as the event, so a client reading the record after it can't see an older answer.
#[test]
fn an_approval_is_open_on_the_record_from_its_request_until_its_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    assert_eq!(append(&store, &asked("a")), Some(open(&[(1, "a")])));
    assert_eq!(append(&store, &EventBody::Unknown), None);
    assert_eq!(append(&store, &asked("b")), Some(open(&[(1, "a"), (3, "b")])));
    assert_eq!(append(&store, &asked("b")), None, "asked again, it's open already");
    assert_eq!(store.get_session(&"s".into()).unwrap().session.open_approvals, Some(open(&[(1, "a"), (3, "b")])));

    let before = store.get_session(&"s".into()).unwrap().session.rev;
    assert_eq!(append(&store, &answered("a")), Some(open(&[(3, "b")])));
    assert!(store.get_session(&"s".into()).unwrap().session.rev > before, "a copy from before is older");
    assert_eq!(append(&store, &answered("a")), None, "resolved already");
    assert_eq!(store.open_approvals(&"s".into()).unwrap(), [ApprovalId::from("b")]);
    assert_eq!(append(&store, &answered("b")), Some(Vec::new()));
    assert_eq!(store.get_session(&"s".into()).unwrap().session.open_approvals, Some(Vec::new()));
}

/// A session from before the record said is filled in from its log, as the hub would have kept it.
#[test]
fn a_session_from_before_has_the_approvals_its_log_leaves_open() {
    let dir = tempfile::tempdir().unwrap();
    let log = [asked("a"), answered("a"), asked("b"), asked("c"), asked("c"), asked("a")];
    // `recorded_run` is from a migration just before, so this is the database of a build from before this one.
    let session = |id: &str, last_seq: usize| {
        format!(
            "INSERT INTO sessions (id, workspace_id, agent, model, permission_mode, effort, agent_session_id, status, \
             last_seq, created_at_ms, updated_at_ms, recorded_run) \
             VALUES ('{id}', 'w', 'claude', 'm', 'bypassPermissions', 'high', '{id}', 'idle', {last_seq}, 0, 0, NULL);"
        )
    };
    let events: Vec<String> = (1..)
        .zip(&log)
        .map(|(seq, body)| {
            format!(
                "INSERT INTO events (session_id, seq, ts_ms, kind, place, body) VALUES ('s', {seq}, 0, '{}', {seq}, '{}');",
                body.kind(),
                serde_json::to_string(body).unwrap()
            )
        })
        .collect();
    let rows = format!("{}{}{}", session("s", log.len()), events.concat(), session("t", 0));

    let store = older::store_from_before(dir.path(), schema::BEFORE_WAITING_ON, &rows);

    let opened = |id: &str| store.get_session(&id.into()).unwrap().session.open_approvals;
    assert_eq!(opened("s"), Some(open(&[(3, "b"), (4, "c"), (6, "a")])), "each once, from its first open request");
    assert_eq!(opened("t"), Some(Vec::new()));
    assert_eq!(append(&store, &answered("c")), Some(open(&[(3, "b"), (6, "a")])));
}

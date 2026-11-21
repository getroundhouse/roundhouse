use protocol::event::EventBody;
use protocol::fixtures;
use protocol::model::{ApprovalId, OpenApproval};

use super::*;
use crate::store::cancelled_approval;
use crate::store::tests::store_with_session;

fn asked(id: &str) -> EventBody {
    fixtures::approval_request(id, fixtures::to_run("ls")).into()
}

fn answered(id: &str) -> EventBody {
    cancelled_approval(ApprovalId::from(id))
}

fn append(store: &Store, body: &EventBody) -> Option<Vec<OpenApproval>> {
    store.append(&"s".into(), None, body, None, None, None).unwrap().approvals
}

fn open(at: &[(u64, &str)]) -> Vec<OpenApproval> {
    at.iter().map(|&(seq, id)| OpenApproval { seq: Seq::new(seq), approval_id: ApprovalId::from(id) }).collect()
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

use protocol::model::{Rev, SessionId, WorkspaceId};
use protocol::worker::ToolsToken;

use crate::store::tests::store_with_session;

#[test]
fn a_workspace_keeps_the_port_its_tools_were_last_served_on_without_changing_its_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = WorkspaceId::from("w");
    let rev = store.get_workspace(&id).unwrap().rev;

    assert_eq!(store.tools_port(&id).unwrap(), None);
    store.set_tools_port(&id, 50_001).unwrap();
    store.set_tools_port(&id, 50_002).unwrap();

    assert_eq!(store.tools_port(&id).unwrap(), Some(50_002));
    assert_eq!(store.get_workspace(&id).unwrap().rev, rev);
    assert_ne!(rev, Rev::default());
}

#[test]
fn a_session_keeps_the_first_token_made_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = SessionId::from("s");

    let first = store.tools_token(&id, &ToolsToken::new("first")).unwrap();
    let again = store.tools_token(&id, &ToolsToken::new("second")).unwrap();

    assert_eq!(first, ToolsToken::new("first"));
    assert_eq!(again, first);
}

#[test]
fn a_session_without_a_record_gets_no_token() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    assert!(store.tools_token(&SessionId::from("nobody"), &ToolsToken::new("t")).is_err());
}

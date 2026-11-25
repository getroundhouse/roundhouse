use protocol::model::{ToolServer, ToolServerAuth};

use super::*;
use crate::store::lock;
use crate::store::tests::open_in;

/// Each is a change to the record a client follows, so each has a revision of its own.
#[test]
fn a_tool_servers_record_has_a_revision_of_its_own_as_its_added_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let ToolServer { id, name, url, auth, .. } = protocol::fixtures::tool_server("t");
    let adding = AddingToolServer { id: &id, name: &name, url: &url, auth: &auth, secret: None };

    let added = store.add_tool_server(&adding).unwrap();
    let listed = store.list_records().unwrap().tool_servers;
    let removed = store.remove_tool_server(&id).unwrap().unwrap();

    assert_eq!((&added.name, &added.url, &added.auth), (&name, &url, &auth));
    assert!(added.rev < removed, "{} {removed}", added.rev);
    assert_eq!(listed, [added]);
    assert_eq!(store.tool_server(&id).unwrap(), None);
    assert_eq!(store.remove_tool_server(&id).unwrap(), None);
}

#[test]
fn a_way_of_knowing_the_person_from_a_newer_build_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let newer = r#"{"kind":"passkey","made_up":1}"#;
    let insert = "INSERT INTO tool_servers (id, name, url, auth) VALUES ('t', 'Docs', 'https://example.com', ?)";
    lock(&store.writer).execute(insert, [newer]).unwrap();

    let (server, _) = store.tool_server(&"t".into()).unwrap().unwrap();

    assert_eq!(server.auth, ToolServerAuth::Unknown);
}

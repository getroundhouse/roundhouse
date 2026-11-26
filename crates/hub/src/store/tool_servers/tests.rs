use protocol::model::{ToolServer, ToolServerAuth, ToolServerName, ToolServerUrl};

use super::*;
use crate::login_files::LoginFile;
use crate::store::tests::open_in;
use crate::store::{OneValue, Store, lock};

/// The server `t`, called `Sentry`.
fn renamed(store: &Store) -> ToolServer {
    let name = ToolServerName::stored("Sentry".into());
    let editing = EditingToolServer { name: Some(&name), ..EditingToolServer::default() };
    store.edit_tool_server(&"t".into(), &editing).unwrap().unwrap()
}

/// Each is a change to the record a client follows, so each has a revision of its own.
#[test]
fn a_tool_servers_record_has_a_revision_of_its_own_as_its_added_edited_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let ToolServer { id, name, url, auth, .. } = protocol::fixtures::tool_server("t");
    let adding = AddingToolServer { id: &id, name: &name, url: &url, auth: &auth, secret: None };

    let added = store.add_tool_server(&adding).unwrap();
    let renamed = renamed(&store);
    let listed = store.list_records().unwrap().tool_servers;
    let removed = store.remove_tool_server(&id).unwrap().unwrap();

    assert_eq!((&added.name, &added.url, &added.auth), (&name, &url, &auth));
    assert!(added.rev < renamed.rev && renamed.rev < removed, "{} {} {removed}", added.rev, renamed.rev);
    assert_eq!(listed, [renamed]);
    assert_eq!(store.tool_server(&id).unwrap(), None);
    assert_eq!(store.remove_tool_server(&id).unwrap(), None);
}

#[test]
fn a_way_of_knowing_the_person_from_a_newer_build_is_unknown_and_kept_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let newer = r#"{"kind":"passkey","made_up":1}"#;
    let insert = "INSERT INTO tool_servers (id, name, url, auth) VALUES ('t', 'Docs', 'https://example.com', ?)";
    lock(&store.writer).execute(insert, [newer]).unwrap();

    let renamed = renamed(&store);

    assert_eq!(renamed.auth, ToolServerAuth::Unknown);
    let kept: String = lock(&store.reader).query_row("SELECT auth FROM tool_servers", [], OneValue::read).unwrap();
    assert_eq!(kept, newer);
}

#[test]
fn an_edit_is_written_whole_and_leaves_the_file_its_secret_is_in_unless_that_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let ToolServer { id, name, url, auth, .. } = protocol::fixtures::tool_server("t");
    let (first, second) = (LoginFile::new(), LoginFile::new());
    store
        .add_tool_server(&AddingToolServer { id: &id, name: &name, url: &url, auth: &auth, secret: Some(&first) })
        .unwrap();
    let kept = || store.tool_server(&id).unwrap().unwrap();

    renamed(&store);
    let of_the_name = kept();
    let neither = ToolServerAuth::Neither;
    let known_by =
        |secret| EditingToolServer { known_by: Some(KnownBy { auth: &neither, secret }), ..Default::default() };
    let elsewhere = ToolServerUrl::stored("https://example.com".into());
    store.edit_tool_server(&id, &EditingToolServer { url: Some(&elsewhere), ..known_by(Some(&second)) }).unwrap();
    let of_all_else = kept();
    store.edit_tool_server(&id, &known_by(None)).unwrap();

    assert_eq!((of_the_name.0.name.as_str(), of_the_name.0.auth, of_the_name.1), ("Sentry", auth, Some(first)));
    let (server, file) = of_all_else;
    assert_eq!(
        (server.name.as_str(), server.url, server.auth, file),
        ("Sentry", elsewhere, neither.clone(), Some(second.clone()))
    );
    assert_eq!(kept().1, None);
    assert_eq!(store.edit_tool_server(&"gone".into(), &known_by(None)).unwrap(), None);
}

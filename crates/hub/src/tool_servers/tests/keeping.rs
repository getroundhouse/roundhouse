//! Adding a server with a key or with neither, and removing one: what's kept, where, and what's refused.

use protocol::client::ErrorCode;
use protocol::client::tool_servers::ToolServerAdd;
use protocol::model::{ToolServerAuth, ToolServerAuthGiven, ToolServerKey};

use super::super::ToolServerChange;
use super::super::secret::{ToolServerSecret, read};
use super::support::{KEY_FILE, files_in, holding, refused, servers_in, with_key, with_neither};
use crate::login_files::{LoginSecret as _, Stored};

#[test]
fn a_key_is_kept_in_a_private_file_of_its_own_alone_and_a_server_with_neither_keeps_none() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());

    let key = servers.add(with_key("k", " Linear (Northwind)\n", " Bearer made-up\r\n")).unwrap();
    let neither = servers.add(with_neither("n", "Docs")).unwrap();

    assert!(key.changed && neither.changed);
    assert_eq!(key.server.name.as_str(), "Linear (Northwind)");
    assert_eq!(key.server.url, neither.server.url, "a URL that's there already isn't refused");
    assert!(matches!(&key.server.auth, ToolServerAuth::Key { header } if header.as_str() == "Authorization"));
    assert_eq!(neither.server.auth, ToolServerAuth::Neither);
    assert_eq!(store.list_records().unwrap().tool_servers, [neither.server, key.server]);
    assert_eq!(files_in(dir.path()), [KEY_FILE]);
    let entry = std::fs::read_dir(dir.path().join("tool-servers")).unwrap().next().unwrap().unwrap();
    assert_eq!(entry.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(holding(dir.path(), "made-up"), ["tool-servers"], "nothing else has it, the database least of all");
}

#[test]
fn a_key_in_authorization_with_no_scheme_is_kept_as_a_bearer_token_and_any_other_as_it_is() {
    for (header, key, kept) in [
        ("Authorization", "made-up", "Bearer made-up"),
        ("AUTHORIZATION", " made-up\n", "Bearer made-up"),
        ("Authorization", "Bearer made-up", "Bearer made-up"),
        ("Authorization", "Basic made-up", "Basic made-up"),
        ("X-API-Key", "made-up", "made-up"),
    ] {
        let given = ToolServerAuthGiven::Key { header: header.into(), key: ToolServerKey::new(key) };
        let (_, secret) = read(given).unwrap();
        assert_eq!(secret, Some(ToolServerSecret::Key(ToolServerKey::new(kept))), "{key:?} in {header}");
    }
}

#[test]
fn adding_one_of_an_id_again_is_the_server_as_it_is_whatever_else_is_asked() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let added = servers.add(with_key("k", "Linear", "Bearer made-up")).unwrap().server;
    let before = store.list_records().unwrap();

    let again = servers.add(with_key("k", "Linear", "Bearer made-up")).unwrap();
    let otherwise = servers.add(ToolServerAdd { url: "not a URL".into(), ..with_key("k", "", "another") }).unwrap();

    assert_eq!(again, ToolServerChange { server: added, changed: false });
    assert_eq!(otherwise, again);
    assert_eq!(store.list_records().unwrap(), before);
    assert_eq!(files_in(dir.path()), [KEY_FILE]);
}

#[test]
fn a_name_agents_would_call_another_server_by_is_refused_saying_which() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    servers.add(with_neither("a", "Linear (Northwind)")).unwrap();
    let before = store.list_records().unwrap();

    let same = servers.add(with_key("c", "Linear (Northwind)", "Bearer made-up")).unwrap_err();
    let same_for_agents = servers.add(with_key("c", "linear northwind", "Bearer made-up")).unwrap_err();

    assert_eq!([&same, &same_for_agents].map(|e| e.code), [ErrorCode::Conflict; 2]);
    assert_eq!(same.message, "another MCP server is called Linear (Northwind)");
    let both = "agents would call linear northwind and the MCP server Linear (Northwind) the same, Linear_Northwind";
    assert_eq!(same_for_agents.message, both);
    assert_eq!(store.list_records().unwrap(), before);
    assert_eq!(files_in(dir.path()), Vec::<String>::new());
}

#[test]
fn removing_forgets_the_server_and_its_key_and_again_there_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let added = servers.add(with_key("k", "Linear", "Bearer made-up")).unwrap().server;
    let other = servers.add(with_key("o", "Docs", "another")).unwrap().server;

    let removed = servers.remove(&"k".into()).unwrap();

    assert!(removed.rev > other.rev);
    assert_eq!(removed.server, added);
    assert_eq!(store.list_records().unwrap().tool_servers, [other]);
    assert_eq!(files_in(dir.path()), [r#"{"format":"key","key":"Bearer another"}"#]);
    assert_eq!(refused(servers.remove(&"k".into())), ErrorCode::NotFound);
    assert!(servers.add(with_neither("k", "Linear")).unwrap().changed, "its name is free, and so is its ID");
}

#[test]
fn what_cant_be_a_servers_is_refused_and_keeps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let before = store.list_records().unwrap();
    let in_header = |header: &str| ToolServerAuthGiven::Key { header: header.into(), key: ToolServerKey::new("k") };

    let asked = [
        with_key("k", " ", "key"),
        ToolServerAdd { url: "https://ada:made-up@mcp.example.com".into(), ..with_key("k", "Docs", "key") },
        ToolServerAdd { auth: in_header("Host"), ..with_neither("k", "Docs") },
        ToolServerAdd { auth: ToolServerAuthGiven::Unknown, ..with_neither("k", "Docs") },
        with_key("k", "Docs", " \n"),
        with_key("k", "Docs", "two\nlines"),
        with_key("k", "Docs", "two\rlines"),
        with_key("k", "Docs", "clé"),
        with_key("k", "Docs", &"k".repeat(4097)),
    ];
    for asked in asked {
        let said = format!("{asked:?}");
        assert_eq!(refused(servers.add(asked)), ErrorCode::InvalidRequest, "{said}");
    }

    assert_eq!(store.list_records().unwrap(), before);
    assert!(!dir.path().join("tool-servers").exists());
}

/// The form is spelled out, since a build that reads it differently would take every key for what isn't one.
#[test]
fn a_keys_file_is_a_json_object_of_its_form_and_what_isnt_one_holds_none() {
    let key = ToolServerSecret::Key(ToolServerKey::new("Bearer made-up"));
    let read = |text: &str| ToolServerSecret::from_bytes(text.as_bytes().to_vec());

    assert_eq!(String::from_utf8(key.to_bytes().unwrap()).unwrap(), KEY_FILE);
    assert_eq!(read(KEY_FILE), Stored::Secret(key.clone()));
    assert_eq!(read(r#"{"format":"key","key":"Bearer made-up","added":1}"#), Stored::Secret(key));
    assert_eq!(read(r#"{"format":"sign_in_of_a_newer_build","made_up":1}"#), Stored::Newer);
    for nothing in ["Bearer made-up", r#"{"format":"key"}"#, r#"{"format":"key","ke"#] {
        assert_eq!(read(nothing), Stored::Nothing, "{nothing}");
    }
}

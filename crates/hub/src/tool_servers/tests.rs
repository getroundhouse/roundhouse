//! What's kept of a tool server, where, and what's refused.

use std::path::Path;

use protocol::client::ErrorCode;
use protocol::client::tool_servers::ToolServerAdd;
use protocol::model::{ToolServerAuth, ToolServerAuthGiven, ToolServerKey};

use super::secret::ToolServerSecret;
use super::*;
use crate::login_files::{LoginFile, LoginSecret as _, Stored};
use crate::store::Store;
use crate::store::tests::open_in;
use std::sync::Arc;

/// The tool servers of a hub whose data directory is `dir`, and its store.
fn servers_in(dir: &Path) -> (ToolServers, Arc<Store>) {
    let store = Arc::new(open_in(dir));
    (ToolServers::new(store.clone(), dir), store)
}

/// Adding `name` as `id`, with the key `key` in `Authorization`.
fn with_key(id: &str, name: &str, key: &str) -> ToolServerAdd {
    let auth = ToolServerAuthGiven::Key { header: "Authorization".into(), key: ToolServerKey::new(key) };
    ToolServerAdd { auth, ..with_neither(id, name) }
}

fn with_neither(id: &str, name: &str) -> ToolServerAdd {
    let url = "https://mcp.example.com/mcp".into();
    ToolServerAdd { id: id.into(), name: name.into(), url, auth: ToolServerAuthGiven::Neither }
}

fn refused<T: std::fmt::Debug>(done: Result<T, protocol::client::ApiError>) -> ErrorCode {
    done.unwrap_err().code
}

/// What's in every file in the tool servers' directory.
fn files_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir.join("tool-servers")) else { return Vec::new() };
    let mut files: Vec<String> = entries.map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap()).collect();
    files.sort();
    files
}

/// Every file in `dir` that holds `text`, such as the database, its log and whatever else the hub keeps.
fn holding(dir: &Path, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(holding(&path, text));
        } else if std::fs::read(&path).unwrap().windows(text.len()).any(|bytes| bytes == text.as_bytes()) {
            found.push(path.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    found
}

const KEY_FILE: &str = r#"{"format":"key","key":"Bearer made-up"}"#;

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
    assert_eq!(files_in(dir.path()), [r#"{"format":"key","key":"another"}"#]);
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

/// A server with neither, one whose key is there, and one whose key is in a form only a newer build reads are kept,
/// with their files. One whose key is gone, as removing it leaves it when the hub stops between, goes, and so does a
/// file no server names, as adding one leaves it.
#[test]
fn tidying_removes_a_server_whose_key_is_gone_and_a_file_no_server_names_and_keeps_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let kept = servers.add(with_key("k", "Linear", "Bearer made-up")).unwrap().server;
    let neither = servers.add(with_neither("n", "Docs")).unwrap().server;
    let newer = servers.add(with_key("newer", "Sentry", "a-key")).unwrap().server;
    let gone = servers.add(with_key("gone", "Figma", "a-key")).unwrap().server;
    let path = |id| {
        let file = store.tool_server(id).unwrap().unwrap().1.unwrap();
        dir.path().join("tool-servers").join(file.as_str())
    };
    let newer_form = r#"{"format":"made_up_by_a_newer_build","made_up":1}"#;
    std::fs::write(path(&newer.id), newer_form).unwrap();
    std::fs::remove_file(path(&gone.id)).unwrap();
    std::fs::write(dir.path().join("tool-servers").join(LoginFile::new().as_str()), KEY_FILE).unwrap();

    servers.tidy().unwrap();

    assert_eq!(store.list_records().unwrap().tool_servers, [neither, kept, newer]);
    assert_eq!(files_in(dir.path()), [KEY_FILE, newer_form]);
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

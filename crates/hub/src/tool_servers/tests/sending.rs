//! Which servers workers are sent, and what the log says of the rest.

use logging::testing::Logs;
use protocol::model::{ToolServerAuth, ToolServerName, ToolServerUrl};
use protocol::worker::{ServerSignIn, TokenNumber};
use tool_sign_in::{OauthError, Renewal};

use super::super::ToolServers;
use super::sign_ins::{GIVEN_AT_MS, again, granted, signed_in};
use super::support::{servers_in, with_key, with_neither};
use crate::store::AddingToolServer;

/// The servers workers are sent, by ID, and what each request to each carries.
fn sent(servers: &ToolServers) -> Vec<(String, Option<String>)> {
    let set = servers.sent().borrow().set();
    let carried = |server: protocol::worker::ProxiedServer| {
        let header = server.header.map(|header| format!("{}: {}", header.name.as_str(), header.value.as_str()));
        (server.id.to_string(), header)
    };
    set.servers.into_iter().map(carried).collect()
}

/// Before any link can follow them, and with no change made.
#[test]
fn a_hub_that_starts_again_has_workers_sent_the_servers_it_had_with_their_keys() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    servers.add(with_key("k", "Linear", "Bearer made-up")).unwrap();
    servers.add(with_neither("n", "Docs")).unwrap();
    drop(servers);

    let (again, _) = servers_in(dir.path());

    let expected = [("n".to_string(), None), ("k".to_string(), Some("Authorization: Bearer made-up".to_string()))];
    assert_eq!(sent(&again), expected);
}

/// Through a change to the sign-in, and to what the hub has of it, as much as to the server. Each token has a higher
/// number than the last.
#[test]
fn a_signed_in_server_is_sent_with_each_access_token_and_which_it_is_and_with_none_once_it_needs_signing_in_to() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    let due = |now_ms| servers.check_sign_ins(now_ms).unwrap().due.remove(0);
    let expected = |token: &str, number| {
        let header = format!("Authorization: Bearer {token}");
        let works = ServerSignIn::Works { token: number };
        (vec![("t".to_string(), Some(header))], Some(works))
    };
    let sent = || (sent(&servers), servers.sent_server(&"t".into()).unwrap().sign_in);
    let second = TokenNumber::FIRST.next();

    signed_in(&servers, "t", "Linear");
    assert_eq!(sent(), expected("made-up-t", TokenNumber::FIRST));

    let ends_at_ms = GIVEN_AT_MS + 3_600_000;
    servers.renewed(&due(ends_at_ms), Renewal::Renewed(granted("made-up-renewed")), ends_at_ms).unwrap();
    assert_eq!(sent(), expected("made-up-renewed", second));

    again(&servers, "t").unwrap();
    assert_eq!(sent(), expected("made-up-again", second.next()));

    let refused = Renewal::SignInAgain(OauthError::new("invalid_grant"));
    servers.renewed(&due(ends_at_ms), refused, ends_at_ms).unwrap();
    assert_eq!(sent(), (vec![("t".to_string(), None)], Some(ServerSignIn::NeedsSignIn)));
}

/// Each is kept, and the rest are sent, and the log says which and why, with no key, and no name or URL.
#[test]
fn a_server_whose_key_cant_be_read_or_whose_way_of_knowing_the_person_isnt_known_is_left_out() {
    let logs = Logs::capture();
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    for id in ["kept", "newer", "gone", "unreadable"] {
        servers.add(with_key(id, id, &format!("Bearer secret-{id}"))).unwrap();
    }
    let path = |id: &str| {
        let file = store.tool_server(&id.into()).unwrap().unwrap().1.unwrap();
        dir.path().join("tool-servers").join(file.as_str())
    };
    std::fs::write(path("newer"), r#"{"format":"made_up_by_a_newer_build","key":"secret-newer"}"#).unwrap();
    std::fs::remove_file(path("gone")).unwrap();
    std::fs::remove_file(path("unreadable")).unwrap();
    std::fs::create_dir(path("unreadable")).unwrap();
    let unknown = AddingToolServer {
        id: &"unknown".into(),
        name: &ToolServerName::typed("Figma").unwrap(),
        url: &ToolServerUrl::typed("https://figma.example.com/mcp").unwrap(),
        auth: &ToolServerAuth::Unknown,
        secret: None,
    };
    store.add_tool_server(&unknown).unwrap();

    servers.add(with_neither("neither", "neither")).unwrap();

    let kept = ("kept".to_string(), Some("Authorization: Bearer secret-kept".to_string()));
    assert_eq!(sent(&servers), [kept, ("neither".to_string(), None)]);
    let left_out = |event: &serde_json::Value| {
        let field = |name: &str| event["fields"][name].as_str().unwrap_or_default().to_string();
        (event["event"] == "tool_servers.left_out").then(|| format!("{}: {}", field("id"), field("why")))
    };
    let mut said: Vec<String> = logs.events().iter().filter_map(left_out).collect();
    said.sort();
    let expected = [
        "gone: its key or its sign-in is gone",
        "newer: only a newer build can read its key or its sign-in",
        "unknown: only a newer build knows how it knows the person",
        "unreadable: its file can't be read: is a directory",
    ];
    assert_eq!(said, expected);
    for private in ["secret", "Figma", "example.com"] {
        assert!(!logs.text().contains(private), "{}", logs.text());
    }
}

#[test]
fn a_change_that_changes_nothing_workers_are_sent_has_them_sent_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    servers.add(with_key("k", "Linear", "Bearer made-up")).unwrap();
    let mut following = servers.sent();
    following.borrow_and_update();

    servers.add(with_key("k", "Linear", "Bearer made-up")).unwrap();
    servers.add(with_key("k2", "linear", "another")).unwrap_err();
    servers.tidy().unwrap();

    assert!(!following.has_changed().unwrap());
    servers.remove(&"k".into()).unwrap();
    assert!(following.has_changed().unwrap());
    assert_eq!(sent(&servers), []);
}

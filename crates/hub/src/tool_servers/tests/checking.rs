//! What a check of a server would ask, and where: what workers would be sent, refused as the change itself is, and
//! nothing changed by it.

use std::path::Path;

use protocol::client::tool_server_checks::{ToolServerCheck, ToolServerToCheck};
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ToolServer, ToolServerAuthEdit, ToolServerAuthGiven, ToolServerKey};

use super::super::{Checking, ToolServers};
use super::sign_ins::signed_in;
use super::support::{edit, files_in, named, path, sent, servers_in, with_key, with_neither};
use crate::store::Store;

const URL: &str = "https://mcp.example.com/mcp";
/// The same server, asked for some of its tools.
const QUERIED: &str = "https://mcp.example.com/mcp?tools=some";
/// Another.
const ELSEWHERE: &str = "https://eu.mcp.example.com/mcp";

/// The ID of the server that's asked, if it's the hub's, its URL, and the header it's sent.
type Asks = (Option<String>, String, Option<String>);

/// What a check of `of` would ask, if anything, which changes nothing, refused or not.
fn check(
    dir: &Path,
    (servers, store): &(ToolServers, std::sync::Arc<Store>),
    of: ToolServerToCheck,
) -> Result<Option<Asks>, ApiError> {
    let everything = || (store.list_records().unwrap().tool_servers, files_in(dir), sent(servers));
    let before = everything();
    let checking = servers.checking(ToolServerCheck { of });
    assert_eq!(everything(), before);
    Ok(match checking? {
        Checking::Nothing => None,
        Checking::Ask(ask) => {
            let header = ask.header.map(|header| format!("{}: {}", header.name.as_str(), header.value.as_str()));
            Some((ask.id.map(|id| id.to_string()), ask.url.as_str().to_string(), header))
        }
    })
}

fn at(id: &str, url: &str) -> ToolServerEdit {
    ToolServerEdit { url: Some(url.into()), ..edit(id) }
}

/// Having the server `id` know the person by `key`, or by the one it has, given for the server at `given_for`.
fn keyed(changes: ToolServerEdit, header: &str, key: Option<&str>, given_for: &str) -> ToolServerEdit {
    let key = key.map(ToolServerKey::new);
    ToolServerEdit {
        auth: Some(ToolServerAuthEdit::Key { header: header.into(), key, url: given_for.into() }),
        ..changes
    }
}

fn neither(changes: ToolServerEdit) -> ToolServerEdit {
    ToolServerEdit { auth: Some(ToolServerAuthEdit::Neither), ..changes }
}

fn asks(id: Option<&str>, url: &str, header: Option<&str>) -> Asks {
    (id.map(str::to_string), url.to_string(), header.map(str::to_string))
}

#[test]
fn a_server_to_add_is_asked_with_what_workers_would_send_it_and_one_there_is_isnt() {
    let dir = tempfile::tempdir().unwrap();
    let hub = servers_in(dir.path());
    let add = |add| check(dir.path(), &hub, ToolServerToCheck::Add(add)).unwrap();
    let in_header = |header: &str| ToolServerAdd {
        auth: ToolServerAuthGiven::Key { header: header.into(), key: ToolServerKey::new(" made-up\n") },
        ..with_neither("t", "Linear")
    };

    assert_eq!(add(with_key("t", "Linear", "made-up")), Some(asks(None, URL, Some("Authorization: Bearer made-up"))));
    assert_eq!(
        add(with_key("t", "Linear", "Basic made-up")),
        Some(asks(None, URL, Some("Authorization: Basic made-up")))
    );
    assert_eq!(add(in_header("X-Key")), Some(asks(None, URL, Some("X-Key: made-up"))));
    assert_eq!(add(with_neither("t", "Linear")), Some(asks(None, URL, None)));
    let queried = ToolServerAdd { url: format!(" {QUERIED}"), ..with_neither("t", "Linear") };
    assert_eq!(add(queried), Some(asks(None, QUERIED, None)));

    hub.0.add(with_neither("t", "Linear")).unwrap();
    assert_eq!(add(with_key("t", "Linear", "made-up")), None, "it's that server again");
}

#[test]
fn an_edit_asks_only_where_it_changes_where_the_server_is_or_what_its_sent() {
    let dir = tempfile::tempdir().unwrap();
    let hub = servers_in(dir.path());
    hub.0.add(with_key("k", "Linear", "Bearer made-up")).unwrap();
    hub.0.add(with_neither("n", "Docs")).unwrap();
    signed_in(&hub.0, "s", "Figma");
    let asked = |edit| check(dir.path(), &hub, ToolServerToCheck::Edit(edit)).unwrap();
    let kept = Some("Authorization: Bearer made-up");

    for nothing in [
        edit("k"),
        named("k", "Sentry"),
        named("n", "Notes"),
        at("k", URL),
        keyed(edit("k"), "Authorization", None, URL),
        at("n", URL),
        neither(named("n", "Notes")),
        // Its sign-in says whether it works.
        named("s", "Sketch"),
        at("s", QUERIED),
    ] {
        let said = format!("{nothing:?}");
        assert_eq!(asked(nothing), None, "{said}");
    }
    assert_eq!(asked(at("k", QUERIED)), Some(asks(Some("k"), QUERIED, kept)));
    assert_eq!(asked(keyed(edit("k"), "X-Key", None, URL)), Some(asks(Some("k"), URL, Some("X-Key: Bearer made-up"))));
    assert_eq!(asked(keyed(at("k", QUERIED), "Authorization", None, URL)), Some(asks(Some("k"), QUERIED, kept)));
    let another = Some("Authorization: Bearer another");
    assert_eq!(asked(keyed(edit("k"), "Authorization", Some("another"), URL)), Some(asks(Some("k"), URL, another)));
    let moved = keyed(at("k", ELSEWHERE), "Authorization", Some("another"), ELSEWHERE);
    assert_eq!(asked(moved), Some(asks(Some("k"), ELSEWHERE, another)));
    assert_eq!(asked(neither(edit("k"))), Some(asks(Some("k"), URL, None)));
    assert_eq!(asked(neither(at("k", ELSEWHERE))), Some(asks(Some("k"), ELSEWHERE, None)), "with no key");
    assert_eq!(asked(at("n", QUERIED)), Some(asks(Some("n"), QUERIED, None)));
    assert_eq!(asked(at("n", ELSEWHERE)), Some(asks(Some("n"), ELSEWHERE, None)));
    assert_eq!(asked(keyed(edit("s"), "X-Key", Some("key"), URL)), Some(asks(Some("s"), URL, Some("X-Key: key"))));
    assert_eq!(asked(neither(edit("s"))), Some(asks(Some("s"), URL, None)));
}

/// Whatever else is changed with it, and whoever has moved the server meanwhile.
#[test]
fn a_key_thats_kept_is_asked_with_only_where_it_was_given_for() {
    let dir = tempfile::tempdir().unwrap();
    let hub = servers_in(dir.path());
    hub.0.add(with_key("k", "Linear", "made-up")).unwrap();
    let code = |edit| check(dir.path(), &hub, ToolServerToCheck::Edit(edit)).unwrap_err().code;

    assert_eq!(code(at("k", ELSEWHERE)), ErrorCode::InvalidRequest);
    assert_eq!(code(keyed(at("k", ELSEWHERE), "X-Key", None, ELSEWHERE)), ErrorCode::InvalidRequest);
    assert_eq!(code(keyed(at("k", ELSEWHERE), "X-Key", None, URL)), ErrorCode::Conflict);
    hub.0.edit(keyed(at("k", ELSEWHERE), "X-Key", Some("another"), ELSEWHERE)).unwrap();
    assert_eq!(code(keyed(edit("k"), "X-Key", None, URL)), ErrorCode::Conflict, "moved meanwhile");
    assert_eq!(code(keyed(edit("k"), "X-Key", Some("made-up"), URL)), ErrorCode::Conflict);
}

#[test]
fn a_check_is_refused_as_what_its_of_is() {
    let dir = tempfile::tempdir().unwrap();
    let hub = servers_in(dir.path());
    hub.0.add(with_key("k", "Linear", "made-up")).unwrap();
    hub.0.add(with_neither("n", "Docs")).unwrap();
    let given = |auth| ToolServerAdd { auth, ..with_neither("t", "Figma") };
    let key = |header: &str, key: &str| {
        given(ToolServerAuthGiven::Key { header: header.into(), key: ToolServerKey::new(key) })
    };
    let adds = [
        with_neither("t", " "),
        with_neither("t", "linear"),
        ToolServerAdd { url: "mcp.example.com".into(), ..with_neither("t", "Figma") },
        key("Host", "key"),
        key("X-Key", "two\nlines"),
        given(ToolServerAuthGiven::Unknown),
    ];
    let edits = [
        named("gone", "Figma"),
        named("k", "docs"),
        at("k", "https://ada:made-up@mcp.example.com/mcp"),
        keyed(edit("k"), "Host", Some("key"), URL),
        keyed(edit("n"), "X-Key", None, URL),
        ToolServerEdit { auth: Some(ToolServerAuthEdit::Unknown), ..edit("k") },
    ];

    for add in adds {
        let refused = hub.0.add(add.clone()).unwrap_err();
        assert_eq!(check(dir.path(), &hub, ToolServerToCheck::Add(add)).unwrap_err(), refused);
    }
    for edit in edits {
        let refused = hub.0.edit(edit.clone()).unwrap_err();
        assert_eq!(check(dir.path(), &hub, ToolServerToCheck::Edit(edit)).unwrap_err(), refused);
    }
    let unknown = check(dir.path(), &hub, ToolServerToCheck::Unknown).unwrap_err();
    assert_eq!(unknown.code, ErrorCode::InvalidRequest, "not that the hub checks none");
}

/// Workers aren't sent it either, and saving the edit goes as it would.
#[test]
fn a_server_whose_key_this_build_cant_read_is_asked_nothing() {
    for holds in [r#"{"format":"made_up"}"#, "cut sho"] {
        let dir = tempfile::tempdir().unwrap();
        let hub = servers_in(dir.path());
        let ToolServer { id, .. } = hub.0.add(with_key("k", "Linear", "made-up")).unwrap().server;
        std::fs::write(path(dir.path(), &hub.1, &id), holds).unwrap();

        assert_eq!(check(dir.path(), &hub, ToolServerToCheck::Edit(at("k", QUERIED))).unwrap(), None, "{holds}");
    }
}

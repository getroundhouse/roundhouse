//! Editing a server without signing in to it: what changes, what's kept and forgotten, and what's refused.

use std::path::Path;

use protocol::client::tool_servers::ToolServerEdit;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    ToolServer, ToolServerAuth, ToolServerAuthEdit, ToolServerHeader, ToolServerKey, ToolServerName, ToolServerUrl,
};

use super::super::{ToolServerChange, ToolServers};
use super::sign_ins::{GIVEN_AT_MS, LASTS, signed_in};
use super::support::{
    KEY_FILE, edit, files_in, holding, named, path, refused, sent, servers_in, stop_writing_rows, with_key,
    with_neither,
};
use crate::store::{AddingToolServer, Store, later_ms};

/// Where each server here is as it's added.
const URL: &str = "https://mcp.example.com/mcp";
/// The same, asked for some of its tools.
const QUERIED: &str = "https://MCP.example.com:443/mcp?tools=some";
/// Another's.
const ELSEWHERE: &str = "https://eu.mcp.example.com/mcp";

/// Having the server `id` be at `url`.
fn at(id: &str, url: &str) -> ToolServerEdit {
    ToolServerEdit { url: Some(url.into()), ..edit(id) }
}

/// Having the server `id` know the person by a key, or by the one it has, given for `url`.
fn keyed(id: &str, header: &str, key: Option<&str>, url: &str) -> ToolServerEdit {
    let auth = ToolServerAuthEdit::Key { header: header.into(), key: key.map(ToolServerKey::new), url: url.into() };
    ToolServerEdit { auth: Some(auth), ..edit(id) }
}

fn neither(id: &str) -> ToolServerEdit {
    ToolServerEdit { auth: Some(ToolServerAuthEdit::Neither), ..edit(id) }
}

fn key_in(header: &str) -> ToolServerAuth {
    ToolServerAuth::Key { header: ToolServerHeader::typed(header).unwrap() }
}

/// Where workers are sent the server `t`.
fn sent_url(servers: &ToolServers) -> String {
    servers.sent_server(&"t".into()).unwrap().url.as_str().to_string()
}

/// What each request to it carries.
fn carries(servers: &ToolServers) -> Option<String> {
    sent(servers).remove(0).1
}

/// Every record the hub has, and every file, which a refusal leaves as they were.
fn everything(dir: &Path, store: &Store) -> (Vec<ToolServer>, Vec<String>) {
    (store.list_records().unwrap().tool_servers, files_in(dir))
}

/// That `asked` is refused with `code`, whatever's refused of it, and leaves everything as it was.
fn refuses(dir: &Path, servers: &ToolServers, store: &Store, asked: ToolServerEdit) -> ApiError {
    let before = (everything(dir, store), sent(servers));
    let e = servers.edit(asked).unwrap_err();
    assert_eq!((everything(dir, store), sent(servers)), before, "{e:?}");
    e
}

#[test]
fn an_edit_changes_what_it_gives_alone_and_the_same_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    let added = servers.add(with_key("t", "Linear", "Bearer made-up")).unwrap().server;

    let renamed = servers.edit(named("t", " Sentry ")).unwrap();
    let again = servers.edit(named("t", "Sentry")).unwrap();
    let queried = servers.edit(at("t", &format!(" {QUERIED}\n"))).unwrap();
    let queried_again = servers.edit(at("t", QUERIED)).unwrap();
    let nothing = servers.edit(edit("t")).unwrap();

    assert!(renamed.changed && renamed.server.rev > added.rev);
    let name = ToolServerName::typed("Sentry").unwrap();
    assert_eq!(renamed.server, ToolServer { name, rev: renamed.server.rev, ..added });
    assert_eq!(again, ToolServerChange { server: renamed.server.clone(), changed: false });
    let url = ToolServerUrl::typed(QUERIED).unwrap();
    assert_eq!(queried.server, ToolServer { url, rev: queried.server.rev, ..renamed.server });
    assert!(queried.changed);
    assert_eq!((queried_again.changed, nothing.changed), (false, false));
    assert_eq!((&queried_again.server, &nothing.server), (&queried.server, &queried.server));
    assert_eq!(files_in(dir.path()), [KEY_FILE]);
    assert_eq!((sent_url(&servers), carries(&servers)), (QUERIED.into(), Some("Authorization: Bearer made-up".into())));
}

#[test]
fn what_cant_be_a_servers_is_refused_as_its_edited_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    servers.add(with_key("t", "Linear", "Bearer made-up")).unwrap();
    servers.add(with_neither("n", "Docs")).unwrap();
    let unknown = ToolServerEdit { auth: Some(ToolServerAuthEdit::Unknown), ..edit("t") };
    let asked = [
        (named("t", " "), ErrorCode::InvalidRequest),
        (named("t", "docs"), ErrorCode::Conflict),
        (at("t", "https://ada:made-up@mcp.example.com/mcp"), ErrorCode::InvalidRequest),
        (keyed("t", "Host", Some("key"), URL), ErrorCode::InvalidRequest),
        (keyed("t", "X-Key", Some("two\nlines"), URL), ErrorCode::InvalidRequest),
        (keyed("t", "X-Key", Some(" "), URL), ErrorCode::InvalidRequest),
        (keyed("t", "X-Key", Some("key"), "not a URL"), ErrorCode::InvalidRequest),
        (keyed("n", "X-Key", None, URL), ErrorCode::InvalidRequest),
        (unknown, ErrorCode::InvalidRequest),
        (named("gone", "Figma"), ErrorCode::NotFound),
        // With something that could be kept, which isn't, since the rest can't.
        (ToolServerEdit { name: Some("Sentry".into()), ..at("t", "mcp.example.com") }, ErrorCode::InvalidRequest),
        (ToolServerEdit { name: Some("Docs".into()), ..neither("t") }, ErrorCode::Conflict),
    ];
    for (asked, code) in asked {
        let said = format!("{asked:?}");
        assert_eq!(refuses(dir.path(), &servers, &store, asked).code, code, "{said}");
    }
}

/// Whichever way the client asks, since it's the hub that keeps what one server knows the person by from another.
#[test]
fn a_key_and_a_sign_in_are_kept_as_the_query_changes_and_a_server_is_never_moved_with_them() {
    let others = [
        ELSEWHERE,
        "http://mcp.example.com/mcp",
        "https://mcp.example.com:8443/mcp",
        "https://mcp.example.com/",
        "https://mcp.example.com/mcp/",
    ];
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    servers.add(with_key("k", "Figma", "made-up")).unwrap();
    signed_in(&servers, "t", "Linear");
    let (name, url) = (ToolServerName::typed("Newer").unwrap(), ToolServerUrl::typed(URL).unwrap());
    let auth = ToolServerAuth::Unknown;
    let newer = AddingToolServer { id: &"newer".into(), name: &name, url: &url, auth: &auth, secret: None };
    store.add_tool_server(&newer).unwrap();
    let kept = files_in(dir.path());

    for id in ["k", "t", "newer"] {
        for other in others {
            let e = refuses(dir.path(), &servers, &store, at(id, other));
            assert_eq!(e.code, ErrorCode::InvalidRequest, "{id} to {other}: {e:?}");
        }
    }
    for other in others {
        // The key it has, for where it is, or for where it would be.
        for given_for in [URL, other] {
            let asked = ToolServerEdit { url: Some(other.into()), ..keyed("k", "Authorization", None, given_for) };
            refuses(dir.path(), &servers, &store, asked);
        }
        let headers_alone = keyed("k", "X-Key", None, other);
        assert_eq!(refuses(dir.path(), &servers, &store, headers_alone).code, ErrorCode::Conflict);
    }

    for id in ["k", "t", "newer"] {
        assert!(servers.edit(at(id, QUERIED)).unwrap().changed);
    }
    assert_eq!(files_in(dir.path()), kept);
    let carried = [
        ("k".to_string(), Some("Authorization: Bearer made-up".to_string())),
        ("t".to_string(), Some("Authorization: Bearer made-up-t".to_string())),
    ];
    assert_eq!(sent(&servers), carried);
    assert_eq!(sent_url(&servers), QUERIED);
}

#[test]
fn a_server_is_moved_with_its_own_key_or_where_it_has_none_and_the_key_before_is_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    servers.add(with_key("t", "Linear", "made-up")).unwrap();
    servers.add(with_neither("n", "Docs")).unwrap();
    let file = path(dir.path(), &store, &"t".into());
    let moving =
        || ToolServerEdit { url: Some(ELSEWHERE.into()), ..keyed("t", "Authorization", Some("another"), ELSEWHERE) };

    let moved = servers.edit(moving()).unwrap();
    let again = servers.edit(moving()).unwrap();
    let moved_neither = servers.edit(at("n", ELSEWHERE)).unwrap();

    assert!(moved.changed && moved_neither.changed);
    assert_eq!(moved.server.url.as_str(), ELSEWHERE);
    assert_eq!(again, ToolServerChange { server: moved.server, changed: false });
    assert_eq!((sent_url(&servers), moved_neither.server.url.as_str()), (ELSEWHERE.into(), ELSEWHERE));
    assert_eq!(servers.sent_server(&"t".into()).unwrap().header.unwrap().value.as_str(), "Bearer another");
    assert_eq!(files_in(dir.path()), [r#"{"format":"key","key":"another"}"#]);
    let moved_to = path(dir.path(), &store, &"t".into());
    assert_ne!(moved_to, file, "another server's is in another file");
    let back = ToolServerEdit { url: Some(URL.into()), ..keyed("t", "Authorization", Some("another"), URL) };
    servers.edit(back).unwrap();
    assert_ne!(path(dir.path(), &store, &"t".into()), moved_to, "so is the same key");
    assert_eq!(holding(dir.path(), "made-up"), Vec::<String>::new());
}

/// As two clients do that each saw the server as it was.
#[test]
fn edits_of_different_things_undo_nothing_and_a_key_for_where_it_was_isnt_sent_to_where_its_moved() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    servers.add(with_key("t", "Linear", "made-up")).unwrap();

    servers.edit(named("t", "Sentry")).unwrap();
    servers.edit(at("t", QUERIED)).unwrap();
    servers.edit(keyed("t", "Authorization", Some("another"), URL)).unwrap();
    let all = servers.edit(edit("t")).unwrap().server;
    let moving = ToolServerEdit { url: Some(ELSEWHERE.into()), ..keyed("t", "X-Key", Some("third"), ELSEWHERE) };
    servers.edit(moving).unwrap();
    let for_where_it_was = refuses(dir.path(), &servers, &store, keyed("t", "X-Key", Some("fourth"), QUERIED));

    assert_eq!((all.name.as_str(), all.url.as_str(), all.auth), ("Sentry", QUERIED, key_in("Authorization")));
    assert_eq!(for_where_it_was.code, ErrorCode::Conflict);
    assert_eq!(carries(&servers).as_deref(), Some("X-Key: third"));
}

/// The record says nothing of the key, so a new one alone leaves it as it is.
#[test]
fn a_key_given_takes_the_place_of_the_one_there_was_and_one_left_out_is_kept_in_whichever_header() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let added = servers.add(with_key("t", "Linear", "made-up")).unwrap().server;
    let edited = |asked| servers.edit(asked).unwrap();
    let key = |key| format!(r#"{{"format":"key","key":"{key}"}}"#);

    let another = edited(keyed("t", "Authorization", Some(" another\n"), URL));
    assert_eq!(another, ToolServerChange { server: added.clone(), changed: false });
    assert_eq!(carries(&servers).as_deref(), Some("Authorization: Bearer another"));
    assert_eq!(files_in(dir.path()), [key("another")]);
    assert_eq!(edited(keyed("t", "Authorization", Some("another"), URL)), another, "the same again");

    let kept = edited(keyed("t", "X-Key", None, URL));
    assert_eq!((kept.changed, &kept.server.auth), (true, &key_in("X-Key")));
    assert_eq!(carries(&servers).as_deref(), Some("X-Key: another"), "as it was given");
    assert_eq!(files_in(dir.path()), [key("another")]);
    assert_eq!(edited(keyed("t", "X-Key", None, URL)), ToolServerChange { changed: false, ..kept });

    let both = edited(keyed("t", "X-Other", Some("third"), URL));
    assert_eq!((both.changed, &both.server.auth), (true, &key_in("X-Other")));
    assert_eq!(files_in(dir.path()), [key("third")]);
    assert_eq!(store.list_records().unwrap().tool_servers, [both.server]);
}

#[test]
fn how_a_server_knows_the_person_changes_and_whats_no_use_any_more_is_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    servers.add(with_key("k", "Figma", "made-up-k")).unwrap();
    servers.add(with_neither("n", "Docs")).unwrap();
    signed_in(&servers, "s", "Linear");
    signed_in(&servers, "t", "Sentry");
    let edited = |asked| servers.edit(asked).unwrap().server.auth;

    assert_eq!(edited(neither("k")), ToolServerAuth::Neither);
    assert_eq!(edited(neither("s")), ToolServerAuth::Neither);
    assert_eq!(edited(keyed("t", "X-Key", Some("key-t"), URL)), key_in("X-Key"));
    assert_eq!(edited(keyed("n", "X-Key", Some("key-n"), URL)), key_in("X-Key"));
    let again = servers.edit(neither("k")).unwrap();

    assert!(!again.changed);
    assert_eq!(holding(dir.path(), "made-up"), Vec::<String>::new());
    assert_eq!(files_in(dir.path()), [r#"{"format":"key","key":"key-n"}"#, r#"{"format":"key","key":"key-t"}"#]);
    let mut carried = sent(&servers);
    carried.sort();
    let of = |id: &str, carried: Option<&str>| (id.to_string(), carried.map(str::to_string));
    assert_eq!(carried, [of("k", None), of("n", Some("X-Key: key-n")), of("s", None), of("t", Some("X-Key: key-t"))]);
    assert_eq!(servers.sent_server(&"s".into()).unwrap().sign_in, None);
    assert_eq!(servers.check_sign_ins(later_ms(GIVEN_AT_MS, LASTS)).unwrap().due.len(), 0, "nothing's left to renew");
}

/// And its file isn't there as nothing, which tidying would remove the server for.
#[test]
fn a_hub_that_stops_before_an_edit_is_written_has_the_server_as_it_was() {
    let moving = ToolServerEdit { url: Some(ELSEWHERE.into()), ..keyed("k", "X-Key", Some("another"), ELSEWHERE) };
    for asked in [moving, neither("k"), neither("s"), keyed("s", "X-Key", Some("another"), URL)] {
        let dir = tempfile::tempdir().unwrap();
        let (servers, store) = servers_in(dir.path());
        servers.add(with_key("k", "Figma", "made-up")).unwrap();
        signed_in(&servers, "s", "Linear");
        let before = (everything(dir.path(), &store), sent(&servers));
        stop_writing_rows(&store);

        assert_eq!(refused(servers.edit(asked)), ErrorCode::Internal);

        assert_eq!((everything(dir.path(), &store), sent(&servers)), before);
    }
}

/// The rest of what the server is stays, where a file that's gone has tidying remove one.
#[test]
fn a_hub_that_stops_once_an_edit_is_written_has_the_server_as_it_was_edited_and_forgets_what_it_had() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    servers.add(with_key("t", "Linear", "made-up")).unwrap();
    let file = path(dir.path(), &store, &"t".into());
    let edited = servers.edit(neither("t")).unwrap().server;
    std::fs::write(&file, KEY_FILE).unwrap();

    servers.tidy().unwrap();

    assert_eq!(everything(dir.path(), &store), (vec![edited], vec![]));
}

#[test]
fn a_renewal_under_way_is_kept_across_an_edit_of_the_query_and_let_go_of_across_another_way() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    let due = servers.check_sign_ins(later_ms(GIVEN_AT_MS, LASTS)).unwrap().due.remove(0);

    servers.edit(at("t", QUERIED)).unwrap();
    assert!(servers.holds(&due).unwrap());
    servers.edit(keyed("t", "X-Key", Some("key"), URL)).unwrap();

    assert!(!servers.holds(&due).unwrap());
}

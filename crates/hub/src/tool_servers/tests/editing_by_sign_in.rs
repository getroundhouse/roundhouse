//! Editing a server by signing in to it: nothing changes until it's approved, and then all of it does at once. How it's
//! approved is tested in `crate::hub`.

use protocol::client::ErrorCode;
use protocol::client::tool_servers::ToolServerEdit;
use protocol::client::tool_sign_ins::ToolSignInTo;
use protocol::model::{
    ToolScopes, ToolServer, ToolServerAuth, ToolServerAuthEdit, ToolServerKey, ToolServerName, ToolServerUrl,
    ToolSignInStatus,
};
use protocol::worker::{AskForToolAccess, ServerSignIn, TokenNumber};
use serde_json::json;
use tool_sign_in::{Issuer, SignedIn};

use super::super::{SigningIn, ToolServerChange, ToolServers};
use super::sign_ins::{GIVEN_AT_MS, LASTS, again, as_first, granted, signed_in};
use super::support::{
    edit, files_in, holding, named, path, refused, sent, servers_in, stop_writing_rows, with_key, with_neither,
};
use crate::store::later_ms;

const ELSEWHERE: &str = "https://eu.mcp.example.com/mcp";

const WORKS: ToolServerAuth = ToolServerAuth::SignIn { status: ToolSignInStatus::Works };

/// Signing in to the server `id` to have it at `url`, called `name`.
fn to(id: &str, name: Option<&str>, url: Option<&str>) -> ToolSignInTo {
    let given = |text: Option<&str>| text.map(str::to_string);
    ToolSignInTo::Edit { id: id.into(), name: given(name), url: given(url) }
}

fn keep(servers: &ToolServers, to: &SigningIn) -> Result<ToolServerChange, protocol::client::ApiError> {
    servers.keep_sign_in("edit".into(), to, as_first(granted("made-up-edit")), GIVEN_AT_MS)
}

fn key(id: &str, url: &str) -> ToolServerEdit {
    let auth =
        ToolServerAuthEdit::Key { header: "X-Key".into(), key: Some(ToolServerKey::new("key")), url: url.into() };
    ToolServerEdit { auth: Some(auth), ..edit(id) }
}

fn neither(id: &str) -> ToolServerEdit {
    ToolServerEdit { auth: Some(ToolServerAuthEdit::Neither), ..edit(id) }
}

#[test]
fn a_server_is_edited_once_its_signed_in_to_however_it_knew_the_person_and_what_it_had_is_forgotten() {
    for (id, name, url) in [
        ("s", None, Some(ELSEWHERE)),
        ("s", Some("Linear (EU)"), Some(ELSEWHERE)),
        ("k", None, None),
        ("k", None, Some(ELSEWHERE)),
        ("n", Some("Notion"), Some("https://mcp.example.com/mcp?tools=some")),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (servers, store) = servers_in(dir.path());
        servers.add(with_key("k", "Figma", "made-up-k")).unwrap();
        servers.add(with_neither("n", "Docs")).unwrap();
        signed_in(&servers, "s", "Linear");
        let record = || store.tool_server(&id.into()).unwrap().unwrap().0;
        let (before, files, sent_before) = (record(), files_in(dir.path()), sent(&servers));

        let signing_in = servers.signing_in(to(id, name, url)).unwrap();
        assert_eq!((record(), files_in(dir.path()), sent(&servers)), (before.clone(), files, sent_before));
        let change = keep(&servers, &signing_in).unwrap();

        let name = name.map_or(before.name, |name| ToolServerName::typed(name).unwrap());
        let url = url.map_or(before.url, |url| ToolServerUrl::typed(url).unwrap());
        assert_eq!(signing_in.url(), &url, "it's signed in to where it's to be");
        let expected = ToolServer { id: id.into(), name, url, auth: WORKS, rev: change.server.rev };
        assert_eq!((change.changed, &change.server), (true, &expected));
        assert_eq!(record(), expected);
        assert_eq!(holding(dir.path(), &format!("made-up-{id}")), Vec::<String>::new(), "{id}: what it had is gone");
        assert_eq!(holding(dir.path(), "made-up-edit"), ["tool-servers"]);
        let proxied = servers.sent_server(&id.into()).unwrap();
        assert_eq!(
            (proxied.url, proxied.header.unwrap().value.as_str()),
            (expected.url.clone(), "Bearer made-up-edit")
        );
        assert_eq!(servers.signed_in_by(&"edit".into()).unwrap(), Some(expected), "so it's finished again by it");
        // Which is what has a run that couldn't connect to it asked to again.
        assert_eq!(servers.sent().borrow().signed_in().by(&id.into()), Some(&"edit".into()));
    }
}

/// Its worker says which of the server's tokens it was, with nothing of where, so no two of a server's have a number.
#[test]
fn a_moved_servers_tokens_go_on_from_the_ones_before_and_none_of_those_is_renewed_or_wanted_of() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    let due = servers.check_sign_ins(later_ms(GIVEN_AT_MS, LASTS)).unwrap().due.remove(0);
    let file = path(dir.path(), &store, &"t".into());

    let signing_in = servers.signing_in(to("t", None, Some(ELSEWHERE))).unwrap();
    keep(&servers, &signing_in).unwrap();

    let second = TokenNumber::FIRST.next();
    assert_eq!(servers.sent_server(&"t".into()).unwrap().sign_in, Some(ServerSignIn::Works { token: second }));
    assert_ne!(path(dir.path(), &store, &"t".into()), file);
    assert!(!servers.holds(&due).unwrap());
    assert!(servers.refused(&"t".into(), TokenNumber::FIRST).unwrap().due.is_empty());
    let wants = ToolScopes::of(["write"]);
    let asked = AskForToolAccess { server: "t".into(), refused: TokenNumber::FIRST, wants };
    assert!(servers.wants_access(&asked).unwrap().change.is_none());
}

/// At one authorization server, which knows Roundhouse by one registration, whichever of its servers is signed in to.
#[test]
fn a_moved_server_asks_for_nothing_the_one_before_was_asked_for_or_wanted_and_uses_its_registration() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    let registration = json!({
        "client_id": "made-up-client",
        "issuer": "https://auth.example.com",
        "redirect_uris": ["http://127.0.0.1:17867/callback"],
    });
    let registration = serde_json::from_value(registration).unwrap();
    let signed = SignedIn { registration: Some(registration), ..as_first(granted("made-up-t")) };
    let issuer: Issuer = signed.renewable.issuer().clone();
    let first = servers.signing_in(super::sign_ins::new("t", "Linear")).unwrap();
    servers.keep_sign_in("t".into(), &first, signed, GIVEN_AT_MS).unwrap();
    let wants = ToolScopes::of(["write"]);
    let asked = AskForToolAccess { server: "t".into(), refused: TokenNumber::FIRST, wants };
    servers.wants_access(&asked).unwrap();

    let in_place = servers.signing_in(to("t", Some("Sentry"), None)).unwrap();
    let moved = servers.signing_in(to("t", None, Some(ELSEWHERE))).unwrap();
    let kept = keep(&servers, &moved).unwrap().server;

    assert_eq!(in_place.asked_before(&issuer).to_string(), "read write");
    assert_eq!(moved.asked_before(&issuer).to_string(), "");
    assert!(in_place.registered().is_some() && moved.registered().is_some());
    assert_eq!(kept.auth, WORKS, "what the one before wanted is nothing to this one");
}

#[test]
fn what_cant_be_edited_by_signing_in_is_refused_as_it_starts() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    servers.add(with_neither("n", "Docs")).unwrap();
    signed_in(&servers, "t", "Linear");
    let start = |to| refused(servers.signing_in(to));

    assert_eq!(start(to("gone", None, None)), ErrorCode::NotFound);
    assert_eq!(start(to("t", Some(" "), None)), ErrorCode::InvalidRequest);
    assert_eq!(start(to("t", Some("docs"), None)), ErrorCode::Conflict);
    assert_eq!(start(to("t", None, Some("mcp.example.com"))), ErrorCode::InvalidRequest);
}

/// A server to edit, and what's done to it, or to another, while its sign-in waits.
type Meanwhile = (&'static str, fn(&ToolServers));

/// And nothing of it is kept, so what's signed in to at one place is never used at another.
#[test]
fn a_server_thats_another_once_its_approved_or_whose_name_is_taken_is_refused_and_left_as_it_is() {
    let meanwhile: [Meanwhile; 6] = [
        ("k", |servers| {
            servers.remove(&"k".into()).unwrap();
        }),
        ("k", |servers| {
            servers.edit(ToolServerEdit { url: Some(ELSEWHERE.into()), ..key("k", ELSEWHERE) }).unwrap();
        }),
        ("n", |servers| {
            servers.edit(ToolServerEdit { url: Some(ELSEWHERE.into()), ..edit("n") }).unwrap();
        }),
        ("s", |servers| {
            servers.edit(neither("s")).unwrap();
        }),
        ("s", |servers| {
            let to = servers.signing_in(to("s", None, Some(ELSEWHERE))).unwrap();
            servers.keep_sign_in("other".into(), &to, as_first(granted("made-up-other")), GIVEN_AT_MS).unwrap();
        }),
        ("s", |servers| {
            servers.add(with_neither("taken", "Taken")).unwrap();
        }),
    ];
    for (at, (id, change)) in meanwhile.into_iter().enumerate() {
        let dir = tempfile::tempdir().unwrap();
        let (servers, store) = servers_in(dir.path());
        servers.add(with_key("k", "Figma", "made-up-k")).unwrap();
        servers.add(with_neither("n", "Docs")).unwrap();
        signed_in(&servers, "s", "Linear");
        let signing_in = servers.signing_in(to(id, Some("Taken"), None)).unwrap();
        change(&servers);
        let everything = || (store.list_records().unwrap().tool_servers, files_in(dir.path()));
        let before = everything();

        let kept = keep(&servers, &signing_in);

        assert_eq!(refused(kept), ErrorCode::Conflict, "{at}");
        assert_eq!(everything(), before, "{at}");
    }
}

#[test]
fn signing_in_again_goes_on_across_an_edit_of_the_name_and_is_refused_once_the_server_is_another() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    let started = || servers.signing_in(ToolSignInTo::Again { id: "t".into() }).unwrap();

    let across_a_name = started();
    servers.edit(named("t", "Sentry")).unwrap();
    assert!(keep(&servers, &across_a_name).is_ok());
    let across_a_move = started();
    let moving = servers.signing_in(to("t", None, Some(ELSEWHERE))).unwrap();
    servers.keep_sign_in("moved".into(), &moving, as_first(granted("made-up-moved")), GIVEN_AT_MS).unwrap();
    let before = store.list_records().unwrap();

    assert_eq!(refused(keep(&servers, &across_a_move)), ErrorCode::Conflict);
    assert_eq!(store.list_records().unwrap(), before);
    assert_eq!(holding(dir.path(), "made-up-edit"), Vec::<String>::new());
    assert!(again(&servers, "t").is_ok(), "one started since is of where it is");
}

#[test]
fn a_hub_that_stops_before_the_edit_is_written_has_the_server_and_what_it_had_as_they_were() {
    for id in ["s", "k", "n"] {
        let dir = tempfile::tempdir().unwrap();
        let (servers, store) = servers_in(dir.path());
        servers.add(with_key("k", "Figma", "made-up-k")).unwrap();
        servers.add(with_neither("n", "Docs")).unwrap();
        signed_in(&servers, "s", "Linear");
        let everything = || (store.list_records().unwrap().tool_servers, files_in(dir.path()), sent(&servers));
        let before = everything();
        let signing_in = servers.signing_in(to(id, None, Some(ELSEWHERE))).unwrap();
        stop_writing_rows(&store);

        assert_eq!(refused(keep(&servers, &signing_in)), ErrorCode::Internal);

        assert_eq!(everything(), before);
    }
}

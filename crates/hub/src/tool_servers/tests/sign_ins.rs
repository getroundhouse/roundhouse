//! Signing in to a server: what's kept of it, where, and what's refused as it starts and as it's kept.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::tool_sign_ins::ToolSignInTo;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ToolServer, ToolServerAuth, ToolSignInStatus};
use serde_json::json;
use tool_sign_in::{AccessToken, Granted, RefreshToken, SignedIn};

use super::super::secret::ToolServerSecret;
use super::super::{ToolServerChange, ToolServers};
use super::support::{files_in, holding, refused, servers_in, with_key, with_neither};
use crate::login_files::{LoginSecret as _, Stored};
use crate::store::Store;

/// When each sign-in here is given its tokens, by the hub's clock.
pub(super) const GIVEN_AT_MS: i64 = 1_000;

/// How long its access token lasts.
pub(super) const LASTS: Duration = Duration::from_hours(1);

/// What an authorization server gives, whose access token is `access`, and whose refresh token is that one's.
pub(super) fn granted(access: &str) -> Granted {
    Granted {
        access: AccessToken::new(access),
        lasts: Some(LASTS),
        refresh: Some(RefreshToken::new(format!("{access}-refresh"))),
    }
}

/// What a person signed in at the authorization server `issuer`, asked to allow `scopes` and given `granted`.
pub(super) fn given(scopes: &str, issuer: &str, granted: Granted) -> SignedIn {
    let renewable = json!({
        "server": "https://mcp.example.com/mcp",
        "resource": "https://mcp.example.com/mcp",
        "issuer": issuer,
        "token_endpoint": "https://auth.example.com/token",
        "client_id": "https://client.example/client.json",
        "scopes": scopes,
    });
    SignedIn { renewable: serde_json::from_value(renewable).unwrap(), granted }
}

/// What a person signed in, asked to allow `read`, as each server here first is.
fn as_first(granted: Granted) -> SignedIn {
    given("read", "https://auth.example.com", granted)
}

fn new(id: &str, name: &str) -> ToolSignInTo {
    ToolSignInTo::New { id: id.into(), name: name.into(), url: "https://mcp.example.com/mcp".into() }
}

/// The server `id`, called `name`, signed in to by the sign-in of the same ID, with the access token `made-up-{id}`.
pub(super) fn signed_in(servers: &ToolServers, id: &str, name: &str) -> ToolServer {
    signed_in_for(servers, id, name, granted(&format!("made-up-{id}")))
}

/// The same, given `granted`.
pub(super) fn signed_in_for(servers: &ToolServers, id: &str, name: &str, granted: Granted) -> ToolServer {
    let to = servers.signing_in(new(id, name)).unwrap();
    servers.keep_sign_in(id.into(), &to, as_first(granted), GIVEN_AT_MS).unwrap().server
}

/// Signing in to the server `id` again, by the sign-in `again`, with the access token `made-up-again`.
pub(super) fn again(servers: &ToolServers, id: &str) -> Result<ToolServerChange, ApiError> {
    let to = servers.signing_in(ToolSignInTo::Again { id: id.into() })?;
    servers.keep_sign_in("again".into(), &to, as_first(granted("made-up-again")), GIVEN_AT_MS)
}

/// What the record of the server `id` says of its sign-in.
pub(super) fn status_of(store: &Arc<Store>, id: &str) -> ToolSignInStatus {
    let (server, _) = store.tool_server(&id.into()).unwrap().unwrap();
    let ToolServerAuth::SignIn { status } = server.auth else { panic!("{server:?}") };
    status
}

/// The form is spelled out, since a build that reads it differently would take every sign-in for what isn't one.
const FILE: &str = concat!(
    r#"{"format":"sign_in","made_by":"t","renewable":{"server":"https://mcp.example.com/mcp","#,
    r#""resource":"https://mcp.example.com/mcp","issuer":"https://auth.example.com","#,
    r#""token_endpoint":"https://auth.example.com/token","client_id":"https://client.example/client.json","#,
    r#""scopes":"read"},"number":1,"tokens":{"access":"made-up-t","ends_at_ms":3601000,"refresh":"made-up-t-refresh"}}"#,
);

#[test]
fn a_sign_in_is_kept_in_a_file_of_its_own_alone_and_the_record_says_it_works() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());

    let server = signed_in(&servers, "t", " Linear (Northwind)\n");

    assert_eq!(server.name.as_str(), "Linear (Northwind)");
    assert_eq!(server.auth, ToolServerAuth::SignIn { status: ToolSignInStatus::Works });
    assert_eq!(store.list_records().unwrap().tool_servers, [server]);
    assert_eq!(files_in(dir.path()), [FILE]);
    assert!(matches!(ToolServerSecret::from_bytes(FILE.into()), Stored::Secret(ToolServerSecret::SignIn(_))));
    assert_eq!(holding(dir.path(), "made-up-t"), ["tool-servers"], "nothing else has it, the database least of all");
}

#[test]
fn what_cant_be_signed_in_to_is_refused_as_it_starts() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _store) = servers_in(dir.path());
    servers.add(with_key("key", "Linear", "made-up")).unwrap();
    servers.add(with_neither("neither", "Docs")).unwrap();
    let start = |to| refused(servers.signing_in(to));
    let to = |id: &str| ToolSignInTo::Again { id: id.into() };
    let url = |url: &str| ToolSignInTo::New { id: "n".into(), name: "New".into(), url: url.into() };

    assert_eq!(start(new("n", " ")), ErrorCode::InvalidRequest);
    assert_eq!(start(url("https://ada:made-up@mcp.example.com")), ErrorCode::InvalidRequest);
    assert_eq!(start(new("n", "linear")), ErrorCode::Conflict, "agents would call both the same");
    assert_eq!(start(new("key", "Another")), ErrorCode::Conflict, "its ID is another's");
    assert_eq!(start(to("gone")), ErrorCode::NotFound);
    assert_eq!(start(to("key")), ErrorCode::InvalidRequest);
    assert_eq!(start(to("neither")), ErrorCode::InvalidRequest);
    assert_eq!(start(ToolSignInTo::Unknown), ErrorCode::InvalidRequest);
}

#[test]
fn a_new_server_whose_name_or_id_was_taken_as_it_was_signed_in_to_is_refused_and_keeps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let name_taken = servers.signing_in(new("n", "Linear")).unwrap();
    let id_taken = servers.signing_in(new("taken", "Sentry")).unwrap();
    servers.add(with_neither("taken", "Linear")).unwrap();
    let before = store.list_records().unwrap();

    for to in [&name_taken, &id_taken] {
        let kept = servers.keep_sign_in("i".into(), to, as_first(granted("made-up")), GIVEN_AT_MS);

        assert_eq!(refused(kept), ErrorCode::Conflict);
    }
    assert_eq!(store.list_records().unwrap(), before);
    assert_eq!(files_in(dir.path()), Vec::<String>::new());
}

#[test]
fn a_server_signed_in_to_again_keeps_its_id_its_name_and_its_files_place() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let first = signed_in(&servers, "t", "Linear");
    let needs = servers.say(&first.id, ToolSignInStatus::NeedsSignIn).unwrap().server;
    let file = store.tool_server(&first.id).unwrap().unwrap().1;

    let change = again(&servers, "t").unwrap();

    assert!(change.changed && change.server.rev > needs.rev);
    let works = ToolServerAuth::SignIn { status: ToolSignInStatus::Works };
    assert_eq!(change.server, ToolServer { auth: works, rev: change.server.rev, ..needs });
    assert_eq!(store.tool_server(&first.id).unwrap().unwrap().1, file);
    let kept = files_in(dir.path()).remove(0);
    assert!(kept.contains(r#""made_by":"again""#) && kept.contains(r#""access":"made-up-again""#), "{kept}");
    assert!(kept.contains(r#""number":2"#), "each of its tokens has a higher number: {kept}");
    assert_eq!(holding(dir.path(), "made-up-t"), Vec::<String>::new(), "and those before are gone");
}

#[test]
fn a_server_removed_as_it_was_signed_in_to_again_stays_removed_and_so_does_one_added_under_its_id_since() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    let to = servers.signing_in(ToolSignInTo::Again { id: "t".into() }).unwrap();
    let keep = || refused(servers.keep_sign_in("again".into(), &to, as_first(granted("made-up-again")), GIVEN_AT_MS));

    servers.remove(&"t".into()).unwrap();
    let removed = keep();
    let since = signed_in(&servers, "t", "Docs");
    let added = keep();

    assert_eq!((removed, added), (ErrorCode::Conflict, ErrorCode::Conflict));
    assert_eq!(store.list_records().unwrap().tool_servers, [since]);
    assert_eq!(holding(dir.path(), "made-up-again"), Vec::<String>::new());
}

#[test]
fn the_server_a_sign_in_made_is_found_by_it_until_its_signed_in_to_again() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _store) = servers_in(dir.path());
    signed_in(&servers, "other", "Docs");
    servers.add(with_key("key", "Figma", "made-up")).unwrap();
    let first = signed_in(&servers, "t", "Linear");
    let by_first = servers.signed_in_by(&"t".into()).unwrap();
    let by_none = servers.signed_in_by(&"never".into()).unwrap();

    let second = again(&servers, "t").unwrap().server;

    assert_eq!((by_first, by_none), (Some(first), None));
    assert_eq!(servers.signed_in_by(&"t".into()).unwrap(), None);
    assert_eq!(servers.signed_in_by(&"again".into()).unwrap(), Some(second));
}

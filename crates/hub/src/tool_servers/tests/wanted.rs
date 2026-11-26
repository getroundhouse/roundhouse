//! What's kept of the access a server wants beyond its sign-in's, which here is asked for `read`, and what isn't.

use std::path::Path;

use protocol::client::ErrorCode;
use protocol::client::tool_sign_ins::ToolSignInTo;
use protocol::model::{ToolScopes, ToolSignInStatus};
use protocol::worker::{AskForToolAccess, ServerSignIn, TokenNumber, ToolAccess};
use tool_sign_in::{Issuer, OauthError, Renewal};

use super::super::{SigningIn, ToolServers, Wanted};
use super::sign_ins::{GIVEN_AT_MS, given, granted, signed_in, status_of};
use super::support::{files_in, holding, refused, servers_in, with_key};

const FIRST: TokenNumber = TokenNumber::FIRST;

fn scopes(text: &str) -> ToolScopes {
    ToolScopes::from(text.to_string())
}

fn ask(id: &str, refused: TokenNumber, wants: &str) -> AskForToolAccess {
    AskForToolAccess { server: id.into(), refused, wants: scopes(wants) }
}

/// What came of the server `t` wanting `wants` of its sign-in's token `refused`.
fn wants(servers: &ToolServers, refused: TokenNumber, wants: &str) -> Wanted {
    servers.wants_access(&ask("t", refused, wants)).unwrap()
}

fn starts(servers: &ToolServers) -> SigningIn {
    servers.signing_in(ToolSignInTo::Again { id: "t".into() }).unwrap()
}

/// What `to` asks to be allowed beyond what its server says, at the authorization server `issuer`.
fn asks(to: &SigningIn, issuer: &str) -> String {
    let issuer: Issuer = serde_json::from_value(issuer.into()).unwrap();
    to.asked_before(&issuer).to_string()
}

const ISSUER: &str = "https://auth.example.com";

/// Finishes `to`, which asks for what it's to.
fn finishes(servers: &ToolServers, to: &SigningIn) {
    let signed_in = given(&asks(to, ISSUER), ISSUER, granted("made-up-again"));
    servers.keep_sign_in("again".into(), to, signed_in, GIVEN_AT_MS).unwrap();
}

fn kept(dir: &Path) -> String {
    files_in(dir).remove(0)
}

const ASKED: Wanted = Wanted { access: ToolAccess::SignInAgain, change: None };
const IN_VAIN: Wanted = Wanted { access: ToolAccess::NotAskedFor, change: None };
const OTHER_TOKEN: Wanted = Wanted { access: ToolAccess::OtherToken, change: None };

/// Its tokens stay workers' to call with, since they're good for the rest.
#[test]
fn what_a_server_wants_is_kept_beside_its_sign_in_once_and_its_record_says_only_that_it_wants_more() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    let sent = servers.sent();

    let first = wants(&servers, FIRST, "made-up:write read");
    let records = store.list_records().unwrap();
    let again = wants(&servers, FIRST, "read made-up:write");

    let change = first.change.unwrap();
    assert!(change.changed && first.access == ToolAccess::SignInAgain);
    assert_eq!(records.tool_servers, [change.server]);
    assert_eq!(status_of(&store, "t"), ToolSignInStatus::NeedsMoreAccess);
    assert_eq!(holding(dir.path(), "made-up:write"), ["tool-servers"], "nothing clients are told of has it");
    assert!(kept(dir.path()).ends_with(r#""wants":"made-up:write read"}"#), "{}", kept(dir.path()));
    assert_eq!((again, store.list_records().unwrap()), (ASKED, records), "asked twice");
    let works = Some(ServerSignIn::Works { token: FIRST });
    assert_eq!(servers.sent_server(&"t".into()).unwrap().sign_in, works);
    assert!(!sent.has_changed().unwrap());
}

/// One that's never the server's, one the hub gave before, a key's, and a server it hasn't.
#[test]
fn nothing_is_kept_of_a_token_that_isnt_the_one_the_hub_has() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    servers.add(with_key("key", "Figma", "made-up")).unwrap();
    let before = (files_in(dir.path()), store.list_records().unwrap());

    let never = wants(&servers, FIRST.next(), "write");
    let of_a_key = servers.wants_access(&ask("key", FIRST, "write")).unwrap();
    let of_none = servers.wants_access(&ask("gone", FIRST, "write"));

    assert_eq!((never, of_a_key, refused(of_none)), (OTHER_TOKEN, OTHER_TOKEN, ErrorCode::NotFound));
    assert_eq!((files_in(dir.path()), store.list_records().unwrap()), before);
    finishes(&servers, &starts(&servers));
    assert_eq!(wants(&servers, FIRST, "write"), OTHER_TOKEN, "one it gave before");
    assert_eq!(status_of(&store, "t"), ToolSignInStatus::Works);
}

/// In one ask, or in as many as a server likes to make.
#[test]
fn no_more_is_kept_than_a_sign_in_could_ask_for_and_nothing_of_a_server_that_names_none() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    let of = |numbers: std::ops::Range<u32>| numbers.map(|n| format!("scope:{n}")).collect::<Vec<_>>().join(" ");
    let before = files_in(dir.path());

    for too_much in [of(0..33), "x".repeat(201), format!("write {}", "x".repeat(201)), String::new(), "\" \\".into()] {
        assert_eq!(wants(&servers, FIRST, &too_much), IN_VAIN, "{too_much}");
    }
    assert_eq!((files_in(dir.path()), status_of(&store, "t")), (before, ToolSignInStatus::Works));

    wants(&servers, FIRST, &of(0..31)).change.unwrap();
    wants(&servers, FIRST, &"x".repeat(200)).change.unwrap();
    let full = files_in(dir.path());
    assert_eq!(wants(&servers, FIRST, "another"), IN_VAIN);
    assert_eq!(wants(&servers, FIRST, &of(3..5)), ASKED, "what's kept still is");
    assert_eq!(files_in(dir.path()), full);
}

#[test]
fn signing_in_again_asks_for_what_it_had_and_what_is_wanted_and_only_at_its_authorization_server() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    wants(&servers, FIRST, "write").change.unwrap();
    wants(&servers, FIRST, "admin").change.unwrap();
    let to = starts(&servers);
    let another = "https://another.example.com";

    assert_eq!(asks(&to, ISSUER), "read write admin");
    assert_eq!(asks(&to, another), "");
    wants(&servers, FIRST, "late").change.unwrap();
    finishes(&servers, &to);

    assert_eq!(status_of(&store, "t"), ToolSignInStatus::NeedsMoreAccess, "what it wanted since is still to ask for");
    assert_eq!(asks(&starts(&servers), ISSUER), "read write admin late");
    finishes(&servers, &starts(&servers));
    assert_eq!(status_of(&store, "t"), ToolSignInStatus::Works);
    let third = FIRST.next().next();
    wants(&servers, third, "more").change.unwrap();
    let of_another = given("", another, granted("made-up-another"));
    servers.keep_sign_in("another".into(), &starts(&servers), of_another, GIVEN_AT_MS).unwrap();
    assert_eq!(status_of(&store, "t"), ToolSignInStatus::Works, "what one server wanted is nothing to another's");
}

/// Through renewals, after one that ends it too, since signing in again is what asks for it.
#[test]
fn what_is_wanted_is_kept_as_the_sign_in_is_renewed_and_once_its_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    wants(&servers, FIRST, "write").change.unwrap();
    let renew = |number, renewal| {
        let due = servers.refused(&"t".into(), number).unwrap().due.remove(0);
        servers.renewed(&due, renewal, GIVEN_AT_MS).unwrap().unwrap();
    };

    renew(FIRST, Renewal::Renewed(granted("made-up-renewed")));
    assert_eq!(status_of(&store, "t"), ToolSignInStatus::NeedsMoreAccess);
    renew(FIRST.next(), Renewal::SignInAgain(OauthError::new("invalid_grant")));

    assert_eq!(status_of(&store, "t"), ToolSignInStatus::NeedsSignIn);
    assert_eq!(asks(&starts(&servers), ISSUER), "read write");
}

/// The person may have left out what was asked for, or an account may have none of it to give.
#[test]
fn a_server_that_wants_what_its_sign_in_was_asked_for_has_it_asked_for_once_more_and_then_no_more() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    let mut number = FIRST;
    let mut sign_in = || {
        finishes(&servers, &starts(&servers));
        number = number.next();
        assert_eq!(status_of(&store, "t"), ToolSignInStatus::Works, "until a call is refused again");
        number
    };

    wants(&servers, FIRST, "write").change.unwrap();
    let second = sign_in();
    assert!(wants(&servers, second, "write").change.unwrap().changed, "it was asked for once");
    let third = sign_in();
    assert_eq!(wants(&servers, third, "write"), IN_VAIN);
    assert!(kept(dir.path()).ends_with(r#""asked_again":true}"#), "{}", kept(dir.path()));
    assert_eq!(status_of(&store, "t"), ToolSignInStatus::Works);
    let fourth = sign_in();
    assert_eq!(wants(&servers, fourth, "write"), IN_VAIN, "nor once the person signs in again themselves");

    wants(&servers, fourth, "admin").change.unwrap();
    assert_eq!(status_of(&store, "t"), ToolSignInStatus::NeedsMoreAccess, "what hasn't been asked for is");
    let fifth = sign_in();
    assert!(wants(&servers, fifth, "admin").change.unwrap().changed);
    assert_eq!(wants(&servers, sign_in(), "admin"), IN_VAIN);
}

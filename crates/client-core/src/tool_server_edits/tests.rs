use protocol::client::tool_servers::ToolServerAdd;
use protocol::fixtures;
use protocol::model::{
    ToolServer, ToolServerAuth, ToolServerAuthEdit, ToolServerAuthGiven, ToolServerHeader, ToolServerKey,
    ToolServerName, ToolServerUrl, ToolSignInStatus,
};

use super::*;

const URL: &str = "https://mcp.northwind.example/mcp";
const QUERIED: &str = "https://mcp.northwind.example/mcp?toolsets=all";
const ELSEWHERE: &str = "https://mcp.contoso.example/mcp";
/// What's typed of a URL partway, which isn't one yet.
const PARTWAY: &str = "https:/";
const KEY: &str = "a-made-up-key";

/// What's said under the way and the key.
const STAY: &str = "You stay signed in.";
const RENEWS: &str = "Your browser opens on its own page, and the hub renews the sign-in by itself.";
const ANOTHER_SIGN_IN: &str = "That's another server, so you sign in to it. Nothing changes until you have.";
const KEY_WAY: &str = "Sent in a header with every request, such as an API key. Only the hub keeps it.";
const GAVE: &str = "What the server gave you.";
const KEEP: &str = "Leave it empty to keep the one you gave.";
const ANOTHER_KEY: &str = "That's another server, so give it its key.";

/// Runbooks, at [`URL`], knowing the person by `auth`.
fn runbooks(auth: ToolServerAuth) -> ToolServer {
    let (name, url) = (ToolServerName::stored("Runbooks".into()), ToolServerUrl::stored(URL.into()));
    ToolServer { name, url, auth, ..fixtures::tool_server("runbooks") }
}

fn keyed() -> ToolServer {
    runbooks(ToolServerAuth::Key { header: ToolServerHeader::typed("Authorization").unwrap() })
}

fn signed_in(status: ToolSignInStatus) -> ToolServer {
    runbooks(ToolServerAuth::SignIn { status })
}

fn open() -> ToolServer {
    runbooks(ToolServerAuth::Neither)
}

fn key(header: &str, key: &str) -> TypedAuth {
    TypedAuth::Key { header: header.into(), key: ToolServerKey::new(key) }
}

/// The key a server has, as its edit opens: in its header, with none typed.
fn kept() -> TypedAuth {
    key("Authorization", "")
}

fn typed(name: &str, url: &str, auth: TypedAuth) -> Typed {
    Typed { name: name.into(), url: url.into(), auth }
}

/// What's said under the way and under the key of `was` edited to what's typed, what its button does, and whether it
/// has what that takes.
fn takes(was: &ToolServer, url: &str, auth: TypedAuth) -> (&'static str, &'static str, Does, bool) {
    let typed = typed("Runbooks", url, auth);
    let edit = typed.edit_of(was);
    let said = (way_says(typed.auth.way(), Some(&edit)), key_says(Some(&edit)));
    (said.0, said.1, typed.does(Some(was)), typed.is_complete(Some(was)))
}

/// A key kept, one for another server, and signing in: as it was, where it moves, and where it's another way.
#[test]
fn what_saving_an_edit_takes_is_said_under_its_way_and_its_key_and_by_its_button() {
    let (keyed, works, open) = (keyed(), signed_in(ToolSignInStatus::Works), open());
    let (more_access, gone) = (signed_in(ToolSignInStatus::NeedsMoreAccess), signed_in(ToolSignInStatus::NeedsSignIn));
    let sign_in = || TypedAuth::SignIn;
    let a_key = |header: &str| key(header, KEY);
    for (row, took, expected) in [
        ("signed in to", takes(&works, QUERIED, sign_in()), (STAY, GAVE, Does::Save, true)),
        ("wanting more access", takes(&more_access, URL, sign_in()), (STAY, GAVE, Does::Save, true)),
        ("needing a sign-in", takes(&gone, URL, sign_in()), (RENEWS, GAVE, Does::Save, true)),
        (
            "signed in to, moved",
            takes(&works, ELSEWHERE, sign_in()),
            (ANOTHER_SIGN_IN, ANOTHER_KEY, Does::SignIn, true),
        ),
        ("a sign-in for a key", takes(&keyed, URL, sign_in()), (SIGN_IN_TO_SAVE, GAVE, Does::SignIn, true)),
        ("a key kept", takes(&keyed, QUERIED, kept()), (KEY_WAY, KEEP, Does::Save, true)),
        ("a key kept, no header", takes(&keyed, URL, a_key(" ")), (KEY_WAY, KEEP, Does::Save, false)),
        ("no key, moved", takes(&keyed, ELSEWHERE, kept()), (KEY_WAY, ANOTHER_KEY, Does::Save, false)),
        ("a key, moved", takes(&keyed, ELSEWHERE, a_key("Authorization")), (KEY_WAY, ANOTHER_KEY, Does::Save, true)),
        ("a key for none", takes(&open, URL, kept()), (KEY_WAY, GAVE, Does::Save, false)),
    ] {
        assert_eq!(took, expected, "{row}");
    }
}

/// Partway, it's taken as the URL the server has, since the hub refuses it and so changes nothing of it, and it's sent
/// for the hub to say why.
#[test]
fn a_url_typed_partway_is_said_as_the_one_the_server_has_and_still_sent() {
    let (keyed, works) = (keyed(), signed_in(ToolSignInStatus::Works));

    assert_eq!(takes(&works, PARTWAY, TypedAuth::SignIn), (STAY, GAVE, Does::Save, true));
    assert_eq!(takes(&keyed, PARTWAY, kept()), (KEY_WAY, KEEP, Does::Save, true));
    assert_eq!(takes(&works, "", TypedAuth::SignIn), (STAY, GAVE, Does::Save, false), "nothing typed of it");
    let sent = typed("Runbooks", PARTWAY, kept()).edit_of(&keyed).into_sent();
    assert_eq!(sent, Sent { name: None, url: Some(PARTWAY.into()), auth: None });
}

#[test]
fn a_server_to_add_is_signed_in_to_or_added_with_whats_typed() {
    let id = "runbooks".into();
    let signing_in = typed("Linear", URL, TypedAuth::SignIn);
    let with_a_key = typed("Runbooks", URL, key("Authorization", KEY));

    assert_eq!((signing_in.does(None), signing_in.added(&id)), (Does::SignIn, None));
    assert_eq!((way_says(Way::SignIn, None), key_says(None)), (RENEWS, GAVE));
    assert_eq!(with_a_key.does(None), Does::Add);
    let auth = ToolServerAuthGiven::Key { header: "Authorization".into(), key: ToolServerKey::new(KEY) };
    let added = ToolServerAdd { id: id.clone(), name: "Runbooks".into(), url: URL.into(), auth };
    assert_eq!(with_a_key.added(&id), Some(added));
    assert!(with_a_key.is_complete(None));
    assert!(!typed("Runbooks", URL, kept()).is_complete(None), "a key it doesn't have");
    assert!(!typed("Docs", " ", TypedAuth::Neither).is_complete(None), "no URL");
    assert!(!typed(" ", URL, TypedAuth::Neither).is_complete(None), "no name");
}

/// So what another client changed meanwhile stays.
#[test]
fn an_edit_sends_only_what_changed_as_it_was_typed() {
    let sent = |was: ToolServer, typed: Typed| typed.edit_of(&was).into_sent();
    let nothing = Sent { name: None, url: None, auth: None };
    let key_for = |header: &str, key: Option<&str>, url: &str| {
        let key = key.map(ToolServerKey::new);
        Some(ToolServerAuthEdit::Key { header: header.into(), key, url: url.into() })
    };

    assert_eq!(sent(keyed(), typed(" Runbooks ", URL, kept())), nothing);
    let renamed = Sent { name: Some("Northwind ".into()), ..nothing.clone() };
    assert_eq!(sent(keyed(), typed("Northwind ", URL, kept())), renamed);
    let header = sent(keyed(), typed("Runbooks", QUERIED, key("X-Key", " ")));
    assert_eq!(header, Sent { url: Some(QUERIED.into()), auth: key_for("X-Key", None, QUERIED), ..nothing.clone() });
    let moved = sent(keyed(), typed("Runbooks", ELSEWHERE, key("Authorization", KEY)));
    assert_eq!(moved.auth, key_for("Authorization", Some(KEY), ELSEWHERE));
    assert_eq!(sent(keyed(), typed("Runbooks", URL, TypedAuth::Neither)).auth, Some(ToolServerAuthEdit::Neither));
    assert_eq!(sent(open(), typed("Runbooks", URL, TypedAuth::Neither)), nothing);
    assert_eq!(sent(keyed(), typed("Runbooks", URL, TypedAuth::SignIn)), nothing, "it's signed in to first");
}

/// As `docs/spec/settings.md` has it, for what's said while the hub saves it.
#[test]
fn a_server_is_asked_for_its_tools_unless_its_signed_in_to_or_its_name_alone_is_changed() {
    let (keyed, works) = (keyed(), signed_in(ToolSignInStatus::Works));
    for (row, typed, was, asked) in [
        ("added with a key", typed("Runbooks", URL, key("Authorization", KEY)), None, true),
        ("added with neither", typed("Runbooks", URL, TypedAuth::Neither), None, true),
        ("added by signing in", typed("Runbooks", URL, TypedAuth::SignIn), None, false),
        ("its name", typed("Northwind", URL, kept()), Some(&keyed), false),
        ("its query, signed in to", typed("Runbooks", QUERIED, TypedAuth::SignIn), Some(&works), false),
        ("its query, with its key", typed("Runbooks", QUERIED, kept()), Some(&keyed), true),
        ("its header", typed("Runbooks", URL, key("X-Key", "")), Some(&keyed), true),
        ("neither, for its key", typed("Runbooks", URL, TypedAuth::Neither), Some(&keyed), true),
    ] {
        assert_eq!(typed.is_asked(was), asked, "{row}");
    }
}

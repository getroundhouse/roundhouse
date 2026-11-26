use std::collections::HashMap;

use protocol::model::ToolScopes;
use url::Url;

use super::{NotARedirect, RedirectUri, Returned, SignedIn, Started, challenge_of};
use crate::secrets::Verifier;
use crate::tests::{CLIENT, REDIRECT, begun};
use crate::{Client, Failure, Found, Iss, Person, Script, StandIn, ToolSignIn};

/// A sign-in with `script`, to its end, and the stand-in it was with.
async fn signed_in_with(script: Script) -> (Result<SignedIn, Failure>, StandIn) {
    let (client, stand_in) = (ToolSignIn::new(CLIENT), StandIn::with(script).await);
    let Started { page, pending } = begun(&client, &stand_in).await;
    let query = stand_in.visit(&page);
    let finished = client.finish(&pending, Returned::of_query(&query)).await;
    (finished, stand_in)
}

#[tokio::test]
async fn the_page_asks_with_s256_a_state_the_server_the_scopes_and_the_client_document() {
    let (client, stand_in) = (ToolSignIn::new(CLIENT), StandIn::with(Script::default()).await);

    let Started { page, pending } = begun(&client, &stand_in).await;
    let again = begun(&client, &stand_in).await;

    let page = Url::parse(page.as_str()).unwrap();
    let asked: HashMap<String, String> = page.query_pairs().into_owned().collect();
    let server = stand_in.server();
    let expected = [
        ("tenant", "made-up"),
        ("response_type", "code"),
        ("client_id", CLIENT),
        ("redirect_uri", REDIRECT),
        ("state", pending.state.as_str()),
        ("code_challenge", &challenge_of(&pending.verifier)),
        ("code_challenge_method", "S256"),
        ("resource", server.as_str()),
        ("scope", "read write"),
    ];
    assert_eq!(asked, expected.map(|(name, value)| (name.to_string(), value.to_string())).into());
    assert_ne!(pending.state, again.pending.state);
    assert_ne!(pending.verifier, again.pending.verifier);
    assert!(pending.verifier.as_str().len() >= 43, "RFC 7636's shortest");
}

/// RFC 7636's own, from its appendix B.
#[test]
fn the_challenge_is_the_verifiers_hash() {
    let verifier = Verifier::new("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");

    assert_eq!(challenge_of(&verifier), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
}

#[tokio::test]
async fn a_server_that_leaves_what_to_allow_to_its_authorization_server_isnt_asked_for_any() {
    let authorization_scopes = Some(vec!["offline_access"]);
    let script = Script { challenge_scope: None, authorization_scopes, ..Script::default() };
    let (client, stand_in) = (ToolSignIn::new(CLIENT), StandIn::with(script).await);

    let Started { page, .. } = begun(&client, &stand_in).await;

    assert!(!page.as_str().contains("scope="), "{}", page.as_str());
}

#[tokio::test]
async fn asking_for_a_refresh_token_has_the_person_asked_to_consent() {
    let authorization_scopes = Some(vec!["offline_access"]);
    let client = ToolSignIn::new(CLIENT);
    let stand_in = StandIn::with(Script { authorization_scopes, ..Script::default() }).await;

    let Started { page, .. } = begun(&client, &stand_in).await;
    stand_in.change(|script| script.challenge_scope = None);
    let Found::SignIn(way) = client.find(&stand_in.server()).await.unwrap() else { panic!("it has none") };
    let redirect = RedirectUri::parse(REDIRECT).unwrap();
    let wanted_since = client.begin(*way, Client::Document, ToolScopes::of(["admin"]), redirect).unwrap().page;

    assert!(page.as_str().contains("scope=read+write+offline_access&prompt=consent"), "{}", page.as_str());
    assert!(wanted_since.as_str().contains("scope=admin+offline_access&prompt=consent"), "{}", wanted_since.as_str());
}

#[tokio::test]
async fn approving_gives_tokens_and_what_renews_them() {
    let (finished, stand_in) = signed_in_with(Script::default()).await;

    let SignedIn { renewable, granted, registration } = finished.unwrap();
    assert_eq!(registration, None);
    assert!(granted.refresh.is_some());
    assert_eq!(granted.lasts.unwrap().as_secs(), 3600);
    assert_eq!(renewable.issuer().as_str(), stand_in.issuer());
    assert_eq!(renewable.scopes(), &ToolScopes::from("read write".to_string()));
    let exchanges = stand_in.asked_for("POST /token");
    let [exchange] = &exchanges[..] else { panic!("{exchanges:?}") };
    assert_eq!(exchange.form("resource").as_deref(), Some(stand_in.server().as_str()));
    assert_eq!(exchange.form("client_id").as_deref(), Some(CLIENT));
    assert_eq!(exchange.form("redirect_uri").as_deref(), Some(REDIRECT));
    assert!(exchange.form("client_secret").is_none());
}

#[tokio::test]
async fn refusing_at_the_page_says_so() {
    let (finished, stand_in) = signed_in_with(Script { person: Person::Refuses, ..Script::default() }).await;

    assert!(matches!(finished, Err(Failure::RefusedByPerson)), "{finished:?}");
    assert!(stand_in.asked_for("POST /token").is_empty());
}

/// Each row of MCP's table, in "Authorization Response Validation", of a person who approves and one who refuses, who
/// isn't believed to have either until it's seen who says so.
#[tokio::test]
async fn what_comes_back_is_from_the_issuer_by_mcps_table() {
    let cases = [
        (Iss::Said, true),
        (Iss::Missing, false),
        (Iss::Another, false),
        (Iss::SaidUnannounced, true),
        (Iss::AnotherUnannounced, false),
        (Iss::Unsaid, true),
    ];
    for (iss, right) in cases {
        for person in [Person::Approves, Person::Refuses] {
            let (finished, stand_in) = signed_in_with(Script { iss, person, ..Script::default() }).await;

            let exchanged = !stand_in.asked_for("POST /token").is_empty();
            let expected = match (right, person) {
                (true, Person::Approves) => finished.is_ok() && exchanged,
                (true, Person::Refuses) => matches!(finished, Err(Failure::RefusedByPerson)) && !exchanged,
                (false, _) => matches!(finished, Err(Failure::WrongIssuer)) && !exchanged,
            };
            assert!(expected, "{iss:?}, {person:?}: {finished:?}");
        }
    }
}

/// A page on another site could send the browser to where the app listens, so it takes nothing from what isn't its own,
/// and goes on waiting.
#[tokio::test]
async fn what_comes_back_without_the_sign_ins_state_leaves_it_waiting() {
    let (client, stand_in) = (ToolSignIn::new(CLIENT), StandIn::with(Script::default()).await);
    let Started { page, pending } = begun(&client, &stand_in).await;
    let query = stand_in.visit(&page);

    let state = format!("state={}", pending.state.as_str());
    let others =
        [String::new(), query.replace(&state, "state=other"), format!("{query}&code=other"), query.replace(&state, "")];
    for other in others {
        let finished = client.finish(&pending, Returned::of_query(&other)).await;
        assert!(matches!(finished, Err(Failure::WrongState)), "{other}: {finished:?}");
    }
    assert!(stand_in.asked_for("POST /token").is_empty());

    assert!(client.finish(&pending, Returned::of_query(&query)).await.is_ok());
}

#[tokio::test]
async fn a_code_is_good_once() {
    let (client, stand_in) = (ToolSignIn::new(CLIENT), StandIn::with(Script::default()).await);
    let Started { page, pending } = begun(&client, &stand_in).await;
    let query = stand_in.visit(&page);

    client.finish(&pending, Returned::of_query(&query)).await.unwrap();
    let again = client.finish(&pending, Returned::of_query(&query)).await;

    let Err(Failure::NoTokens(error)) = again else { panic!("{again:?}") };
    assert_eq!(error.as_str(), "invalid_grant");
}

#[test]
fn the_person_is_sent_back_only_to_this_machines_own_address() {
    for here in [REDIRECT, "http://127.0.0.1/callback"] {
        assert_eq!(RedirectUri::parse(here).unwrap().as_str(), here);
    }
    let elsewhere = [
        "https://127.0.0.1/callback",
        "http://localhost/callback",
        "http://[::1]/callback",
        "http://10.0.0.1/callback",
        "http://example.com/callback",
        "http://ada@127.0.0.1/callback",
        "http://127.0.0.1/callback?a=b",
        "http://127.0.0.1/callback#a",
        "roundhouse://callback",
    ];
    for elsewhere in elsewhere {
        assert_eq!(RedirectUri::parse(elsewhere), Err(NotARedirect), "{elsewhere}");
    }
}

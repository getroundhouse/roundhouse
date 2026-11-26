use std::time::Duration;

use fetching::serving::NOWHERE;
use protocol::client::{ApiError, ErrorCode};
use serde_json::{Value, json};

use super::{Client, Registration, TakenAs};
use crate::tests::{CLIENT, REDIRECT, begun};
use crate::{
    Failure, Found, Issuer, NoWay, NotFetched, OauthError, RedirectUri, Refreshing, Registers, Renewal, Returned,
    Script, SignedIn, StandIn, ToolSignIn, Way,
};

/// Far longer than a stand-in takes to be asked, on a busy machine.
const ASKED_WITHIN: Duration = Duration::from_secs(10);

/// An authorization server that takes no client document, and registers any client that asks.
fn registers(registers: Registers) -> Script {
    Script { client_documents: false, registers, ..Script::default() }
}

/// How an authorization server says a client can say who it is.
type AuthMethods = Option<Vec<&'static str>>;

/// Why its way to sign in is none.
type Why = fn(Issuer) -> NoWay;

async fn found(stand_in: &StandIn) -> Found {
    ToolSignIn::new(CLIENT).find(&stand_in.server()).await.unwrap()
}

async fn way(stand_in: &StandIn) -> Way {
    let found = found(stand_in).await;
    let Found::SignIn(way) = found else { panic!("{found:?}") };
    *way
}

fn sent_back_to(redirect: &str) -> RedirectUri {
    RedirectUri::parse(redirect).unwrap()
}

/// What asking `stand_in` for a client comes to, with `before` from before.
async fn asked(stand_in: &StandIn, redirect: &str, before: Option<&Registration>) -> Result<TakenAs, Failure> {
    ToolSignIn::new(CLIENT).client_for(&way(stand_in).await, &sent_back_to(redirect), before).await
}

/// The registration that `taken` is.
fn registration(taken: Result<TakenAs, Failure>) -> Registration {
    let Ok(TakenAs::Client(Client::Registered(registration))) = taken else { panic!("{taken:?}") };
    registration
}

fn times_asked(stand_in: &StandIn) -> usize {
    stand_in.asked_for("POST /register").len()
}

#[tokio::test]
async fn the_document_comes_first_then_registering_with_no_secret_and_finding_out_asks_for_neither() {
    let (none, secret) = (Some(vec!["client_secret_basic", "none"]), Some(vec!["client_secret_basic"]));
    let (anyone, nobody) = (Registers::Anyone, Registers::Nobody);
    let cases: [(bool, Registers, AuthMethods, Result<bool, Why>); 9] = [
        (true, nobody.clone(), None, Ok(false)),
        (true, anyone.clone(), None, Ok(false)),
        (true, anyone.clone(), secret.clone(), Ok(false)),
        (false, anyone.clone(), None, Ok(true)),
        (false, anyone.clone(), none.clone(), Ok(true)),
        (false, anyone.clone(), secret.clone(), Err(NoWay::OnlyClientsWithSecrets)),
        (false, anyone, Some(vec![]), Err(NoWay::OnlyClientsWithSecrets)),
        (false, nobody.clone(), none, Err(NoWay::OnlyRegisteredClients)),
        (false, nobody, secret, Err(NoWay::OnlyRegisteredClients)),
    ];
    for (client_documents, registers, client_auth_methods, expected) in cases {
        let said = format!("{client_documents}, {registers:?}, {client_auth_methods:?}");
        let script = Script { client_documents, registers, client_auth_methods, ..Script::default() };
        let stand_in = StandIn::with(script).await;

        let found = found(&stand_in).await;
        assert_eq!(times_asked(&stand_in), 0, "{said}");

        match (found, expected) {
            (Found::SignIn(_), Ok(is_registered)) => {
                let taken = asked(&stand_in, REDIRECT, None).await;
                let Ok(TakenAs::Client(client)) = taken else { panic!("{said}: {taken:?}") };
                assert_eq!(client.registration().is_some(), is_registered, "{said}");
                assert_eq!(times_asked(&stand_in), usize::from(is_registered), "{said}");
            }
            (Found::NoWay(why), Err(expected)) => assert_eq!(why, expected(Issuer::new(stand_in.issuer())), "{said}"),
            (found, _) => panic!("{said}: {found:?}"),
        }
    }
    let no_s256 = StandIn::with(Script { s256: false, ..registers(Registers::Anyone) }).await;
    let found = found(&no_s256).await;
    assert!(matches!(found, Found::NoWay(NoWay::NoS256(_))), "{found:?}");
}

/// As the site publishes it, but for its own address and the redirect with no port, which none is sent back to.
#[tokio::test]
async fn what_registering_sends_is_the_client_documents_with_each_redirect_listened_at_and_the_one_in_use() {
    let site = concat!(env!("CARGO_MANIFEST_DIR"), "/../../site/oauth/client.json");
    let document: Value = serde_json::from_str(&std::fs::read_to_string(site).unwrap()).unwrap();
    let ports = [17867, 17868, 17869].map(|port| format!("http://127.0.0.1:{port}/callback"));
    let another = "http://127.0.0.1:4321/callback";

    for (redirect, redirects) in [(REDIRECT, ports.to_vec()), (another, [&ports[..], &[another.into()]].concat())] {
        let stand_in = StandIn::with(registers(Registers::Anyone)).await;

        let registration = registration(asked(&stand_in, redirect, None).await);

        let sent = stand_in.asked_for("POST /register").remove(0);
        let mut expected = document.clone();
        expected.as_object_mut().unwrap().remove("client_id");
        expected["redirect_uris"] = json!(redirects);
        assert_eq!(serde_json::from_str::<Value>(&sent.body).unwrap(), expected);
        let kept = serde_json::to_value(&registration).unwrap();
        let client_id = registration.client_id.as_str();
        assert_eq!(
            kept,
            json!({"client_id": client_id, "issuer": stand_in.issuer(), "redirect_uris": redirects}),
            "and nothing more of what was answered is kept"
        );
        assert_eq!(serde_json::from_value::<Registration>(kept).unwrap(), registration);
    }
}

#[tokio::test]
async fn a_registered_client_signs_in_and_renews_as_it_was_registered_with_no_secret() {
    let (client, stand_in) = (ToolSignIn::new(CLIENT), StandIn::with(registers(Registers::Anyone)).await);

    let started = begun(&client, &stand_in).await;
    let logged = format!("{started:?}");
    let registered = started.pending.registration().unwrap().clone();
    let id = registered.client_id.as_str();
    let query = stand_in.visit(&started.page);
    let finished = client.finish(&started.pending, Returned::of_query(&query)).await.unwrap();
    let logged = format!("{logged} {finished:?}");
    let SignedIn { renewable, granted, registration } = finished;
    let refresh = granted.refresh.unwrap();
    let renewal = client.renew(&renewable, &refresh).await.unwrap();
    stand_in.change(|script| script.refreshing = Refreshing::Refused("invalid_client"));
    let unknown = client.renew(&renewable, &refresh).await.unwrap();

    assert_eq!(registration.as_ref(), Some(&registered));
    assert!(started.page.as_str().contains(&format!("&client_id={id}&")), "{}", started.page.as_str());
    assert!(matches!(renewal, Renewal::Renewed(_)), "{renewal:?}");
    let Renewal::SignInAgain(said) = unknown else { panic!("{unknown:?}") };
    assert!(said.is_invalid_client(), "a registration it has forgotten is gone for good");
    let asked = stand_in.asked_for("POST /token");
    assert_eq!(asked.len(), 3);
    for asked in asked {
        assert_eq!(asked.form("client_id").as_deref(), Some(id));
        assert!(asked.form("client_secret").is_none() && !asked.headers.contains_key("authorization"));
    }
    for secret in ["made-up-secret", "made-up-registration-token"] {
        assert!(!logged.contains(secret), "{logged}");
    }
}

#[tokio::test]
async fn a_registration_is_used_again_at_its_own_issuer_for_a_redirect_it_has_and_never_over_the_document() {
    let stand_in = StandIn::with(registers(Registers::Anyone)).await;
    let before = registration(asked(&stand_in, REDIRECT, None).await);
    let other_issuer = Registration { issuer: Issuer::new("https://other.example"), ..before.clone() };

    let same = registration(asked(&stand_in, REDIRECT, Some(&before)).await);
    let other_port = registration(asked(&stand_in, "http://127.0.0.1:17869/callback", Some(&before)).await);
    assert_eq!((&same, &other_port, times_asked(&stand_in)), (&before, &before, 1));

    let not_registered = registration(asked(&stand_in, "http://127.0.0.1:4321/callback", Some(&before)).await);
    let elsewhere = registration(asked(&stand_in, REDIRECT, Some(&other_issuer)).await);
    assert_eq!(times_asked(&stand_in), 3);
    assert!(not_registered.client_id != before.client_id && elsewhere.client_id != before.client_id);

    stand_in.change(|script| script.client_documents = true);
    let document = asked(&stand_in, REDIRECT, Some(&before)).await;
    assert!(matches!(document, Ok(TakenAs::Client(Client::Document))), "{document:?}");
    assert_eq!(times_asked(&stand_in), 3);
}

#[tokio::test]
async fn a_client_thats_given_is_taken_only_with_an_id_that_can_be_one_and_without_a_need_for_a_secret() {
    let id = |id: &str| json!({"client_id": id});
    let with = |method: Value| json!({"client_id": "a", "token_endpoint_auth_method": method, "client_secret": "b"});
    let longest = "a".repeat(2048);
    for (status, body, expected) in [
        (201, id("a"), "a"),
        (200, id("a"), "a"),
        (201, with("none".into()), "a"),
        (201, with(Value::Null), "a"),
        (201, id(" ~A.b"), " ~A.b"),
        (201, id(&longest), &longest),
    ] {
        let stand_in = StandIn::with(registers(Registers::Says(status, body))).await;

        assert_eq!(registration(asked(&stand_in, REDIRECT, None).await).client_id.as_str(), expected);
    }

    let longer = "a".repeat(2049);
    let not_ids = ["", "  ", "a\nb", "a\u{7f}", "clé", &longer].map(id);
    for body in [vec![json!({}), json!([]), json!({"client_id": 7})], not_ids.to_vec()].concat() {
        let stand_in = StandIn::with(registers(Registers::Says(201, body.clone()))).await;

        let failure = asked(&stand_in, REDIRECT, None).await.unwrap_err();

        assert!(matches!(failure, Failure::Unreadable { .. }), "{body}: {failure:?}");
    }
    for method in ["client_secret_basic", "client_secret_post", "private_key_jwt", ""] {
        let stand_in = StandIn::with(registers(Registers::Says(201, with(method.into())))).await;

        let taken = asked(&stand_in, REDIRECT, None).await;

        let Ok(TakenAs::NoWay(why)) = taken else { panic!("{method}: {taken:?}") };
        assert_eq!(why, NoWay::OnlyClientsWithSecrets(Issuer::new(stand_in.issuer())), "{method}");
    }
}

/// Nothing of what it was answered is in what's said but one of OAuth's names.
#[tokio::test]
async fn a_refusal_to_register_says_so_with_oauths_name_for_why_and_what_may_pass_says_that() {
    let error = |error: &str| json!({"error": error, "error_description": "made up"});
    let refusals = [
        (400, error("invalid_redirect_uri"), Some("invalid_redirect_uri")),
        (401, error("invalid_token"), Some("invalid_token")),
        (403, json!({"message": "made up"}), None),
        (404, json!("made up"), None),
        (400, error("Made up, <b>whatever</b>"), Some("an error it didn't name")),
    ];
    for (status, body, named) in refusals {
        let stand_in = StandIn::with(registers(Registers::Says(status, body))).await;

        let taken = asked(&stand_in, REDIRECT, None).await;

        let Ok(TakenAs::NoWay(why)) = taken else { panic!("{status}: {taken:?}") };
        assert!(!format!("{why:?} {why}").contains("ade up"), "{why:?}");
        let NoWay::NotRegistered { issuer, error } = why else { panic!("{status}: {why:?}") };
        assert_eq!(issuer.as_str(), stand_in.issuer());
        assert_eq!(error.as_ref().map(OauthError::as_str), named, "{status}");
    }

    let long = "a".repeat(300 * 1024);
    let failures = [
        (Registers::Says(503, json!("made up")), ErrorCode::Unavailable),
        (Registers::Says(429, json!("made up")), ErrorCode::Unavailable),
        (Registers::At(NOWHERE.to_string()), ErrorCode::Unavailable),
        (Registers::Says(202, json!({"client_id": "made up"})), ErrorCode::InvalidRequest),
        (Registers::Says(307, json!({"client_id": "made up"})), ErrorCode::InvalidRequest),
        (Registers::Says(201, json!({"client_id": "a", "made up": long})), ErrorCode::InvalidRequest),
    ];
    for (registering, code) in failures {
        let stand_in = StandIn::with(registers(registering.clone())).await;

        let failure = asked(&stand_in, REDIRECT, None).await.unwrap_err();

        let said = ApiError::from(failure);
        assert_eq!(said.code, code, "{said:?}");
        assert!(said.message.contains("127.0.0.1") && !said.message.contains("made up"), "{said:?}");
        assert!(stand_in.asked_for("POST /moved").is_empty());
    }
}

/// As it's found out, before the person is told it can be signed in to.
#[tokio::test]
async fn a_place_to_register_that_isnt_fetched_from_is_refused_and_one_that_isnt_needed_is_nothing() {
    for elsewhere in ["file:///etc/passwd", "ftp://sign-in.example/register", "not an address"] {
        let stand_in = StandIn::with(registers(Registers::At(elsewhere.into()))).await;

        let failure = ToolSignIn::new(CLIENT).find(&stand_in.server()).await.unwrap_err();

        let refused =
            matches!(failure, Failure::NotFetched { why: NotFetched::NotHttps, .. } | Failure::Unreadable { .. });
        assert!(refused, "{elsewhere}: {failure:?}");
        stand_in.change(|script| script.client_documents = true);
        way(&stand_in).await;
    }
}

#[test]
fn a_client_the_authorization_server_doesnt_know_is_told_from_whatever_else_it_says() {
    let says = |failure: fn(OauthError) -> Failure, error| failure(OauthError::new(error)).is_invalid_client();

    for failure in [Failure::NoTokens as fn(_) -> _, Failure::RefusedAtPage] {
        assert!(says(failure, "invalid_client"));
        assert!(!says(failure, "invalid_grant") && !says(failure, "unauthorized_client"));
    }
    assert!(!Failure::RefusedByPerson.is_invalid_client());
}

/// Time stands still once it has been asked, and moves only by tokio's paused clock, so nothing is waited out.
#[tokio::test]
async fn an_authorization_server_that_never_answers_whether_it_registers_fails_at_the_time_limit() {
    let stand_in = StandIn::with(registers(Registers::Anyone)).await;
    stand_in.change(|script| script.answers_after = Some(crate::Gate::closed()));
    let way = way(&stand_in).await;
    let asking = tokio::spawn(async move {
        ToolSignIn::new(CLIENT).client_for(&way, &sent_back_to(REDIRECT), None).await.unwrap_err()
    });
    waiting::task::until(ASKED_WITHIN, "it wasn't asked to register", || times_asked(&stand_in) == 1).await;

    tokio::time::pause();
    let failure = tokio::time::timeout(Duration::from_mins(10), asking).await.expect("it was still waiting").unwrap();

    assert!(matches!(failure, Failure::TimedOut { .. }), "{failure:?}");
}

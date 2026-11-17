//! What the hub asks GitHub for an app of your own, and what each answer comes to, against the stand-in, which checks
//! each JWT against the public half of the key it made the app with, as GitHub does.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use protocol::client::ErrorCode;
use protocol::client::github_apps::{GithubAppManifestWrite, GithubManifestCode};
use protocol::model::{GithubAppId, GithubInstallationId, GithubUser, GithubUserId};

use super::super::stand_in::{Answer, MADE_UP_APP_KEY, Script, StandIn};
use super::super::{Github, GithubConfig};
use super::*;

const APP: GithubAppId = GithubAppId::new(9);

fn key() -> GithubAppKey {
    GithubAppKey::new(MADE_UP_APP_KEY)
}

async fn github(script: Script) -> (StandIn, Github) {
    let stand_in = StandIn::with(script).await;
    let github = Github::new(GithubConfig::at(stand_in.api()));
    (stand_in, github)
}

/// Once, since GitHub takes each code once, and as nobody, since nobody is yet.
#[tokio::test]
async fn making_an_app_gives_its_key_once_for_each_code_asked_as_nobody() {
    let made = Answer::app_made(9, "ada-roundhouse", (1, "ada"));
    let (stand_in, github) = github(Script { conversions: vec![("a-code", made)], ..Script::default() }).await;
    let code = GithubManifestCode::new("a-code");

    let app = github.make_app(&code).await.unwrap();
    let again = github.make_app(&code).await.unwrap_err();

    let owner = GithubUser { id: GithubUserId::new(1), name: "ada".into() };
    assert_eq!((app.id, app.slug.as_str(), &app.owner), (APP, "ada-roundhouse", &owner));
    assert_eq!(app.key.as_str(), MADE_UP_APP_KEY);
    assert_eq!(again.code, ErrorCode::InvalidRequest);
    let asked = stand_in.asked();
    assert!(asked[0].starts_with("post /app-manifests/a-code/conversions http/1.1\r\n"), "{}", asked[0]);
    assert!(!asked[0].contains("authorization:"), "{}", asked[0]);
}

#[tokio::test]
async fn the_apps_bot_is_whom_github_names_for_its_slug() {
    let bot = Answer::user(41, "ada-roundhouse[bot]", None);
    let (stand_in, github) = github(Script { named: vec![("ada-roundhouse[bot]", bot)], ..Script::default() }).await;

    let found = github.bot(&"ada-roundhouse".into()).await.unwrap();

    assert_eq!(found, GithubUser { id: GithubUserId::new(41), name: "ada-roundhouse[bot]".into() });
    assert!(stand_in.asked()[0].starts_with("get /users/ada-roundhouse%5bbot%5d http/1.1\r\n"));
}

/// The stand-in answers only a JWT signed with the key it made the app with, whose public half it checks it against.
#[tokio::test]
async fn asked_as_the_app_with_a_jwt_its_key_signed_github_lists_where_its_installed() {
    let listed = Answer::installations(&[(70, 2, "adalabs"), (71, 1, "ada")]);
    let (stand_in, github) = github(Script { installations: vec![listed], ..Script::default() }).await;

    let installations = github.installations(APP, &key()).await.unwrap();

    let on = |id, name: &str| GithubUser { id: GithubUserId::new(id), name: name.into() };
    let expected = [
        Installation { id: GithubInstallationId::new(70), on: on(2, "adalabs") },
        Installation { id: GithubInstallationId::new(71), on: on(1, "ada") },
    ];
    assert_eq!(installations, expected);
    let asked = stand_in.asked();
    assert!(asked[0].starts_with("get /app/installations?per_page=100&page=1 http/1.1\r\n"), "{}", asked[0]);
}

#[tokio::test]
async fn a_jwt_says_its_the_app_and_lasts_no_longer_than_github_takes() {
    let jwt = key().signed_as(APP, 1_800_000_000_000).unwrap();

    let claims = jwt.as_str().split('.').nth(1).unwrap();
    let claims: Claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).unwrap()).unwrap();

    assert_eq!(claims, Claims { iat: 1_799_999_940, exp: 1_800_000_540, iss: "9".to_string() });
}

#[tokio::test]
async fn a_key_that_isnt_one_cant_sign_and_an_installation_github_doesnt_know_is_gone() {
    let (_stand_in, github) = github(Script::default()).await;
    let not_a_key = GithubAppKey::new("-----BEGIN RSA PRIVATE KEY-----\nbWFkZSB1cA==\n-----END RSA PRIVATE KEY-----\n");

    let unsigned = github.installations(APP, &not_a_key).await.unwrap_err();
    let refused = github.installation_token(APP, &key(), GithubInstallationId::new(70)).await;

    assert_eq!(unsigned.code, ErrorCode::Conflict);
    assert!(matches!(refused, Ok(Minted::Gone)), "an installation GitHub doesn't know is gone: {refused:?}");
}

#[tokio::test]
async fn an_installations_token_ends_when_github_says() {
    let token = Answer::installation_token("ghs_made_up", "2030-01-01T00:00:00Z");
    let (stand_in, github) = github(Script { minted: vec![(70, vec![token])], ..Script::default() }).await;

    let minted = github.installation_token(APP, &key(), GithubInstallationId::new(70)).await.unwrap();

    let Minted::Token { token, ends_at_ms } = minted else { panic!("{minted:?}") };
    assert_eq!((token.as_str(), ends_at_ms), ("ghs_made_up", 1_893_456_000_000));
    assert!(stand_in.asked()[0].starts_with("post /app/installations/70/access_tokens http/1.1\r\n"));
}

#[test]
/// Spelled out whole, since each permission is the owner's to give, and a private app, which only its owner installs.
fn the_manifest_asks_for_what_agents_use_as_a_private_app_with_no_webhook() {
    let asked = |name: &str, redirect_url: &str| GithubAppManifestWrite {
        name: name.to_string(),
        redirect_url: redirect_url.to_string(),
    };

    let made = manifest(&asked("ada-roundhouse", "http://127.0.0.1:4711/made")).unwrap().manifest;
    let refused =
        [asked(" ", "http://127.0.0.1:1/"), asked(&"a".repeat(35), "http://127.0.0.1:1/"), asked("a", "made")]
            .map(|asked| manifest(&asked).unwrap_err().code);

    assert_eq!(
        (&made["name"], &made["redirect_url"]),
        (&"ada-roundhouse".into(), &"http://127.0.0.1:4711/made".into())
    );
    assert_eq!((&made["public"], &made["hook_attributes"]["active"]), (&false.into(), &false.into()));
    assert_eq!(made["default_events"], serde_json::json!([]));
    let permissions = serde_json::json!({
        "metadata": "read",
        "contents": "write",
        "pull_requests": "write",
        "issues": "write",
        "workflows": "write",
        "actions": "read",
        "checks": "read",
        "statuses": "read",
    });
    assert_eq!(made["default_permissions"], permissions);
    assert_eq!(refused, [ErrorCode::InvalidRequest; 3]);
}

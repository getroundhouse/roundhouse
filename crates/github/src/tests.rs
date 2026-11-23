use fetching::serving::NOWHERE;
use protocol::model::{GithubRepositoryName, GithubUser, GithubUserId};
use protocol::worker::GitCredential;

use super::*;
use crate::GithubProfile;

/// 2026-12-31 23:59:59 UTC, in milliseconds since the epoch.
const NEW_YEARS_EVE_MS: i64 = 1_798_761_599_000;

#[tokio::test]
async fn a_token_is_whom_github_says_and_ends_when_github_says() {
    let github = StandIn::start(vec![
        ("ends", Answer::user(7, "ada", Some("2026-12-31 23:59:59 UTC"))),
        ("lasts", Answer::user(7, "ada", None)),
    ])
    .await;
    let calls = Github::new(GithubConfig::at(github.api()));

    let ends = calls.token_owner(&GithubToken::new("ends")).await.unwrap();
    let lasts = calls.token_owner(&GithubToken::new("lasts")).await.unwrap();

    let (ada, profile) = (GithubUser { id: GithubUserId::new(7), name: "ada".into() }, GithubProfile::default());
    assert_eq!(ends, TokenOwner { user: ada.clone(), profile: profile.clone(), ends_at_ms: Some(NEW_YEARS_EVE_MS) });
    assert_eq!(lasts, TokenOwner { user: ada, profile, ends_at_ms: None });
    let asked = github.asked();
    assert!(asked.iter().all(|head| head.starts_with("get /user http/1.1\r\n")), "{asked:?}");
    assert!(asked[0].contains("authorization: bearer ends\r\n"), "{}", asked[0]);
}

/// As GitHub gives them: `null` for a user who hasn't set a name, or shows no email, and left out, as from a GitHub
/// that doesn't say. Blank is none too, and the rest is trimmed.
#[tokio::test]
async fn a_tokens_user_has_the_name_and_public_email_github_gives_and_none_for_one_it_doesnt() {
    let github = StandIn::start(vec![
        ("both", Answer::user_with_profile(7, "ada", Some("Ada Lovelace"), Some("ada@example.com"))),
        ("neither", Answer::user_with_profile(7, "ada", None, None)),
        ("blank", Answer::user_with_profile(7, "ada", Some("  "), Some(""))),
        ("padded", Answer::user_with_profile(7, "ada", Some(" Ada "), None)),
        ("unsaid", Answer::With { status: 200, headers: Vec::new(), body: r#"{"login":"ada","id":7}"#.to_string() }),
    ])
    .await;
    let calls = Github::new(GithubConfig::at(github.api()));

    let mut profiles = Vec::new();
    for token in ["both", "neither", "blank", "padded", "unsaid"] {
        profiles.push(calls.token_owner(&GithubToken::new(token)).await.unwrap().profile);
    }

    let profile = |name: Option<&str>, email: Option<&str>| GithubProfile {
        name: name.map(str::to_string),
        email: email.map(str::to_string),
    };
    let both = profile(Some("Ada Lovelace"), Some("ada@example.com"));
    let none = profile(None, None);
    assert_eq!(profiles, [both, none.clone(), none.clone(), profile(Some("Ada"), None), none]);
}

#[test]
fn when_a_token_ends_reads_as_github_writes_it_or_with_an_offset_and_anything_else_is_unread() {
    let read = ["2026-12-31 23:59:59 UTC", "2026-12-31 23:59:59 +0000", "2026-12-31 15:59:59 -0800", "next week"]
        .map(ends_at_ms);

    assert_eq!(read, [Some(NEW_YEARS_EVE_MS), Some(NEW_YEARS_EVE_MS), Some(NEW_YEARS_EVE_MS), None]);
}

/// A token GitHub doesn't know, one that isn't a user's, such as an app's, and a GitHub that limits the hub's calls,
/// fails, or answers with something else.
#[tokio::test]
async fn a_token_github_refuses_is_refused_and_a_github_that_cant_say_whose_it_is_is_unavailable() {
    let limited = vec![("x-ratelimit-remaining", "0")];
    let github = StandIn::start(vec![
        ("an-apps", Answer::status(403, Vec::new())),
        ("limited", Answer::status(403, limited)),
        ("too-often", Answer::status(429, vec![("retry-after", "60")])),
        ("down", Answer::status(503, Vec::new())),
        ("garbled", Answer::With { status: 200, headers: Vec::new(), body: "<html>".to_string() }),
    ])
    .await;
    let calls = Github::new(GithubConfig::at(github.api()));

    let mut codes = Vec::new();
    for token in ["revoked", "an-apps", "limited", "too-often", "down", "garbled"] {
        codes.push(calls.token_owner(&GithubToken::new(token)).await.unwrap_err().code);
    }

    let (invalid, unavailable) = (ErrorCode::InvalidRequest, ErrorCode::Unavailable);
    assert_eq!(codes, [invalid, invalid, unavailable, unavailable, unavailable, unavailable]);
}

#[tokio::test]
async fn a_github_that_cant_be_reached_is_unavailable() {
    let refused = Github::new(GithubConfig::at(NOWHERE.to_string())).token_owner(&GithubToken::new("a-token")).await;

    assert_eq!(refused.unwrap_err().code, ErrorCode::Unavailable);
}

/// Time stands still once GitHub has the request, and moves only by tokio's paused clock, so nothing is waited out.
#[tokio::test]
async fn a_github_that_never_answers_fails_the_call_at_its_time_limit() {
    let github = StandIn::start(vec![("a-token", Answer::Never)]).await;
    let calls = Github::new(GithubConfig::at(github.api()));
    let started = tokio::time::Instant::now();
    let call = tokio::spawn(async move { calls.token_owner(&GithubToken::new("a-token")).await });
    github.until_asked(1).await;

    tokio::time::pause();
    let ended = tokio::time::timeout(ASK_WITHIN * 2, call).await;

    let refused = ended.expect("the call was still waiting on GitHub").unwrap();
    assert_eq!(refused.unwrap_err().code, ErrorCode::Unavailable);
    assert!(started.elapsed() >= ASK_WITHIN, "it ended after {:?}, before its limit", started.elapsed());
}

fn ada(token: &str) -> GitCredential {
    GitCredential { username: "ada".into(), token: GithubToken::new(token), identity: None }
}

/// As GitHub answered for a repository a login could push to, one it couldn't, one that isn't there, and a token it
/// doesn't take, in November 2026, as `docs/reference/github.md` says.
#[tokio::test]
async fn a_push_meets_what_github_answers_git_as_it_asks_before_sending_anything() {
    let github = StandIn::with(Script {
        pushes: vec![
            ("ada/app", vec![Answer::takes_pushes()]),
            ("cli/cli", vec![Answer::git_says(403, "Permission to cli/cli.git denied to ada.")]),
            ("ada/revoked", vec![Answer::git_says(401, "Invalid username or token.")]),
        ],
        ..Script::default()
    })
    .await;
    let calls = Github::new(GithubConfig::at(github.api()));
    let reach = |repository: &'static str| {
        let calls = &calls;
        async move { calls.push_reach(&GithubRepositoryName::named(repository), &ada("a-token")).await.unwrap() }
    };

    let answers = [reach("ada/app").await, reach("cli/cli").await, reach("ada/gone").await, reach("ada/revoked").await];

    let refused = PushAnswer::Refused("Permission to cli/cli.git denied to ada.".to_string());
    assert_eq!(answers, [PushAnswer::Takes, refused, PushAnswer::NotFound, PushAnswer::TokenRefused]);
    let asked = github.asked();
    assert!(asked[0].starts_with("get /ada/app.git/info/refs?service=git-receive-pack http/1.1\r\n"), "{}", asked[0]);
    // `ada:a-token`, as git's credential helper gives them.
    assert!(asked[0].contains("authorization: basic ywrhometdg9rzw4=\r\n"), "{}", asked[0]);
}

/// A network that answers with a page of its own, GitHub limiting how often it's asked, failing, or saying nothing.
#[tokio::test]
async fn a_push_github_doesnt_answer_for_is_unavailable_and_a_refusal_without_a_reason_says_so() {
    let page = Answer::With { status: 200, headers: vec![("content-type", "text/html")], body: "<html>".into() };
    let github = StandIn::with(Script {
        pushes: vec![
            ("a/portal", vec![page]),
            ("a/limited", vec![Answer::git_says(429, "Too many requests.")]),
            ("a/down", vec![Answer::git_says(503, "Unavailable.")]),
            ("a/quiet", vec![Answer::git_says(403, " \n ")]),
        ],
        ..Script::default()
    })
    .await;
    let calls = Github::new(GithubConfig::at(github.api()));

    let mut codes = Vec::new();
    for repository in ["a/portal", "a/limited", "a/down"] {
        let answer = calls.push_reach(&GithubRepositoryName::named(repository), &ada("a-token")).await;
        codes.push(answer.unwrap_err().code);
    }
    let quiet = calls.push_reach(&GithubRepositoryName::named("a/quiet"), &ada("a-token")).await.unwrap();

    assert_eq!(codes, [ErrorCode::Unavailable; 3]);
    assert_eq!(quiet, PushAnswer::Refused("GitHub refused it, and said nothing more".to_string()));
}

/// Time stands still once GitHub has the request, and moves only by tokio's paused clock, so nothing is waited out.
#[tokio::test]
async fn a_github_that_never_answers_a_push_fails_the_check_at_its_time_limit() {
    let github = StandIn::with(Script { pushes: vec![("a/app", vec![Answer::Never])], ..Script::default() }).await;
    let calls = Github::new(GithubConfig::at(github.api()));
    let started = tokio::time::Instant::now();
    let check =
        tokio::spawn(async move { calls.push_reach(&GithubRepositoryName::named("a/app"), &ada("a-token")).await });
    github.until_asked(1).await;

    tokio::time::pause();
    let ended = tokio::time::timeout(ASK_WITHIN * 2, check).await;

    let refused = ended.expect("the check was still waiting on GitHub").unwrap();
    assert_eq!(refused.unwrap_err().code, ErrorCode::Unavailable);
    assert!(started.elapsed() >= ASK_WITHIN, "it ended after {:?}, before its limit", started.elapsed());
}

/// The page a person grants an organization access to the app on is the app's own, by its client ID, on GitHub itself
/// or wherever a test's stand-in serves the device flow.
#[test]
fn the_app_s_page_is_its_connection_on_whichever_github_signs_people_in() {
    let own = GithubConfig::from_env(|_| None);
    let stand_in = GithubConfig::at("http://127.0.0.1:9".to_string());

    assert_eq!(own.app_page(), "https://github.com/settings/connections/applications/Ov23liWtqywYN3M2m6EU");
    assert_eq!(stand_in.app_page(), "http://127.0.0.1:9/settings/connections/applications/a-made-up-client-id");
}

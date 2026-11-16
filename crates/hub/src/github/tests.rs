use protocol::model::{GithubUser, GithubUserId};

use super::*;

/// 2026-12-31 23:59:59 UTC, in milliseconds since the epoch.
const NEW_YEARS_EVE_MS: i64 = 1_798_761_599_000;

#[tokio::test]
async fn a_token_is_whom_github_says_and_ends_when_github_says() {
    let github = StandIn::start(vec![
        ("ends", Answer::user(7, "ada", Some("2026-12-31 23:59:59 UTC"))),
        ("lasts", Answer::user(7, "ada", None)),
    ])
    .await;
    let calls = Github::new(github.api());

    let ends = calls.token_owner(&GithubToken::new("ends")).await.unwrap();
    let lasts = calls.token_owner(&GithubToken::new("lasts")).await.unwrap();

    let ada = GithubUser { id: GithubUserId::new(7), name: "ada".into() };
    assert_eq!(ends, TokenOwner { user: ada.clone(), ends_at_ms: Some(NEW_YEARS_EVE_MS) });
    assert_eq!(lasts, TokenOwner { user: ada, ends_at_ms: None });
    let asked = github.asked();
    assert!(asked.iter().all(|head| head.starts_with("get /user http/1.1\r\n")), "{asked:?}");
    assert!(asked[0].contains("authorization: bearer ends\r\n"), "{}", asked[0]);
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
    let calls = Github::new(github.api());

    let mut codes = Vec::new();
    for token in ["revoked", "an-apps", "limited", "too-often", "down", "garbled"] {
        codes.push(calls.token_owner(&GithubToken::new(token)).await.unwrap_err().code);
    }

    let (invalid, unavailable) = (ErrorCode::InvalidRequest, ErrorCode::Unavailable);
    assert_eq!(codes, [invalid, invalid, unavailable, unavailable, unavailable, unavailable]);
}

#[tokio::test]
async fn a_github_that_cant_be_reached_is_unavailable() {
    let nowhere = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", nowhere.local_addr().unwrap());
    drop(nowhere);

    let refused = Github::new(api).token_owner(&GithubToken::new("a-token")).await;

    assert_eq!(refused.unwrap_err().code, ErrorCode::Unavailable);
}

/// Time stands still once GitHub has the request, and moves only by tokio's paused clock, so nothing is waited out.
#[tokio::test]
async fn a_github_that_never_answers_fails_the_call_at_its_time_limit() {
    let github = StandIn::start(vec![("a-token", Answer::Never)]).await;
    let calls = Github::new(github.api());
    let started = tokio::time::Instant::now();
    let call = tokio::spawn(async move { calls.token_owner(&GithubToken::new("a-token")).await });
    github.until_asked(1).await;

    tokio::time::pause();
    let ended = tokio::time::timeout(ASK_WITHIN * 2, call).await;

    let refused = ended.expect("the call was still waiting on GitHub").unwrap();
    assert_eq!(refused.unwrap_err().code, ErrorCode::Unavailable);
    assert!(started.elapsed() >= ASK_WITHIN, "it ended after {:?}, before its limit", started.elapsed());
}

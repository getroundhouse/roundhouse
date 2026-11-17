//! What GitHub is asked, and what each of its answers comes to, against the stand-in, one call at a time.

use std::time::Duration;

use fetching::serving::NOWHERE;
use serde_json::json;
use tokio::time::Instant;

use super::*;
use crate::github::Github;

fn calling(github: &StandIn) -> Github {
    Github::new(GithubConfig::at(github.api()))
}

/// What was sent as the body of the `n`th request the stand-in was sent.
fn sent(github: &StandIn, n: usize) -> serde_json::Value {
    let asked = github.asked();
    serde_json::from_str(asked[n].split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[tokio::test]
async fn a_code_is_asked_for_through_the_app_for_every_scope_and_says_what_to_type_where_and_until_when() {
    let codes = vec![Answer::code("a-device-code", 900, 7)];
    let github = StandIn::with(Script { codes, ..Script::default() }).await;
    let asked_at = Instant::now();

    let code = calling(&github).device_code().await.unwrap();

    let lasts = Duration::from_mins(15);
    assert_eq!((code.user_code.as_str(), code.address.as_str()), ("WDJB-MJHT", "https://github.com/login/device"));
    assert_eq!((code.device.as_str(), code.lasts, code.interval), ("a-device-code", lasts, Duration::from_secs(7)));
    assert!((asked_at + lasts..=Instant::now() + lasts).contains(&code.ends), "it ends 900 s from when it came");
    let head = &github.asked()[0];
    assert!(head.starts_with("post /login/device/code http/1.1\r\n"), "{head}");
    assert!(head.contains("accept: application/json\r\n"), "{head}");
    assert_eq!(sent(&github, 0), json!({"client_id": "a-made-up-client-id", "scope": SCOPES}));
    assert!(!format!("{code:?}").contains("a-device-code"), "{code:?}");
}

/// One that says no interval, and one that says no time between asks, which is kept as GitHub said it, since asking
/// keeps to the hub's least, as `pacing` tests.
#[tokio::test]
async fn a_code_without_an_interval_is_asked_after_every_five_seconds_and_one_of_none_as_it_says() {
    let code = |interval: Option<u64>| {
        let mut body = json!({"device_code": "d", "user_code": "U", "verification_uri": "v", "expires_in": 900});
        if let Some(interval) = interval {
            body["interval"] = interval.into();
        }
        Answer::With { status: 200, headers: Vec::new(), body: body.to_string() }
    };
    let github = StandIn::with(Script { codes: vec![code(None), code(Some(0))], ..Script::default() }).await;
    let calls = calling(&github);

    let unsaid = calls.device_code().await.unwrap();
    let none = calls.device_code().await.unwrap();

    assert_eq!((unsaid.interval, none.interval), (Duration::from_secs(5), Duration::ZERO));
}

/// GitHub failing, answering with what isn't a code or with an error, a GitHub that can't be reached, and one that
/// never answers, whose limit is waited out on tokio's paused clock once the stand-in has the request.
#[tokio::test]
async fn a_code_github_wont_give_is_unavailable() {
    let codes = vec![
        Answer::status(503, Vec::new()),
        Answer::With { status: 200, headers: Vec::new(), body: "<html>".to_string() },
        Answer::device_flow_error("device_flow_disabled"),
        Answer::Never,
    ];
    let github = StandIn::with(Script { codes, ..Script::default() }).await;
    let calls = calling(&github);
    let unreachable = Github::new(GithubConfig::at(NOWHERE.to_string()));

    let mut codes = Vec::new();
    for _ in 0..3 {
        codes.push(calls.device_code().await.unwrap_err().code);
    }
    codes.push(unreachable.device_code().await.unwrap_err().code);
    let (never, ()) = tokio::join!(calls.device_code(), async {
        github.until_asked(4).await;
        tokio::time::pause();
    });
    codes.push(never.unwrap_err().code);

    assert_eq!(codes, [ErrorCode::Unavailable; 5]);
}

/// One whose token ends, and comes with what renews it, and one whose token doesn't, as an app whose tokens don't end
/// grants.
#[tokio::test]
async fn a_granted_token_is_asked_for_with_the_code_through_the_app_and_comes_with_when_it_ends_and_what_renews_it() {
    let lasting = json!({"access_token": "a-lasting-token", "token_type": "bearer", "scope": "repo"}).to_string();
    let lasting = Answer::With { status: 200, headers: Vec::new(), body: lasting };
    let github = StandIn::with(Script {
        codes: vec![Answer::code("a-device-code", 900, 5), Answer::code("lasting", 900, 5)],
        polls: vec![("a-device-code", vec![Answer::granted("a-token", "a-refresh-token")]), ("lasting", vec![lasting])],
        ..Script::default()
    })
    .await;
    let calls = calling(&github);
    let code = calls.device_code().await.unwrap();
    let lasting_code = calls.device_code().await.unwrap();

    let ending = calls.ask_after_token(&code).await.unwrap();
    let lasting = calls.ask_after_token(&lasting_code).await.unwrap();

    let Asked::Done(Approval::Granted(granted)) = ending else { panic!("{ending:?}") };
    assert_eq!((granted.token.as_str(), granted.lasts), ("a-token", Some(Duration::from_hours(8))));
    let (refresh, refresh_lasts) = granted.refresh.unwrap();
    assert_eq!((refresh.as_str(), refresh_lasts), ("a-refresh-token", Some(Duration::from_hours(181 * 24))));
    let Asked::Done(Approval::Granted(lasting)) = lasting else { panic!("{lasting:?}") };
    assert_eq!((lasting.token.as_str(), lasting.lasts, lasting.refresh.is_none()), ("a-lasting-token", None, true));
    let head = &github.asked()[2];
    assert!(head.starts_with("post /login/oauth/access_token http/1.1\r\n"), "{head}");
    assert!(head.contains("accept: application/json\r\n"), "{head}");
    let asked = json!({
        "client_id": "a-made-up-client-id",
        "device_code": "a-device-code",
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
    });
    assert_eq!(sent(&github, 2), asked);
}

/// Each answer GitHub gives before a token, by a code of its own, and answers that aren't GitHub's.
#[tokio::test]
async fn an_answer_without_a_token_says_to_ask_again_less_often_or_how_the_sign_in_ended() {
    let answers = [
        ("pending", Answer::device_flow_error("authorization_pending")),
        ("slower", Answer::slow_down(Some(10))),
        ("slower-by", Answer::slow_down(None)),
        ("refused", Answer::device_flow_error("access_denied")),
        ("ended", Answer::device_flow_error("expired_token")),
        ("disabled", Answer::device_flow_error("device_flow_disabled")),
        ("failing", Answer::status(503, Vec::new())),
        ("garbled", Answer::With { status: 200, headers: Vec::new(), body: "<html>".to_string() }),
    ];
    let codes = answers.iter().map(|(device, _)| Answer::code(device, 900, 5)).collect();
    let polls = answers.iter().map(|(device, answer)| (*device, vec![answer.clone()])).collect();
    let github = StandIn::with(Script { codes, polls, ..Script::default() }).await;
    let calls = calling(&github);

    let mut asked = Vec::new();
    for _ in &answers {
        let code = calls.device_code().await.unwrap();
        asked.push(calls.ask_after_token(&code).await);
    }

    assert!(matches!(asked[0], Ok(Asked::Again)), "{:?}", asked[0]);
    let ten = Some(Duration::from_secs(10));
    assert!(matches!(asked[1], Ok(Asked::SlowDown { interval }) if interval == ten), "{:?}", asked[1]);
    assert!(matches!(asked[2], Ok(Asked::SlowDown { interval: None })), "{:?}", asked[2]);
    assert!(matches!(asked[3], Ok(Asked::Done(Approval::Refused))), "{:?}", asked[3]);
    assert!(matches!(asked[4], Ok(Asked::Done(Approval::Ended))), "{:?}", asked[4]);
    let Ok(Asked::Done(Approval::Failed(message))) = &asked[5] else { panic!("{:?}", asked[5]) };
    assert!(message.contains("device_flow_disabled"), "{message}");
    let unavailable = asked[6..].iter().map(|asked| asked.as_ref().map(|_| ()).map_err(|e| e.code)).collect::<Vec<_>>();
    assert_eq!(unavailable, [Err(ErrorCode::Unavailable); 2]);
}

#[tokio::test]
async fn a_renewal_is_asked_with_what_renews_the_token_through_the_app_and_comes_with_both_anew() {
    let renewed = Answer::granted("a-renewed-token", "a-renewed-refresh-token");
    let github =
        StandIn::with(Script { renewals: vec![("a-refresh-token", vec![renewed])], ..Script::default() }).await;

    let renewal = calling(&github).renew(&GithubRefreshToken::new("a-refresh-token")).await.unwrap();

    let Renewal::Renewed(granted) = renewal else { panic!("{renewal:?}") };
    assert_eq!((granted.token.as_str(), granted.lasts), ("a-renewed-token", Some(Duration::from_hours(8))));
    let (refresh, refresh_lasts) = granted.refresh.unwrap();
    assert_eq!((refresh.as_str(), refresh_lasts), ("a-renewed-refresh-token", Some(Duration::from_hours(181 * 24))));
    let head = &github.asked()[0];
    assert!(head.starts_with("post /login/oauth/access_token http/1.1\r\n"), "{head}");
    assert!(head.contains("accept: application/json\r\n"), "{head}");
    let asked =
        json!({"client_id": "a-made-up-client-id", "grant_type": "refresh_token", "refresh_token": "a-refresh-token"});
    assert_eq!(sent(&github, 0), asked);
}

/// One GitHub doesn't know, as it answers one that has ended, been revoked or been used already, and then each answer
/// that may pass by the next try: a limit on how often it's asked, a failure, what isn't a token or an error, an error
/// that says nothing of what renews the token, a GitHub that can't be reached, and one that never answers, whose limit
/// is waited out on tokio's paused clock once the stand-in has the request.
#[tokio::test]
async fn what_renews_a_token_that_github_refuses_is_refused_and_anything_else_is_unavailable() {
    let renewals = vec![
        ("limited", vec![Answer::status(429, vec![("retry-after", "60")])]),
        ("failing", vec![Answer::status(503, Vec::new())]),
        ("garbled", vec![Answer::With { status: 200, headers: Vec::new(), body: "<html>".to_string() }]),
        ("misconfigured", vec![Answer::device_flow_error("incorrect_client_credentials")]),
        ("never", vec![Answer::Never]),
    ];
    let github = StandIn::with(Script { renewals, ..Script::default() }).await;
    let calls = calling(&github);
    let unreachable = Github::new(GithubConfig::at(NOWHERE.to_string()));
    let renewing = |refresh: &str| GithubRefreshToken::new(refresh);

    let refused = calls.renew(&renewing("used-already")).await;
    let mut codes = Vec::new();
    for refresh in ["limited", "failing", "garbled", "misconfigured"] {
        codes.push(calls.renew(&renewing(refresh)).await.unwrap_err().code);
    }
    codes.push(unreachable.renew(&renewing("a-refresh-token")).await.unwrap_err().code);
    let never = renewing("never");
    let (never, ()) = tokio::join!(calls.renew(&never), async {
        github.until_asked(6).await;
        tokio::time::pause();
    });
    codes.push(never.unwrap_err().code);

    let Ok(Renewal::Refused(message)) = &refused else { panic!("{refused:?}") };
    assert!(message.contains("bad_refresh_token"), "{message}");
    assert_eq!(codes, [ErrorCode::Unavailable; 6]);
}

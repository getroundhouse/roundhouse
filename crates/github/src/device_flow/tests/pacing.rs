//! How often GitHub is asked after a code's token, and for how long, on tokio's paused clock, with each answer as the
//! test says.

use std::collections::VecDeque;
use std::time::Duration;

use protocol::client::ApiError;
use protocol::model::GithubToken;
use tokio::time::Instant;

use super::*;

/// The least a caller asks after a code's token, a second, as the hub's `Timers::DEFAULT.sign_in_pace` is, which on
/// tokio's paused clock no test waits out.
const LEAST: Duration = Duration::from_secs(1);

/// What a scripted ask comes to.
enum Then {
    Answers(Asked),
    Fails,
    NeverAnswers,
}

fn granted() -> Then {
    Then::Answers(Asked::Done(Approval::Granted(Granted { token: GithubToken::new("t"), lasts: None, refresh: None })))
}

fn slow_down(seconds: Option<u64>) -> Then {
    Then::Answers(Asked::SlowDown { interval: seconds.map(Duration::from_secs) })
}

/// Asks after a code that ends `ends_in` seconds from now, first `interval` seconds apart, answered with `script` in
/// turn, and says how it ended, how many seconds after it began each ask was, and when it ended.
async fn paced_by(interval: u64, ends_in: u64, script: Vec<Then>) -> (Approval, Vec<u64>, u64) {
    let started = Instant::now();
    let mut script = VecDeque::from(script);
    let mut asked = Vec::new();
    let ends = started + Duration::from_secs(ends_in);
    let approval = paced(Duration::from_secs(interval), LEAST, ends, || {
        asked.push(started.elapsed().as_secs());
        let then = script.pop_front().expect("it asked more often than the test says GitHub answers");
        async move {
            match then {
                Then::Answers(asked) => Ok(asked),
                Then::Fails => Err(ApiError::new(ErrorCode::Unavailable, "made up")),
                Then::NeverAnswers => std::future::pending().await,
            }
        }
    })
    .await;
    (approval, asked, started.elapsed().as_secs())
}

#[tokio::test(start_paused = true)]
async fn it_waits_as_long_as_github_says_before_each_ask_and_ends_with_what_github_grants() {
    let (approval, asked, ended) =
        paced_by(5, 900, vec![Then::Answers(Asked::Again), Then::Answers(Asked::Again), granted()]).await;

    assert!(matches!(approval, Approval::Granted(_)), "{approval:?}");
    assert_eq!((asked, ended), (vec![5, 10, 15], 15));
}

/// As long as GitHub says, five seconds longer when it says nothing of how long, and a second at the least.
#[tokio::test(start_paused = true)]
async fn slow_down_has_it_ask_as_seldom_as_github_says_or_five_seconds_less_often() {
    let script = vec![slow_down(Some(10)), slow_down(None), Then::Answers(Asked::Again), slow_down(Some(0)), granted()];

    let (approval, asked, _) = paced_by(5, 900, script).await;

    assert!(matches!(approval, Approval::Granted(_)), "{approval:?}");
    assert_eq!(asked, [5, 15, 30, 45, 46]);
}

/// A code that says no time between asks, which GitHub's don't, is asked after [`LEAST`] apart, the least it's given.
#[tokio::test(start_paused = true)]
async fn a_code_that_says_no_time_between_asks_is_asked_after_no_sooner_than_the_least() {
    let (approval, asked, _) =
        paced_by(0, 900, vec![Then::Answers(Asked::Again), Then::Answers(Asked::Again), granted()]).await;

    assert!(matches!(approval, Approval::Granted(_)), "{approval:?}");
    assert_eq!(asked, [1, 2, 3]);
}

/// Asked at 5 s and 10 s, and not at 15 s, which is past its end at 12 s, so it ends at 10 s.
#[tokio::test(start_paused = true)]
async fn a_code_github_never_says_has_ended_ends_by_the_clock() {
    let (approval, asked, ended) =
        paced_by(5, 12, vec![Then::Answers(Asked::Again), Then::Answers(Asked::Again)]).await;

    assert!(matches!(approval, Approval::Ended), "{approval:?}");
    assert_eq!((asked, ended), (vec![5, 10], 10));
}

/// Failing at 5 s, then never answering at 10 s, 30 s and 50 s, each until its limit 15 s on, past which a code that
/// ends at 60 s has no time for another.
#[tokio::test(start_paused = true)]
async fn a_github_that_fails_or_never_answers_is_asked_again_until_the_code_ends() {
    let script = vec![Then::Fails, Then::NeverAnswers, Then::NeverAnswers, Then::NeverAnswers];

    let (approval, asked, ended) = paced_by(5, 60, script).await;

    assert!(matches!(approval, Approval::Ended), "{approval:?}");
    assert_eq!(ASK_WITHIN, Duration::from_secs(15));
    assert_eq!((asked, ended), (vec![5, 10, 30, 50], 65));
}

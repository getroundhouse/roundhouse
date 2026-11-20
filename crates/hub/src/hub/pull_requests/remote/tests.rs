use super::*;
use crate::hub::tests::{hub_reading_github, hub_without_account};
use crate::timers::Timers;
use github::{Answer, Script, StandIn};
use protocol::client::github_accounts::GithubAccountConnect;
use protocol::fixtures::{observed_pull_request, pull_request_wait};
use protocol::model::{GithubToken, PullRequestNumber, Wait};

fn wait(id: &str, numbers: &[u64]) -> Wait {
    let mut wait = pull_request_wait(id);
    let watch = wait.watch.as_mut().unwrap();
    watch.requested = PullRequestSelection::try_from(
        numbers.iter().map(|n| PullRequestNumber::try_from(*n).unwrap()).collect::<Vec<_>>(),
    )
    .unwrap();
    watch.snapshot.pull_requests = numbers.iter().map(|n| observed_pull_request(*n)).collect();
    wait
}

/// `c` joins `a` and `b` through what each shares with it, and `d` shares nothing, so it's asked apart, as is `e`, in
/// another workspace.
#[test]
fn overlapping_scopes_coalesce_transitively_and_unrelated_ones_stay_apart() {
    let elsewhere = Wait { workspace_id: "v".into(), ..wait("e", &[1]) };
    let mut waited = waited(vec![wait("a", &[1, 2]), wait("b", &[3]), wait("c", &[2, 3]), wait("d", &[9]), elsewhere]);

    let numbers = |waited: &Waited| waited.selection.numbers().iter().map(|n| n.get()).collect::<Vec<_>>();
    let mut ws: Vec<_> = waited.remove(&"w".into()).unwrap().iter().map(numbers).collect();
    ws.sort();
    assert_eq!(ws, [vec![1, 2, 3], vec![9]]);
    assert_eq!(waited.remove(&"v".into()).unwrap().iter().map(numbers).collect::<Vec<_>>(), [vec![1]]);
}

/// What `asked` says a stand-in for GitHub was asked to read with, a token for each query.
fn read_with(asked: &[String]) -> Vec<String> {
    asked
        .iter()
        .filter(|asked| asked.starts_with("post /graphql "))
        .filter_map(|asked| asked.lines().find_map(|line| line.strip_prefix("authorization: bearer ")))
        .map(str::to_string)
        .collect()
}

/// As nobody while the hub has no GitHub account, then as the default once one is connected.
#[tokio::test]
async fn an_agents_read_goes_as_the_default_account_and_fails_without_one() {
    let dir = tempfile::tempdir().unwrap();
    let script = Script {
        users: vec![("an-accounts", Answer::user(7, "ada", None))],
        graphql: Answer::pull_request_read(7, false),
        ..Script::default()
    };
    let github = StandIn::with(script).await;
    let hub = hub_without_account(dir.path(), github.api(), Timers::FOR_TESTS);
    let selection = pull_request_wait("a").watch.unwrap().requested;

    let as_nobody = hub.read_remote_for_agent(&"w".into(), selection.clone()).await.map(|read| read.0);
    let connect = GithubAccountConnect { token: GithubToken::new("an-accounts"), id: None };
    hub.github_account_connect(connect).await.unwrap();
    let as_account = hub.read_remote_for_agent(&"w".into(), selection).await.map(|read| read.0);

    // As a read that may work later, once an account is connected, saying so.
    let refused = as_nobody.unwrap_err();
    assert_eq!(refused.code, ErrorCode::Worker);
    assert!(refused.message.contains("Connect one"), "{}", refused.message);
    assert_eq!(as_account.unwrap().pull_requests[0].pull_request.number, 7);
    assert_eq!(read_with(&github.asked()), ["an-accounts", "an-accounts"]);
}

/// GitHub refusing the default's token, which renewing it or connecting it again may get past.
#[tokio::test]
async fn a_read_github_refuses_the_defaults_token_for_may_work_later() {
    let dir = tempfile::tempdir().unwrap();
    let github = StandIn::with(Script { graphql: vec![Answer::status(401, Vec::new())], ..Script::default() }).await;
    let hub = hub_reading_github(dir.path(), github.api());
    let selection = pull_request_wait("a").watch.unwrap().requested;

    let refused = hub.read_remote_for_agent(&"w".into(), selection).await.map(|read| read.0);

    assert_eq!(refused.unwrap_err().code, ErrorCode::Worker, "a read that may work later");
    assert_eq!(read_with(&github.asked()), ["a-token"]);
}

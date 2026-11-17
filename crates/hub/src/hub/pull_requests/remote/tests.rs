use super::*;
use crate::github::{Answer, Script, StandIn};
use crate::hub::tests::hub_reading_github;
use protocol::client::github_accounts::GithubAccountConnect;
use protocol::fixtures::{observed_pull_request, pull_request_wait};
use protocol::model::{GithubToken, Wait};

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

#[test]
fn overlapping_scopes_coalesce_transitively_and_unrelated_waits_have_separate_backoff() {
    let groups = groups(vec![wait("a", &[1, 2]), wait("b", &[3]), wait("c", &[2, 3]), wait("d", &[9])]);
    assert_eq!(groups.len(), 2);
    let now = Instant::now();
    let mut schedules = HashMap::new();
    let failing = groups.keys().find(|key| key.2.len() == 3).unwrap().clone();
    schedules.insert(failing, RemoteSchedule { next: now + BACKOFF_MAX, failures: 4 });

    let due = due(groups, &schedules, now);

    let [(_, group)] = &due[..] else { panic!("one due") };
    assert_eq!(group.selection.numbers(), [PullRequestNumber::try_from(9).unwrap()]);
}

#[test]
fn the_oldest_due_group_precedes_an_earlier_repository_key() {
    let groups = groups(vec![wait("a", &[1]), wait("b", &[9])]);
    let now = Instant::now();
    let schedules = groups
        .keys()
        .map(|key| {
            let overdue = if key.2[0].get() == 9 { Duration::from_secs(30) } else { Duration::from_secs(1) };
            (key.clone(), RemoteSchedule { next: now.checked_sub(overdue).unwrap(), failures: 0 })
        })
        .collect();

    let due = due(groups, &schedules, now);

    let numbers: Vec<_> = due.iter().map(|(_, group)| group.selection.numbers()[0].get()).collect();
    assert_eq!(numbers, [9, 1], "the one due longest first");
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

/// As nobody while this machine's own `gh` has handed the hub no token and the hub has no account, then as that login
/// once its worker has handed one, then as the default account once one is connected.
#[tokio::test]
async fn an_agents_read_goes_as_the_default_account_or_else_the_machines_own_gh_and_fails_without_either() {
    let dir = tempfile::tempdir().unwrap();
    let answers = [Answer::pull_request_read(7, false), Answer::pull_request_read(7, false)].concat();
    let script =
        Script { users: vec![("an-accounts", Answer::user(7, "ada", None))], graphql: answers, ..Script::default() };
    let github = StandIn::with(script).await;
    let hub = hub_reading_github(dir.path(), github.api());
    let selection = pull_request_wait("a").watch.unwrap().requested;
    let provider = hub.store.get_workspace(&"w".into()).unwrap().provider;
    let handed = hub.found_tokens.of(&provider, Duration::ZERO).await;
    hub.found_tokens.found(provider.clone(), None);

    let as_nobody = hub.read_remote_for_agent(&"w".into(), selection.clone()).await.map(|read| read.0);
    hub.found_tokens.found(provider, handed);
    let as_machines = hub.read_remote_for_agent(&"w".into(), selection.clone()).await.map(|read| read.0);
    let connect = GithubAccountConnect { token: GithubToken::new("an-accounts"), id: None };
    hub.github_account_connect(connect).await.unwrap();
    let as_account = hub.read_remote_for_agent(&"w".into(), selection).await.map(|read| read.0);

    // As a read that may work later, once `gh` there logs in.
    assert_eq!(as_nobody.unwrap_err().code, ErrorCode::Worker);
    assert_eq!(as_machines.unwrap().pull_requests[0].pull_request.number, 7);
    assert_eq!(as_account.unwrap().pull_requests[0].pull_request.number, 7);
    assert_eq!(read_with(&github.asked()), ["a-token", "a-token", "an-accounts", "an-accounts"]);
}

/// GitHub refusing it, as once `gh` has logged in again, so it's no longer the machine's token. The worker is asked
/// for it again by the next link to the provider.
#[tokio::test]
async fn a_machines_token_github_refuses_is_forgotten_and_asked_for_again() {
    let dir = tempfile::tempdir().unwrap();
    let github = StandIn::with(Script { graphql: vec![Answer::status(401, Vec::new())], ..Script::default() }).await;
    let hub = hub_reading_github(dir.path(), github.api());
    let provider = hub.store.get_workspace(&"w".into()).unwrap().provider;
    let selection = pull_request_wait("a").watch.unwrap().requested;

    let refused = hub.read_remote_for_agent(&"w".into(), selection).await.map(|read| read.0);

    assert_eq!(refused.unwrap_err().code, ErrorCode::Worker, "a read that may work later");
    assert_eq!(hub.found_tokens.of(&provider, Duration::ZERO).await, None);
    assert!(!hub.github_logins_asked.has(&provider), "the next link to it asks again");
}

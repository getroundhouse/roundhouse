//! Reading a workspace's own pull request for an agent and for a client, from a stand-in for GitHub, by the branch on
//! GitHub that the branch checked out pushes to, on the real clock, since a call to the stand-in would race tokio's.

use protocol::model::{
    Branches, CheckCounts, GitFile, GitStatus, Head, Local, OwnPullRequest, PrState, PullRequestFound,
    PullRequestLinked,
};
use protocol::worker::{WorkerRequest, WorkerResponse};

use super::super::*;
use crate::hub::tests::{hub_on_github, hub_on_github_timed, hub_without_account, pushed};
use crate::timers::Timers;
use github::PullRequestState;

/// The GraphQL queries the stand-in was asked.
pub(super) fn queries(github: &github::StandIn) -> Vec<String> {
    github.asked().into_iter().filter(|asked| asked.starts_with("post /graphql ")).collect()
}

/// Agents' reads a second apart, many times as long as a read of the stand-in takes, so a read that waited for its turn
/// can be told from one that's slow.
const SPACED: Timers = Timers { agent_spacing: Duration::from_secs(1), ..Timers::FOR_TESTS };

/// A client's ask taking what a read began within the product's time found, for the tests about it.
pub(super) const CLIENTS_TAKE_A_READ: Timers =
    Timers { fresh_for_clients: Timers::DEFAULT.fresh_for_clients, ..Timers::FOR_TESTS };

/// The workspace's pull request as the hub knows it, from its record's link and the cache.
pub(super) fn known(hub: &Hub) -> Option<OwnPullRequest> {
    hub.known_of(&hub.store.get_workspace(&"w".into()).unwrap()).and_then(|found| found.pull_request)
}

/// Its number, and whether it's open, merged or closed.
pub(super) fn numbered(own: &OwnPullRequest) -> (u64, PrState) {
    (own.pull_request.number.get(), own.pull_request.said().state)
}

/// How many of its checks passed, failed and are still running, which were read.
fn counts(own: &OwnPullRequest) -> CheckCounts {
    own.pull_request.checks_said().expect("its checks were read").counts()
}

#[tokio::test]
async fn an_agent_asking_again_soon_after_a_read_waits_for_a_read_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github_timed(dir.path(), SPACED).await;
    pull_requests.set(7, PullRequestState::Open);
    let first_at = tokio::time::Instant::now();
    hub.pull_request_for_agent(&"w".into()).await.unwrap();
    pull_requests.set(7, PullRequestState::Passed);

    let again = hub.pull_request_for_agent(&"w".into()).await.unwrap().unwrap();

    assert_eq!(counts(&again).passed, 1, "read again, not what the first read found");
    assert!(first_at.elapsed() >= hub.timers.agent_spacing, "after {:?}", first_at.elapsed());
}

#[tokio::test]
async fn an_agent_hears_that_a_read_failed_where_a_client_is_shown_what_was_known() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.workspace_pull_request("w".into()).await.unwrap();
    pull_requests.fail(true);

    let agent = hub.pull_request_for_agent(&"w".into()).await;
    let client = hub.workspace_pull_request("w".into()).await;

    assert!(agent.is_err(), "{agent:?}");
    assert_eq!(client.unwrap().map(|pr| numbered(&pr).0), Some(7));
}

/// GitHub's answer to a client's read said little is left, which an agent's read leaves for everything else.
#[tokio::test]
async fn an_agent_is_refused_the_part_of_githubs_limit_kept_for_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    pull_requests.leave(10);
    hub.workspace_pull_request("w".into()).await.unwrap();
    let asked = queries(&github).len();

    let agent = hub.pull_request_for_agent(&"w".into()).await;

    assert_eq!(agent.unwrap_err().code, ErrorCode::Unavailable, "not what the client's read found");
    assert_eq!(queries(&github).len(), asked, "GitHub wasn't asked");
}

/// A branch that's moved is the agent's doing, so the read it brings waits as one an agent asks for does.
#[tokio::test]
async fn a_branch_moving_reads_again_no_sooner_than_an_agent_asking_would() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github_timed(dir.path(), SPACED).await;
    pull_requests.set(7, PullRequestState::Open);
    let moved_at = tokio::time::Instant::now();
    hub.workspace_pull_request("w".into()).await.unwrap();
    pull_requests.set(7, PullRequestState::Passed);

    hub.read_pull_request_after_move(&"w".into(), When::Always);
    let hub = &hub;
    waiting::task::until_ok_async(Duration::from_secs(30), || async move {
        let passed = known(hub).map(|pr| counts(&pr).passed);
        if passed == Some(1) { Ok(()) } else { Err(format!("read again, but {passed:?} checks passed")) }
    })
    .await;

    assert!(moved_at.elapsed() >= hub.timers.agent_spacing, "after {:?}", moved_at.elapsed());
}

/// A read that began once the worktree had checked out another branch may have found that branch's already, and then
/// the read the switch asked for doesn't read again.
#[tokio::test]
async fn a_switch_reads_only_while_what_is_known_is_another_branchs() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let id = WorkspaceId::from("w");
    hub.workspace_pull_request(id.clone()).await.unwrap();
    let a_read = queries(&github).len();
    let linked = hub.store.get_workspace(&id).unwrap().pull_request_link.unwrap();

    hub.read_pull_request(&id, Asked::Waiting, When::OnAnotherBranch).await.unwrap();
    let found_on_it = queries(&github).len() - a_read;
    hub.store.set_workspace_pull_request(&id, Some(&linked), Some(&BranchName::from("aspen"))).unwrap();
    hub.read_pull_request(&id, Asked::Waiting, When::OnAnotherBranch).await.unwrap();

    assert_eq!(found_on_it, 0, "what's known was found on the branch checked out");
    assert!(queries(&github).len() > a_read);
    assert_eq!(known(&hub).map(|pr| pr.pull_request.said().about.title.clone()), Some("Change 7".into()));
}

/// The branch's pull request is the one GitHub has from the branch it's pushed to, and with none there, it has none,
/// which is recorded, so the app offers to make one.
#[tokio::test]
async fn a_workspace_has_the_pull_request_from_the_branch_its_branch_is_pushed_to() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github(dir.path()).await;
    pull_requests.set_from(8, PullRequestState::Open, "another");

    let none = hub.workspace_pull_request("w".into()).await.unwrap();
    pull_requests.set(7, PullRequestState::Merged);
    let merged = hub.workspace_pull_request("w".into()).await.unwrap();

    assert_eq!(none, None, "#8 is from another branch");
    let workspace = hub.store.get_workspace(&"w".into()).unwrap();
    assert_eq!(workspace.pull_request_linked().map(|link| link.number.get()), Some(7));
    assert_eq!(hub.known_of(&workspace), Some(PullRequestFound { pull_request: merged.clone() }));
    assert_eq!(merged.as_ref().map(numbered), Some((7, PrState::Merged)));
    let asked = queries(&github);
    assert!(asked[0].contains(r#"headRefName:\"branch\""#), "{asked:?}");
}

/// The repository cloned is ada/project, a fork, and its branch's pull request merges into example/project, which it
/// was forked from, so it's asked after by the branch in the fork and read in example/project.
#[tokio::test]
async fn a_clone_of_a_forks_pull_request_is_read_where_it_merges_into() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github(dir.path()).await;
    let fork = protocol::model::RepoUrl::try_from("https://github.com/ada/project".to_string()).unwrap();
    // Added again, as a checkout seen again is, so its address is read again.
    hub.store.add_repo(&"r".into(), "repo", "/repo", 0).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&fork)).unwrap();
    let mut branches = pushed("head-7", "head-7", (0, 0));
    branches.had[0].upstream.as_mut().unwrap().address = Some(fork);
    hub.store.set_workspace_branches(&"w".into(), &branches).unwrap();
    pull_requests.set(7, PullRequestState::Open);

    let found = hub.workspace_pull_request("w".into()).await.unwrap();

    assert_eq!(found.map(|pr| numbered(&pr).0), Some(7));
    let asked = queries(&github);
    assert!(asked[0].contains(r#"r0: repository(owner:\"ada\",name:\"project\")"#), "{asked:?}");
    assert!(asked[0].contains(r#"n0: repository(owner:\"ada\",name:\"project\")"#), "{asked:?}");
    assert!(
        asked[1..].iter().all(|query| query.contains(r#"repository(owner:\"example\",name:\"project\")"#)
            && !query.contains(r#"owner:\"ada\""#)),
        "{asked:?}"
    );
}

/// The hub has no GitHub account, so there's no login to read the pull request as, which asking again won't change
/// until one is connected, so with nothing known it's recorded as none, and the app offers to create one rather than
/// look for one for ever.
#[tokio::test]
async fn with_no_account_to_read_as_a_workspace_with_nothing_known_has_none() {
    let dir = tempfile::tempdir().unwrap();
    let pull_requests = github::PullRequests::default();
    let script = github::Script { pull_requests: Some(pull_requests.clone()), ..github::Script::default() };
    let github = github::StandIn::with(script).await;
    let hub = hub_without_account(dir.path(), github.api(), Timers::FOR_TESTS);
    hub.store.set_workspace_branches(&"w".into(), &pushed("head-7", "head-7", (0, 0))).unwrap();
    pull_requests.set(7, PullRequestState::Open);

    let found = hub.workspace_pull_request("w".into()).await;

    assert_eq!(found.unwrap(), None);
    let recorded = hub.store.get_workspace(&"w".into()).unwrap().pull_request_link;
    assert_eq!(recorded, Some(PullRequestLinked { link: None }));
    assert!(queries(&github).is_empty(), "nothing to ask GitHub as");
}

/// How the worktree stands against it: the commits its branch has that GitHub's doesn't, those GitHub's has that it
/// hasn't fetched, and, from its worker, already running, the files it hasn't committed.
#[tokio::test]
async fn a_pull_request_read_says_how_the_worktree_stands_against_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let file = |path: &str| GitFile { path: path.into(), orig_path: None, status: ".M".into() };
    let status = GitStatus { files: vec![file("a"), file("b")], ..GitStatus::default() };
    let link = WorkerLink::answering(move |request| {
        let asked_status = matches!(request, WorkerRequest::GitStatus(_));
        Some(Ok(if asked_status { WorkerResponse::GitStatus(status.clone()) } else { WorkerResponse::Ok }))
    });
    hub.workers.lock().insert("w".into(), link);

    let pushed_here = hub.workspace_pull_request("w".into()).await.unwrap().unwrap().local;
    hub.store.set_workspace_branches(&"w".into(), &pushed("local", "head-7", (2, 0))).unwrap();
    let ahead = hub.workspace_pull_request("w".into()).await.unwrap().unwrap().local;
    hub.store.set_workspace_branches(&"w".into(), &pushed("local", "older", (0, 0))).unwrap();
    let unfetched = hub.workspace_pull_request("w".into()).await.unwrap().unwrap().local;

    assert_eq!(pushed_here, Local { uncommitted: 2, ahead: 0, behind: 0 });
    assert_eq!(ahead, Local { uncommitted: 2, ahead: 2, behind: 0 });
    assert_eq!(unfetched, Local { uncommitted: 2, ahead: 0, behind: 1 }, "GitHub's has a commit it hasn't fetched");
}

/// With HEAD detached there's no branch, so nothing is asked of GitHub.
#[tokio::test]
async fn a_detached_head_has_no_pull_request_and_asks_github_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let detached = Branches { head: Head::Detached { commit: "head-7".into() }, had: Vec::new() };
    hub.store.set_workspace_branches(&"w".into(), &detached).unwrap();

    let read = hub.workspace_pull_request("w".into()).await.unwrap();

    assert_eq!(read, None);
    assert!(queries(&github).is_empty(), "{:?}", queries(&github));
}

/// GitHub deleted the branch as its pull request merged, so the branch has none of its own to list, and the one the
/// workspace had is found by the branch's name.
#[tokio::test]
async fn a_pull_request_whose_branch_github_deleted_as_it_merged_is_still_the_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.workspace_pull_request("w".into()).await.unwrap();
    pull_requests.set(7, PullRequestState::Merged);
    pull_requests.delete_branch("branch");

    let merged = hub.workspace_pull_request("w".into()).await.unwrap();

    assert_eq!(merged.as_ref().map(numbered), Some((7, PrState::Merged)));
}

/// An agent's wait on `numbers` of `w`'s repository until they've merged, started as the tool starts one.
async fn waiting_until_merged(hub: &std::sync::Arc<Hub>, numbers: &[u64]) -> protocol::model::WaitId {
    use protocol::worker::{PullRequestWaitUntil, StartPullRequestWait};
    let numbers: Vec<_> = numbers.iter().map(|n| protocol::model::PullRequestNumber::try_from(*n).unwrap()).collect();
    let asked = StartPullRequestWait {
        pull_requests: protocol::model::PullRequestSelection::try_from(numbers).unwrap(),
        until: PullRequestWaitUntil::Merged,
        timeout_seconds: None,
        call: Some("call".into()),
    };
    let started = hub.start_remote_wait(&"w".into(), "s".into(), asked).await.unwrap();
    assert_eq!(started.wait.ended, None);
    started.wait.id
}

fn ended(hub: &Hub, wait: &protocol::model::WaitId) -> Option<protocol::model::WaitEnd> {
    hub.store.wait(wait).unwrap().ended.map(|ended| ended.how)
}

/// The read of `w`'s own #7, for its bar, is the read of what the wait on it watches, so it ends as the bar shows #7
/// merged, rather than at a read of its own minutes later.
#[tokio::test]
async fn a_wait_on_the_workspaces_own_pull_request_ends_as_its_read_finds_it_merged() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let wait = waiting_until_merged(&hub, &[7]).await;
    pull_requests.set(7, PullRequestState::Merged);
    let before = queries(&github).len();

    let merged = hub.workspace_pull_request("w".into()).await.unwrap();

    assert_eq!(merged.map(|pr| numbered(&pr).1), Some(PrState::Merged));
    assert_eq!(ended(&hub, &wait), Some(protocol::model::WaitEnd::Merged));
    assert_eq!(queries(&github).len() - before, 3, "which branches have which, then one read of #7 for both");
}

/// A wait on #6, below `w`'s own #7 in its stack and merged already, as well as #7: the read of #7 for the bar takes in
/// #6 too, so the wait can tell that #6 merged rather than left the stack, and ends with the bar.
#[tokio::test]
async fn a_wait_on_a_stack_under_the_workspaces_pull_request_ends_with_its_read() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set_from(6, PullRequestState::Merged, "lower");
    pull_requests.set(7, PullRequestState::Open);
    let wait = waiting_until_merged(&hub, &[6, 7]).await;
    pull_requests.set(7, PullRequestState::Merged);

    hub.workspace_pull_request("w".into()).await.unwrap();

    assert_eq!(ended(&hub, &wait), Some(protocol::model::WaitEnd::Merged));
}

/// A client asking just after a read, as a pane shown as its turn ends, or a second client, is answered with what
/// that read found, without asking GitHub again, until that's as old as a client's ask takes.
#[tokio::test]
async fn a_client_asking_just_after_a_read_is_answered_from_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github_timed(dir.path(), CLIENTS_TAKE_A_READ).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.workspace_pull_request("w".into()).await.unwrap();
    let a_read = queries(&github).len();
    pull_requests.set(7, PullRequestState::Passed);

    let soon = hub.workspace_pull_request("w".into()).await.unwrap().unwrap();
    hub.github_cache.age(CLIENTS_TAKE_A_READ.fresh_for_clients);
    let later = hub.workspace_pull_request("w".into()).await.unwrap().unwrap();

    let soon_counts = counts(&soon);
    assert_eq!((numbered(&soon).0, soon_counts.pending, soon_counts.passed), (7, 1, 0), "what the read found");
    assert_eq!(counts(&later).passed, 1, "read again");
    assert_eq!(queries(&github).len(), 2 * a_read);
}

/// A turn that ends reads the pull request again even just after a read, since the agent may have pushed meanwhile.
#[tokio::test]
async fn a_turn_ending_just_after_a_read_reads_again() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github_timed(dir.path(), CLIENTS_TAKE_A_READ).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.workspace_pull_request("w".into()).await.unwrap();
    pull_requests.set(7, PullRequestState::Passed);

    hub.read_pull_request_after_turn(&"w".into());

    let hub = &hub;
    waiting::task::until_ok_async(Duration::from_secs(30), || async move {
        let passed = known(hub).map(|pr| counts(&pr).passed);
        if passed == Some(1) { Ok(()) } else { Err(format!("read again, but {passed:?} checks passed")) }
    })
    .await;
}

/// One read in turn that finds what the last read did waits twice as long before the next, though each read is newer
/// than the last, so what's compared is what GitHub said, not when it said it.
#[tokio::test]
async fn a_pull_request_that_comes_back_the_same_waits_longer_each_time() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    // Merged, so it isn't moving on GitHub, which would keep it at the soonest.
    pull_requests.set(7, PullRequestState::Merged);
    let id = WorkspaceId::from("w");
    let mut every = Vec::new();
    for _ in 0..3 {
        hub.read_pull_request(&id, Asked::InTurn, When::Always).await.unwrap();
        every.push(hub.pull_requests.schedules.lock().get(&id).unwrap().every);
    }

    assert_eq!(every, [Duration::from_mins(2), Duration::from_mins(4), Duration::from_mins(8)]);
}

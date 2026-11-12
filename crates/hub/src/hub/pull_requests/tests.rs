//! Reading a pull request for an agent and for a client, through a stand-in for the workspace's worker, on tokio's
//! clock, which moves on by itself while every task waits for it.

use std::sync::atomic::AtomicUsize;

use protocol::model::{Branches, Head};
use protocol::worker::RateLimit;

use super::*;
use crate::hub::tests::{hub_with_worktree, open, reading, says};

#[tokio::test(start_paused = true)]
async fn an_agent_asking_again_soon_after_a_read_waits_for_a_read_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let says = says(Some(open()));
    let link = reading(&hub, &says);
    hub.pull_request_for_agent(&"w".into(), &link).await.unwrap();
    *says.lock() = Ok(Some(PullRequest { title: "Pushed".into(), ..open() }));
    let asked_at = tokio::time::Instant::now();

    let again = hub.pull_request_for_agent(&"w".into(), &link).await.unwrap();

    assert_eq!(again.unwrap().title, "Pushed", "read again, not what the first read found");
    assert!(asked_at.elapsed() >= AGENT_SPACING, "after {:?}", asked_at.elapsed());
}

#[tokio::test(start_paused = true)]
async fn an_agent_hears_that_a_read_failed_where_a_client_is_shown_what_was_known() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let says = says(Some(open()));
    let link = reading(&hub, &says);
    hub.workspace_pull_request("w".into()).await.unwrap();
    *says.lock() = Err(ApiError::new(ErrorCode::Unavailable, "GitHub can't be reached"));

    let agent = hub.pull_request_for_agent(&"w".into(), &link).await;
    let client = hub.workspace_pull_request("w".into()).await;

    assert_eq!(agent.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(client.unwrap(), Some(open()));
}

#[tokio::test(start_paused = true)]
async fn an_agent_is_refused_the_part_of_githubs_limit_kept_for_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let limit = RateLimit { limit: 5000, remaining: 10, resets_at_ms: now_ms() + 3_600_000 };
    let link = WorkerLink::answering(move |request| {
        Some(Ok(if matches!(request, WorkerRequest::GithubRateLimit { .. }) {
            WorkerResponse::GithubRateLimit(limit)
        } else {
            WorkerResponse::PullRequest { pull_request: Some(open()), branch: None }
        }))
    });
    hub.workers.lock().insert("w".into(), link.clone());
    hub.workspace_pull_request("w".into()).await.unwrap();

    let agent = hub.pull_request_for_agent(&"w".into(), &link).await;

    assert_eq!(agent.unwrap_err().code, ErrorCode::Unavailable, "not what the client's read found");
}

/// A branch that's moved is the agent's doing, so the read it brings waits as one an agent asks for does.
#[tokio::test(start_paused = true)]
async fn a_branch_moving_reads_again_no_sooner_than_an_agent_asking_would() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let says = says(Some(open()));
    let link = reading(&hub, &says);
    hub.workspace_pull_request("w".into()).await.unwrap();
    *says.lock() = Ok(Some(PullRequest { title: "Pushed".into(), ..open() }));
    let moved_at = tokio::time::Instant::now();

    hub.read_pull_request_after_move(&"w".into(), link, When::Always);
    let mut title = None;
    for _ in 0..600 {
        title = hub.workspace(&"w".into()).await.unwrap().known_pull_request().map(|pr| pr.title.clone());
        if title.as_deref() == Some("Pushed") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert_eq!(title.as_deref(), Some("Pushed"), "read again");
    assert!(moved_at.elapsed() >= AGENT_SPACING, "after {:?}", moved_at.elapsed());
}

/// A read that began once the worktree had checked out another branch may have found that branch's already, and then
/// the read the switch asked for doesn't read again.
#[tokio::test(start_paused = true)]
async fn a_switch_reads_only_while_what_is_known_is_another_branchs() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = reads.clone();
    let link = WorkerLink::answering(move |request| {
        Some(Ok(if matches!(request, WorkerRequest::PullRequest { .. }) {
            counted.fetch_add(1, Ordering::SeqCst);
            WorkerResponse::PullRequest { pull_request: Some(open()), branch: Some(BranchName::from("header")) }
        } else {
            WorkerResponse::Ok
        }))
    });
    let id = WorkspaceId::from("w");
    let on_header = Branches { head: Head::Branch { name: BranchName::from("header") }, had: Vec::new() };
    hub.store.set_workspace_branches(&id, &on_header).unwrap();
    let found = PullRequestFound { pull_request: Some(open()) };

    hub.store.set_workspace_pull_request(&id, &found, Some(&BranchName::from("header"))).unwrap();
    hub.read_pull_request(&id, "/w".into(), &link, Asked::Waiting, When::OnAnotherBranch).await.unwrap();
    let found_on_it = reads.load(Ordering::SeqCst);
    hub.store.set_workspace_pull_request(&id, &found, Some(&BranchName::from("aspen"))).unwrap();
    hub.read_pull_request(&id, "/w".into(), &link, Asked::Waiting, When::OnAnotherBranch).await.unwrap();
    let found_elsewhere = reads.load(Ordering::SeqCst);

    assert_eq!(found_on_it, 0, "what's known was found on the branch checked out");
    assert_eq!(found_elsewhere, 1);
}

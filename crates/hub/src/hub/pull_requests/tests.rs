//! Reading a pull request for an agent and for a client, through a stand-in for the workspace's worker, on tokio's
//! clock, which moves on by itself while every task waits for it.

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
            WorkerResponse::PullRequest { pull_request: Some(open()) }
        }))
    });
    hub.workers.lock().insert("w".into(), link.clone());
    hub.workspace_pull_request("w".into()).await.unwrap();

    let agent = hub.pull_request_for_agent(&"w".into(), &link).await;

    assert_eq!(agent.unwrap_err().code, ErrorCode::Unavailable, "not what the client's read found");
}

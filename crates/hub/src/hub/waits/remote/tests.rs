use super::*;
use crate::hub::tests::hub_with_worktree;
use protocol::fixtures::pull_request_wait;
use protocol::worker::{RemotePullRequestsReply, WorkerRequest, WorkerResponse};

fn request(call: &str) -> StartPullRequestWait {
    StartPullRequestWait {
        pull_requests: pull_request_wait("a").watch.unwrap().requested,
        until: PullRequestWaitUntil::RequiredChecks,
        timeout_seconds: None,
        call: Some(call.into()),
    }
}

#[tokio::test(start_paused = true)]
async fn explicit_waits_follow_remote_reads_and_failed_starts_leave_existing_waits() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let snapshot = pull_request_wait("a").watch.unwrap().snapshot;
    let answer = Arc::new(parking_lot::Mutex::new(Ok(snapshot.clone())));
    let link = WorkerLink::answering({
        let answer = answer.clone();
        move |request| {
            let WorkerRequest::ReadRemotePullRequests(_) = request else {
                return Some(Ok(WorkerResponse::Ok));
            };
            Some(Ok(WorkerResponse::RemotePullRequests(RemotePullRequestsReply {
                observation: answer.lock().clone(),
                rate_limit: None,
                cost: 4,
                affected: Vec::new(),
            })))
        }
    });
    hub.workers.lock().insert("w".into(), link.clone());
    let started = hub.start_remote_wait(&"w".into(), &link, "s".into(), request("first")).await.unwrap();
    *answer.lock() = Err(ApiError::new(ErrorCode::Worker, "offline"));

    let again = hub.start_remote_wait(&"w".into(), &link, "s".into(), request("first")).await.unwrap();
    let failed = hub.start_remote_wait(&"w".into(), &link, "s".into(), request("second")).await;

    assert_eq!(again, started);
    assert_eq!(failed.unwrap_err().code, ErrorCode::Worker);
    assert_eq!(hub.store.remote_waits().unwrap(), [started.wait]);
    assert_eq!(hub.store.get_session(&"s".into()).unwrap().session.open_waits().len(), 1);
}

#[tokio::test]
async fn outages_keep_the_last_snapshot_and_access_loss_ends_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let wait = pull_request_wait("a");
    hub.store.start_wait(&wait, Seen::default()).unwrap();
    let change =
        crate::store::SessionChange { status: None, queue_held: Some(true), recorded: None, initialized: None };
    hub.store
        .append(
            &"s".into(),
            None,
            &protocol::event::EventBody::Notice {
                level: protocol::event::NoticeLevel::Info,
                message: "held".into(),
                parent_tool_use_id: None,
            },
            None,
            None,
            Some(change),
        )
        .unwrap();
    let repository = wait.watch.as_ref().unwrap().snapshot.repository.clone();

    hub.remote_waits_failed(&"w".into(), &repository, &ApiError::new(ErrorCode::Worker, "offline"), &[]).await;
    let pending = hub.store.wait(&wait.id).unwrap();
    assert_eq!(pending.ended, None);
    assert_eq!(pending.watch.as_ref().unwrap().snapshot, wait.watch.as_ref().unwrap().snapshot);
    hub.remote_waits_failed(&"w".into(), &repository, &ApiError::new(ErrorCode::Unavailable, "access lost"), &[]).await;

    let ended = hub.store.wait(&wait.id).unwrap();
    assert_eq!(ended.ended.unwrap().how, WaitEnd::Unavailable);
    assert_eq!(ended.watch.as_ref().unwrap().read_error.as_deref(), Some("access lost"));
}

#[tokio::test]
async fn a_shared_reads_pr_access_failure_does_not_end_an_unrelated_wait() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let a = pull_request_wait("a");
    let mut b = pull_request_wait("b");
    b.number = 9;
    let watch = b.watch.as_mut().unwrap();
    watch.requested =
        protocol::model::PullRequestSelection::try_from(vec![PullRequestNumber::try_from(9).unwrap()]).unwrap();
    watch.snapshot.pull_requests = vec![protocol::fixtures::observed_pull_request(9)];
    let repository = a.watch.as_ref().unwrap().snapshot.repository.clone();
    hub.store.start_wait(&a, Seen::default()).unwrap();
    hub.store.start_wait(&b, Seen::default()).unwrap();
    let change =
        crate::store::SessionChange { status: None, queue_held: Some(true), recorded: None, initialized: None };
    hub.store
        .append(
            &"s".into(),
            None,
            &protocol::event::EventBody::Notice {
                level: protocol::event::NoticeLevel::Info,
                message: "held".into(),
                parent_tool_use_id: None,
            },
            None,
            None,
            Some(change),
        )
        .unwrap();

    hub.remote_waits_failed(
        &"w".into(),
        &repository,
        &ApiError::new(ErrorCode::Unavailable, "PR inaccessible"),
        &[PullRequestNumber::try_from(7).unwrap()],
    )
    .await;

    assert_eq!(hub.store.wait(&a.id).unwrap().ended.unwrap().how, WaitEnd::Unavailable);
    let untouched = hub.store.wait(&b.id).unwrap();
    assert_eq!(untouched.ended, None);
    assert_eq!(untouched.watch.unwrap().read_error, None);
}

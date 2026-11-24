use super::*;
use crate::hub::tests::{hub_reading_github, hub_with_worktree};
use github::{Answer, Script, StandIn};
use protocol::fixtures::pull_request_wait;
use protocol::model::CheckState;

fn request(call: &str) -> StartPullRequestWait {
    StartPullRequestWait {
        pull_requests: pull_request_wait("a").watch.unwrap().requested,
        until: PullRequestWaitUntil::RequiredChecks,
        timeout_seconds: None,
        call: Some(call.into()),
    }
}

/// Read by the hub from GitHub, which answers the first start's read and then can't be reached.
#[tokio::test]
async fn explicit_waits_follow_remote_reads_and_failed_starts_leave_existing_waits() {
    let dir = tempfile::tempdir().unwrap();
    let mut answers = Answer::pull_request_read(7, false);
    answers.push(Answer::status(503, Vec::new()));
    let github = StandIn::with(Script { graphql: answers, ..Script::default() }).await;
    let hub = hub_reading_github(dir.path(), github.api());
    let started = hub.start_remote_wait(&"w".into(), "s".into(), request("first")).await.unwrap();

    let again = hub.start_remote_wait(&"w".into(), "s".into(), request("first")).await.unwrap();
    let failed = hub.start_remote_wait(&"w".into(), "s".into(), request("second")).await;

    assert_eq!(again, started);
    let watch = started.wait.watch.as_ref().unwrap();
    assert_eq!(watch.snapshot.repository.name_with_owner, "example/project");
    assert_eq!(watch.snapshot.pull_requests[0].checks_said().unwrap().checks[0].state, CheckState::Pending);
    assert_eq!(failed.unwrap_err().code, ErrorCode::Worker);
    let kept: Vec<_> = hub.store.remote_waits().unwrap().into_iter().map(|wait| hub.as_read(wait)).collect();
    assert_eq!(kept, [started.wait]);
    assert_eq!(hub.store.get_session(&"s".into()).unwrap().session.open_waits().len(), 1);
    let asked = github.asked();
    assert!(asked.iter().all(|asked| asked.contains("authorization: bearer a-token\r\n")), "{asked:?}");
}

#[tokio::test]
async fn outages_keep_the_last_snapshot_and_access_loss_ends_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let wait = pull_request_wait("a");
    hub.start_wait(wait.clone()).await.unwrap();
    let change = crate::store::SessionChange {
        status: None,
        queue_held: Some(true),
        recorded: None,
        initialized: None,
        claims: None,
        interaction: None,
    };
    hub.store
        .append(
            &"s".into(),
            None,
            &protocol::event::EventBody::Notice {
                level: protocol::event::NoticeLevel::Info,
                message: "held".into(),
                subagent: None,
            },
            None,
            None,
            Some(change),
        )
        .unwrap();
    let repository = wait.watch.as_ref().unwrap().snapshot.repository.clone();

    hub.remote_waits_failed(&"w".into(), Some(&repository), &ApiError::new(ErrorCode::Worker, "offline"), &[]).await;
    let pending = hub.as_read(hub.store.wait(&wait.id).unwrap());
    assert_eq!(pending.ended, None);
    assert_eq!(pending.watch.as_ref().unwrap().snapshot, wait.watch.as_ref().unwrap().snapshot);
    hub.remote_waits_failed(&"w".into(), Some(&repository), &ApiError::new(ErrorCode::Unavailable, "access lost"), &[])
        .await;

    let ended = hub.store.wait(&wait.id).unwrap();
    assert_eq!(ended.ended.unwrap().how, WaitEnd::Unavailable);
    assert_eq!(ended.watch.as_ref().unwrap().read_error.as_deref(), Some("access lost"));
}

/// A read that can't reach a pull request ends each wait that watches it, one that has read nothing yet included, and no
/// other.
#[tokio::test]
async fn a_shared_reads_pr_access_failure_ends_each_wait_on_it_and_no_unrelated_one() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let a = pull_request_wait("a");
    let mut b = pull_request_wait("b");
    b.number = 9;
    let watch = b.watch.as_mut().unwrap();
    watch.requested =
        protocol::model::PullRequestSelection::try_from(vec![PullRequestNumber::try_from(9).unwrap()]).unwrap();
    watch.snapshot.pull_requests = vec![protocol::fixtures::pull_request(9)];
    // On the same pull request as `a`, with none of what it read kept, as a wait from before facets has.
    let mut c = pull_request_wait("c");
    c.watch.as_mut().unwrap().snapshot.pull_requests.clear();
    let repository = a.watch.as_ref().unwrap().snapshot.repository.clone();
    hub.store.start_wait(&a).unwrap();
    hub.store.start_wait(&b).unwrap();
    hub.store.start_wait(&c).unwrap();
    let change = crate::store::SessionChange {
        status: None,
        queue_held: Some(true),
        recorded: None,
        initialized: None,
        claims: None,
        interaction: None,
    };
    hub.store
        .append(
            &"s".into(),
            None,
            &protocol::event::EventBody::Notice {
                level: protocol::event::NoticeLevel::Info,
                message: "held".into(),
                subagent: None,
            },
            None,
            None,
            Some(change),
        )
        .unwrap();

    hub.remote_waits_failed(
        &"w".into(),
        Some(&repository),
        &ApiError::new(ErrorCode::Unavailable, "PR inaccessible"),
        &[PullRequestNumber::try_from(7).unwrap()],
    )
    .await;

    assert_eq!(hub.store.wait(&a.id).unwrap().ended.unwrap().how, WaitEnd::Unavailable);
    let unread = hub.store.wait(&c.id).unwrap();
    assert_eq!(unread.ended.unwrap().how, WaitEnd::Unavailable, "one that has read nothing yet ends too");
    assert_eq!(unread.watch.unwrap().read_error.as_deref(), Some("PR inaccessible"));
    let untouched = hub.store.wait(&b.id).unwrap();
    assert_eq!(untouched.ended, None);
    assert_eq!(untouched.watch.unwrap().read_error, None);
}

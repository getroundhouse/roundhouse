use super::super::*;
use crate::hub::tests::{hub_in, hub_on_github, hub_with_worktree};
use crate::hub::{Followed, FollowerId};
use crate::store::tests::idle_session;
use crate::store::{SessionChange, StatusChange};
use github::PullRequestState;
use protocol::event::{AgentTask, EventBody, ExitReason, NoticeLevel, TaskKind};
use protocol::model::{OwnPullRequest, Seq, SessionStatus, Workspace};
use protocol::worker::{
    Answer as _, CancelWait, PullRequestWaitUntil, ReadWait, StartPullRequestWait, WaitAnswer, WorkerAsk,
    WorkerRequest, WorkerResponse,
};

/// A wait of session `s`'s, in workspace `w`, on #7's required checks, as [`hub_in`] makes them, that hasn't ended.
pub(in super::super) fn wait() -> Wait {
    Wait { call: None, ..protocol::fixtures::pull_request_wait("a") }
}

/// What the hub answers the worker of `w` asking `ask` for the session `s`.
pub(super) async fn ask(
    hub: &Arc<Hub>,
    link: &WorkerLink,
    ask: WorkerAsk,
) -> Result<(Wait, Option<OwnPullRequest>), ApiError> {
    let reply = hub.answer_ask(&"w".into(), link, "s".into(), ask).await?;
    let answer = WaitAnswer::read(reply).unwrap_or_else(|reply| panic!("a wait: {reply:?}"));
    Ok((answer.wait, answer.pull_request))
}

/// Starts a wait until #7 has merged, for the agent's call `call`.
pub(super) fn start(call: &str) -> WorkerAsk {
    WorkerAsk::StartPullRequestWait(StartPullRequestWait {
        pull_requests: protocol::fixtures::pull_request_wait("a").watch.unwrap().requested,
        until: PullRequestWaitUntil::Merged,
        timeout_seconds: None,
        call: Some(call.into()),
    })
}

pub(super) fn how(wait: &Wait) -> Option<WaitEnd> {
    wait.ended.map(|ended| ended.how)
}

/// A wait on the pull requests it watches carries them in its watch, as last read, apart from any worktree, so it comes
/// with no pull request of the workspace's own.
#[tokio::test]
async fn a_worker_reads_a_wait_with_the_pull_requests_as_last_read_and_only_for_its_own_chat() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.store.start_wait(&wait()).unwrap();
    let mut other = idle_session(1);
    other.session.id = "t".into();
    hub.store.insert_session(&other, None).unwrap();
    let link = WorkerLink::unconnected();
    let read = || WorkerAsk::Wait(ReadWait { wait: "a".into() });

    let (wait, pull_request) = ask(&hub, &link, read()).await.unwrap();
    let refused = hub.answer_ask(&"w".into(), &link, "t".into(), read()).await.unwrap_err();

    assert_eq!((wait.id.as_str(), wait.ended, pull_request), ("a", None, None));
    assert_eq!(wait.watch, self::wait().watch, "as last read");
    assert_eq!(refused.code, ErrorCode::NotFound, "another chat's wait: {refused:?}");
}

#[tokio::test]
async fn cancelling_a_wait_ends_it_once_and_a_wait_that_had_ended_stays_as_it_ended() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.store.start_wait(&wait()).unwrap();
    let (link, cancel) = (WorkerLink::unconnected(), || WorkerAsk::CancelWait(CancelWait { wait: "a".into() }));

    let (cancelled, _) = ask(&hub, &link, cancel()).await.unwrap();
    let (again, _) = ask(&hub, &link, cancel()).await.unwrap();

    assert_eq!(how(&cancelled), Some(WaitEnd::Cancelled));
    assert_eq!(again.ended, cancelled.ended, "ended once");
}

/// Before its archive script runs, so a wait that ends meanwhile doesn't start an agent in a worktree that's going.
#[tokio::test]
async fn archiving_a_workspace_calls_off_its_waits_before_its_archive_script_runs() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let waiting_as_the_script_ran = Arc::new(parking_lot::Mutex::new(None));
    let link = WorkerLink::answering({
        let (hub, waiting) = (Arc::downgrade(&hub), waiting_as_the_script_ran.clone());
        move |request| {
            if matches!(request, WorkerRequest::ArchiveScript(_)) {
                let waited_on = hub.upgrade().map(|hub| hub.store.workspaces_waited_on().unwrap());
                *waiting.lock() = waited_on.map(|workspaces| !workspaces.is_empty());
            }
            Some(Ok(WorkerResponse::Ok))
        }
    });
    hub.workers.lock().insert("w".into(), link.clone());
    ask(&hub, &link, start("toolu_1")).await.unwrap();

    hub.workspace_archive("w".into(), true).await.unwrap();

    assert_eq!(*waiting_as_the_script_ran.lock(), Some(false));
}

/// As after the hub started again: its agent is started for a chat waiting on nothing else, which says so when it
/// can't be, and isn't for one that was cut off or is held for the person.
#[tokio::test]
async fn a_wait_ending_starts_the_agent_of_a_chat_waiting_on_nothing_else() {
    let cases = [
        (SessionStatus::Background, false, true),
        (SessionStatus::Idle, false, true),
        (SessionStatus::Interrupted, true, false),
        (SessionStatus::Interrupted, false, false),
        (SessionStatus::Idle, true, false),
    ];
    for (status, queue_held, woken) in cases {
        let dir = tempfile::tempdir().unwrap();
        let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
        pull_requests.set(7, PullRequestState::Open);
        let change = SessionChange {
            status: Some(status.into()),
            queue_held: Some(queue_held),
            recorded: None,
            initialized: None,
            claims: None,
            interaction: None,
        };
        let said = EventBody::Notice { level: NoticeLevel::Info, message: "x".into(), subagent: None };
        hub.store.append(&"s".into(), None, &said, None, None, Some(change)).unwrap();
        let starts = Arc::new(parking_lot::Mutex::new(0));
        let link = WorkerLink::answering({
            let starts = starts.clone();
            move |request| {
                if matches!(request, WorkerRequest::AgentStart(_)) {
                    *starts.lock() += 1;
                }
                // Not that it started, so starting fails.
                Some(Ok(WorkerResponse::Ok))
            }
        });
        hub.workers.lock().insert("w".into(), link.clone());
        ask(&hub, &link, start("toolu_1")).await.unwrap();

        pull_requests.set(7, PullRequestState::Merged);
        hub.workspace_pull_request("w".into()).await.unwrap();
        // Once the wake is done, since a chat carries out what it's asked in turn.
        let _ = hub.session_cancel("s".into()).await;

        assert_eq!(*starts.lock(), usize::from(woken), "{status:?}, held: {queue_held}");
        let events = hub.store.read_events(&"s".into(), Seq::new(1), 100, u64::MAX, false).unwrap();
        let told = events
            .iter()
            .any(|event| matches!(event.body, EventBody::AgentExited { reason: ExitReason::StartFailed, .. }));
        assert_eq!(told, woken, "{status:?}, held: {queue_held}");
    }
}

/// As after a hub started again, before the agent has listed anything: what the chat has in the background is what
/// its record lists, so one whose only work is a wait isn't counted as running, and one with a command still is.
#[tokio::test]
async fn a_chat_whose_record_lists_only_a_wait_isnt_counted_as_running() {
    for (kind, running) in [(TaskKind::Wait, 0), (TaskKind::Command, 1)] {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        let task = AgentTask { id: "t".into(), description: None, kind, call: None, subagent: None };
        let listed = EventBody::AgentTasks { tasks: vec![task.clone()], stopped: vec![] };
        let status = StatusChange { background_tasks: Some(vec![task]), ..SessionStatus::Background.into() };
        let change = SessionChange {
            status: Some(status),
            queue_held: None,
            recorded: None,
            initialized: None,
            claims: None,
            interaction: None,
        };
        hub.store.append(&"s".into(), None, &listed, None, None, Some(change)).unwrap();

        hub.live_session(&"s".into()).await.unwrap();

        assert_eq!(hub.status().running_sessions, running, "{kind:?}");
    }
}

#[tokio::test]
async fn with_no_client_following_the_hub_only_a_workspace_waited_on_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let w = hub.store.get_workspace(&"w".into()).unwrap();
    // One that would come first.
    let other = Workspace { id: "v".into(), name: "other".into(), instance_id: "v".into(), ..w };
    hub.store.insert_workspace(&other, &crate::store::WorkspacePlacement::local_checkout("/repo"), None).unwrap();
    let next = || async { hub.to_read_in_turn().await.first().map(|workspace| workspace.id.clone()) };

    let unwaited = next().await;
    hub.store.start_wait(&wait()).unwrap();
    let waited = next().await;
    let _following = hub.follow(FollowerId { connection: 1, subscription: 1 }, [Followed::Hub]);
    let seen = next().await;

    assert_eq!(unwaited, None, "nobody would see it");
    assert_eq!(waited, Some("w".into()), "not the other, which nobody would see either");
    assert_eq!(seen, Some("v".into()), "once a client follows the hub");
}

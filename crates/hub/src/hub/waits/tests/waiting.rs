use super::super::*;
use crate::github::PullRequestState;
use crate::hub::tests::{hub_in, hub_on_github, hub_with_worktree, open, pushed, taking_worker};
use crate::store::tests::idle_session;
use crate::store::{Seen, SessionChange, StatusChange};
use protocol::event::{AgentTask, EventBody, ExitReason, NoticeLevel, TaskKind};
use protocol::model::{Checks, GitFile, GitStatus, PrState, PullRequest, Seq, SessionStatus, Workspace};
use protocol::worker::{
    Answer as _, CancelWait, ReadWait, StartWait, WaitAnswer, WorkerAsk, WorkerRequest, WorkerResponse,
};

/// A wait of session `s`'s, in workspace `w`, as [`hub_in`] makes them, on [`open`], that hasn't ended.
pub(in super::super) fn wait(until: WaitUntil) -> Wait {
    Wait {
        id: "a".into(),
        session_id: "s".into(),
        workspace_id: "w".into(),
        number: 7,
        call: None,
        until,
        watch: None,
        started_at_ms: 1,
        ended: None,
    }
}

/// What the hub answers the worker of `w` asking `ask` for the session `s`.
pub(super) async fn ask(
    hub: &Arc<Hub>,
    link: &WorkerLink,
    ask: WorkerAsk,
) -> Result<(Wait, Option<PullRequest>), ApiError> {
    let reply = hub.answer_ask(&"w".into(), link, "s".into(), ask).await?;
    let answer = WaitAnswer::read(reply).unwrap_or_else(|reply| panic!("a wait: {reply:?}"));
    Ok((answer.wait, answer.pull_request))
}

/// Starts a wait until it's done, for the agent's call `call`.
pub(super) fn start(call: &str) -> WorkerAsk {
    WorkerAsk::StartWait(StartWait { until: WaitUntil::Done, call: Some(call.into()) })
}

pub(super) fn how(wait: &Wait) -> Option<WaitEnd> {
    wait.ended.map(|ended| ended.how)
}

#[tokio::test]
async fn a_wait_starts_from_a_read_of_its_own_even_just_after_one() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Failed);
    let (link, _) = taking_worker(&hub);
    hub.pull_request_for_agent(&"w".into()).await.unwrap();
    // The agent pushes a fix, and its checks start again.
    pull_requests.set(7, PullRequestState::Open);

    let (wait, pull_request) = ask(&hub, &link, start("toolu_1")).await.unwrap();
    // Asking again needs no read, so one that would fail now changes nothing.
    pull_requests.fail(true);
    let again = ask(&hub, &link, start("toolu_1")).await;

    let running = Checks { passed: 0, failed: 0, pending: 1 };
    let checks = pull_request.map(|pr| pr.checks);
    assert_eq!((how(&wait), checks), (None, Some(running)), "not ended by the checks of the commit it replaced");
    assert_eq!((wait.number, wait.call.clone()), (7, Some("toolu_1".into())));
    let again = again.expect("the same call asking again was read for again, rather than given the wait it started");
    assert_eq!(again.0, wait, "the same call asking again gets the one it started, as when the agent's CLI asks again");
}

#[tokio::test]
async fn a_wait_doesnt_start_from_what_was_known_when_its_own_read_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let (link, _) = taking_worker(&hub);
    hub.pull_request_for_agent(&"w".into()).await.unwrap();
    pull_requests.fail(true);

    let refused = ask(&hub, &link, start("toolu_1")).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::Worker, "{refused:?}");
    assert!(hub.store.workspaces_waited_on().unwrap().is_empty());
}

#[tokio::test]
async fn a_read_that_finds_the_pull_request_merged_ends_its_wait_and_one_that_fails_ends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Queued);
    let (link, _) = taking_worker(&hub);
    let (wait, _) = ask(&hub, &link, start("toolu_1")).await.unwrap();

    pull_requests.fail(true);
    let _ = hub.workspace_pull_request("w".into()).await;
    let after_failing = hub.store.wait(&wait.id).unwrap();
    pull_requests.fail(false);
    pull_requests.set(7, PullRequestState::Merged);
    hub.workspace_pull_request("w".into()).await.unwrap();

    assert_eq!(how(&after_failing), None, "a read that failed says nothing");
    let (read, pull_request) = ask(&hub, &link, WorkerAsk::Wait(ReadWait { wait: wait.id })).await.unwrap();
    assert_eq!((how(&read), pull_request.map(|pr| pr.state)), (Some(WaitEnd::Merged), Some(PrState::Merged)));
}

#[tokio::test]
async fn a_branch_without_a_pull_request_has_none_to_wait_on() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _pull_requests, _github) = hub_on_github(dir.path()).await;
    let (link, _) = taking_worker(&hub);

    let refused = ask(&hub, &link, start("toolu_1")).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::NotFound, "{refused:?}");
}

/// Uncommitted files are left alone, since they change nothing GitHub says, and so is a pull request that ends the
/// wait as it starts, or that the queue merges whatever the worktree holds.
#[tokio::test]
async fn a_worktree_with_commits_not_pushed_has_nothing_to_wait_on_yet() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    let unpushed = pushed("local", "head-7", (2, 0));
    hub.store.set_workspace_branches(&"w".into(), &unpushed).unwrap();
    let file = |path: &str| GitFile { path: path.into(), orig_path: None, status: ".M".into() };
    let status = GitStatus { files: vec![file("a"), file("b"), file("c")], ..GitStatus::default() };
    let link = WorkerLink::answering(move |request| {
        let asked_status = matches!(request, WorkerRequest::GitStatus(_));
        Some(Ok(if asked_status { WorkerResponse::GitStatus(status.clone()) } else { WorkerResponse::Ok }))
    });
    hub.workers.lock().insert("w".into(), link.clone());

    let refused = ask(&hub, &link, start("toolu_1")).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::Conflict, "{refused:?}");
    assert!(refused.message.starts_with("2 commits here aren't on the pull request yet"), "{}", refused.message);
    assert!(hub.store.workspaces_waited_on().unwrap().is_empty());

    // Three files uncommitted, and a commit GitHub's has that the worktree hasn't fetched.
    let unfetched = pushed("local", "older", (0, 0));
    let cases = [
        (PullRequestState::Merged, &unpushed, Some(WaitEnd::Merged)),
        (PullRequestState::Draft, &unpushed, Some(WaitEnd::Draft)),
        (PullRequestState::Queued, &unpushed, None),
        (PullRequestState::Open, &unfetched, None),
    ];
    for (n, (state, branches, ends)) in cases.into_iter().enumerate() {
        pull_requests.set(7, state);
        hub.store.set_workspace_branches(&"w".into(), branches).unwrap();
        let started = ask(&hub, &link, start(&format!("toolu_{}", n + 2))).await;
        assert_eq!(started.map(|(wait, _)| how(&wait)), Ok(ends), "{state:?}");
    }
}

#[tokio::test]
async fn a_wait_on_a_pull_request_thats_done_or_needs_the_agent_ends_as_it_starts() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Merged);
    let (link, _) = taking_worker(&hub);

    let (merged, _) = ask(&hub, &link, start("toolu_1")).await.unwrap();
    pull_requests.set(7, PullRequestState::Conflicting);
    let (conflicts, _) = ask(&hub, &link, start("toolu_2")).await.unwrap();

    assert_eq!((how(&merged), how(&conflicts)), (Some(WaitEnd::Merged), Some(WaitEnd::Conflicts)));
    assert!(hub.store.workspaces_waited_on().unwrap().is_empty());
}

#[tokio::test]
async fn a_wait_doesnt_start_when_githubs_limit_cant_spare_its_read() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    pull_requests.leave(10);
    let (link, _) = taking_worker(&hub);
    // Which GitHub's answer says leaves too little for an agent's.
    hub.workspace_pull_request("w".into()).await.unwrap();

    let refused = ask(&hub, &link, start("toolu_1")).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::Unavailable, "{refused:?}");
    assert!(hub.store.workspaces_waited_on().unwrap().is_empty(), "nothing waits");
}

#[tokio::test(start_paused = true)]
async fn a_wait_doesnt_start_when_its_read_doesnt_finish_before_the_worker_stops_waiting_to_hear() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github(dir.path()).await;
    pull_requests.set(7, PullRequestState::Open);
    pull_requests.hold();
    let (link, _) = taking_worker(&hub);
    let asked_at = tokio::time::Instant::now();

    let refused = ask(&hub, &link, start("toolu_1")).await;

    assert!(refused.is_err(), "{refused:?}");
    assert!(asked_at.elapsed() < Duration::from_mins(1), "within the minute a worker waits for an answer");
    assert!(hub.store.workspaces_waited_on().unwrap().is_empty(), "nothing waits");
}

#[tokio::test]
async fn a_worker_reads_a_wait_with_the_pull_request_as_last_read_and_only_for_its_own_chat() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.store.start_wait(&wait(WaitUntil::Done), Seen::default()).unwrap();
    hub.store.set_workspace_pull_request(&"w".into(), &PullRequestFound { pull_request: Some(open()) }, None).unwrap();
    let mut other = idle_session(1);
    other.session.id = "t".into();
    hub.store.insert_session(&other, None).unwrap();
    let link = WorkerLink::unconnected();
    let read = || WorkerAsk::Wait(ReadWait { wait: "a".into() });

    let (wait, pull_request) = ask(&hub, &link, read()).await.unwrap();
    let refused = hub.answer_ask(&"w".into(), &link, "t".into(), read()).await.unwrap_err();

    assert_eq!((wait.id.as_str(), wait.ended, pull_request), ("a", None, Some(open())));
    assert_eq!(refused.code, ErrorCode::NotFound, "another chat's wait: {refused:?}");
}

#[tokio::test]
async fn cancelling_a_wait_ends_it_once_and_a_wait_that_had_ended_stays_as_it_ended() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.store.start_wait(&wait(WaitUntil::Done), Seen::default()).unwrap();
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
        let change =
            SessionChange { status: Some(status), queue_held: None, recorded: None, initialized: None, claims: None };
        hub.store.append(&"s".into(), None, &listed, None, None, Some(change)).unwrap();

        hub.live_session(&"s".into()).await.unwrap();

        assert_eq!(hub.status().running_sessions, running, "{kind:?}");
    }
}

#[tokio::test]
async fn with_no_client_connected_only_a_workspace_waited_on_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let w = hub.store.get_workspace(&"w".into()).unwrap();
    // One that would come first, with an open pull request.
    let found = Some(PullRequestFound { pull_request: Some(open()) });
    let other = Workspace { id: "v".into(), name: "other".into(), instance_id: "v".into(), pull_request: found, ..w };
    hub.store.insert_workspace(&other, None).unwrap();
    let next = || async { hub.to_read_in_turn().await.first().map(|workspace| workspace.id.clone()) };

    let unwaited = next().await;
    hub.store.start_wait(&wait(WaitUntil::Done), Seen::default()).unwrap();
    let waited = next().await;
    hub.clients.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let seen = next().await;

    assert_eq!(unwaited, None, "nobody would see it");
    assert_eq!(waited, Some("w".into()), "not the other, which nobody would see either");
    assert_eq!(seen, Some("v".into()), "once a client would");
}

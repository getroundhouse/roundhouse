//! The person calling off a chat's wait, and a chat with no agent running whose waits are over.

use protocol::model::Session;

use super::*;

/// Has the chat `s` at work only on a wait, with no agent running, as after a hub started again, and returns it.
fn kept_across_a_restart(hub: &Hub, wait: &Wait) -> Session {
    kept_with_its_queue(hub, wait, None)
}

/// The same, with its queue held or not as `held` says, or as it was.
fn kept_with_its_queue(hub: &Hub, wait: &Wait, held: Option<bool>) -> Session {
    hub.store.start_wait(wait, Seen::default()).unwrap();
    let task = AgentTask { id: "t".into(), description: None, kind: TaskKind::Wait, call: None };
    let listed = EventBody::AgentTasks { tasks: vec![task.clone()], stopped: vec![] };
    let status = StatusChange { background_tasks: Some(vec![task]), ..SessionStatus::Background.into() };
    let change = SessionChange { status: Some(status), queue_held: held, recorded: None, initialized: None };
    hub.store.append(&"s".into(), None, &listed, None, None, Some(change)).unwrap();
    hub.store.get_session(&"s".into()).unwrap().session
}

fn at_rest(session: &Session) -> (SessionStatus, usize) {
    (session.status, session.background_tasks.len())
}

/// No agent is running to take the wait off the chat's list, so calling it off does, which leaves the chat idle, and
/// calling it off again changes nothing more.
#[tokio::test]
async fn calling_off_the_wait_of_a_chat_with_no_agent_running_leaves_it_idle() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    kept_across_a_restart(&hub, &wait(WaitUntil::Done));

    let called_off = hub.session_call_off_wait("s".into(), "a".into()).await.unwrap();
    let again = hub.session_call_off_wait("s".into(), "a".into()).await.unwrap();

    assert_eq!(at_rest(&called_off), (SessionStatus::Idle, 0));
    assert_eq!(how(&hub.store.wait(&"a".into()).unwrap()), Some(WaitEnd::CalledOffByPerson));
    assert_eq!(again.last_seq, called_off.last_seq, "nothing more was recorded");
}

/// Its wait has ended, but with no agent running, nothing took it off the chat's list, as when starting the agent to
/// hear of it failed. Stop takes it off, with nothing left to call off.
#[tokio::test]
async fn calling_off_a_wait_that_has_ended_still_takes_it_off_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    kept_across_a_restart(&hub, &wait(WaitUntil::Done));
    hub.store.end_wait(&"a".into(), WaitEnded { at_ms: 2, how: WaitEnd::Merged }).unwrap();

    let called_off = hub.session_call_off_wait("s".into(), "a".into()).await.unwrap();

    assert_eq!(at_rest(&called_off), (SessionStatus::Idle, 0));
    assert_eq!(how(&hub.store.wait(&"a".into()).unwrap()), Some(WaitEnd::Merged), "as it ended");
}

/// Stop sent for a wait the chat no longer has open, as after the agent started another, calls nothing off, and leaves
/// the open one listed, since its line is the CLI's copy of it.
#[tokio::test]
async fn calling_off_a_wait_that_isnt_open_leaves_the_one_that_is() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    kept_across_a_restart(&hub, &wait(WaitUntil::Done));

    let called_off = hub.session_call_off_wait("s".into(), "an older one".into()).await.unwrap();

    assert_eq!(at_rest(&called_off), (SessionStatus::Background, 1));
    assert_eq!(called_off.wait.map(|wait| wait.id), Some("a".into()));
}

/// A read ends the wait of a chat held for the person, so its agent isn't started to hear until they prompt it, and
/// meanwhile the wait comes off its list, rather than leave it at work, keeping the machine awake, on a wait that's
/// over.
#[tokio::test]
async fn a_wait_that_ends_with_nobody_to_hear_until_the_next_prompt_comes_off_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let says = says(Some(open()));
    reading(&hub, &says);
    kept_with_its_queue(&hub, &wait(WaitUntil::Done), Some(true));
    let session = hub.live_session(&"s".into()).await.unwrap();

    *says.lock() = Ok(Some(PullRequest { state: PrState::Merged, ..open() }));
    hub.workspace_pull_request("w".into()).await.unwrap();
    // Once the wake is done, since a chat carries out what it's asked in turn.
    let _ = hub.session_cancel("s".into()).await;

    let left = hub.store.get_session(&"s".into()).unwrap().session;
    assert_eq!((at_rest(&left), left.queue_held, session.agent_running()), ((SessionStatus::Idle, 0), true, false));
}

/// Closing the chat or archiving its workspace ends its wait, and with no agent running, the wait comes off its list
/// too, so it doesn't stay at work, keeping the machine awake, until the next hub starts.
#[tokio::test]
async fn a_chat_with_no_agent_running_goes_idle_as_closing_it_or_archiving_its_workspace_ends_its_wait() {
    for archiving in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        reading(&hub, &says(Some(open())));
        kept_across_a_restart(&hub, &wait(WaitUntil::Done));
        hub.live_session(&"s".into()).await.unwrap();

        if archiving {
            hub.workspace_archive("w".into(), true).await.unwrap();
        } else {
            hub.session_close("s".into()).await.unwrap();
        }

        let session = hub.store.get_session(&"s".into()).unwrap().session;
        assert_eq!(at_rest(&session), (SessionStatus::Idle, 0), "archiving: {archiving}");
        assert!(!hub.status().keeping_awake, "archiving: {archiving}");
    }
}

//! A workspace's setup, as the hub records it, and what waits on it: a chat's prompts, which reach its agent only once
//! setup has finished, whatever became of it. Running the script, and stopping it, are the worker's, which the daemon's
//! `e2e/setup.rs` has a real one do.

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::event::EventBody;
use protocol::model::{Seq, SessionId, SetupStatus, Workspace};
use protocol::prompt::Prompt;
use protocol::worker::{SetupDone, WorkerRequest, WorkerResponse};

use crate::hub::Hub;
use crate::hub::stand_in_workers::{Asked, TIMEOUT, a_chat, a_turn, answering_first, hub_with_stand_in_workers};

/// How long a test gives the hub to launch an agent it mustn't yet, which with a stand-in for the worker it would do
/// in a few milliseconds.
const NOT_LAUNCHED_WITHIN: Duration = Duration::from_millis(300);

/// Runs the workspace's setup again, as the person can, held until the test sends how it ended, and has another
/// stand-in take over as the workspace's worker for everything asked after it, so what the hub asks meanwhile is seen
/// as it's asked. The setup holds the first stand-in's task, as a worker's setup holds its script, which only a test
/// on more than one thread can do, and fails after twice the test's limit, so a test that never ends it fails rather
/// than hangs.
async fn setup_held(hub: &Arc<Hub>, asked: &Arc<Asked>, workspace: &Workspace) -> Sender<SetupDone> {
    waiting::task::until(TIMEOUT, "the first setup to finish", || hub.setup_gates.lock().is_empty()).await;
    let (end, ended) = std::sync::mpsc::channel();
    answering_first(hub, &workspace.id, asked, move |request| {
        let WorkerRequest::SetupWorkspace(_) = request else { return None };
        let held = tokio::task::block_in_place(|| ended.recv_timeout(TIMEOUT * 2));
        let never = |_| ApiError::new(ErrorCode::Worker, "the test never let setup finish");
        Some(held.map(WorkerResponse::SetupDone).map_err(never))
    });
    hub.workspace_rerun_setup(workspace.id.clone()).await.unwrap();
    let setups = || asked.requests(|request| matches!(request, WorkerRequest::SetupWorkspace(_)).then_some(())).len();
    waiting::task::until(TIMEOUT, "the stand-in to take the setup", || setups() == 2).await;
    answering_first(hub, &workspace.id, asked, |_| None);
    end
}

fn log(hub: &Hub, chat: &SessionId) -> Vec<EventBody> {
    let events = hub.store.read_events(chat, Seq::FIRST, 1000, u64::MAX, false).unwrap();
    events.into_iter().map(|event| event.body).collect()
}

/// It's recorded as it's sent, and answered at once, and its agent is launched only once setup has finished, so the
/// agent doesn't start in a half-made worktree.
#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_sent_during_setup_is_recorded_at_once_and_given_once_setup_has_finished() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    let end = setup_held(&hub, &asked, &workspace).await;

    let prompting = hub.session_prompt(chat.id.clone(), None, Prompt::new("go"));
    let (seq, _) = tokio::time::timeout(TIMEOUT, prompting).await.expect("the prompt waited for setup").unwrap();
    let recorded = log(&hub, &chat.id);
    let launched = waiting::task::up_to(NOT_LAUNCHED_WITHIN, || !asked.launches(&chat.id).is_empty()).await;
    let setup_meanwhile = hub.store.get_workspace(&workspace.id).unwrap().setup;
    end.send(SetupDone { code: Some(0) }).unwrap();
    let given = || asked.prompts_given(&chat.id).contains(&seq);
    waiting::task::until(TIMEOUT, "the prompt to be given once setup had finished", given).await;

    assert_eq!(seq, Seq::new(1));
    assert!(matches!(recorded.as_slice(), [EventBody::UserPrompt { .. }]), "recorded at once: {recorded:?}");
    assert_eq!(setup_meanwhile, SetupStatus::Running);
    assert!(!launched, "an agent was launched before setup had finished");
}

/// Commands run one at a time, so once its prompt is dropped, the chat has dealt with it.
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_chat_with_a_prompt_waiting_on_setup_never_starts_its_agent() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    let end = setup_held(&hub, &asked, &workspace).await;
    let (seq, _) = hub.session_prompt(chat.id.clone(), None, Prompt::new("go")).await.unwrap();

    tokio::time::timeout(TIMEOUT, hub.session_close(chat.id.clone())).await.expect("closing waited").unwrap();
    end.send(SetupDone { code: Some(0) }).unwrap();

    let dropped = || log(&hub, &chat.id).contains(&EventBody::PromptDropped { prompt: seq });
    let launched = || !asked.launches(&chat.id).is_empty();
    waiting::task::until(TIMEOUT, "the prompt to be dropped, or given", || dropped() || launched()).await;
    assert!(!launched(), "an agent was launched for a closed chat");
    assert!(dropped(), "the prompt is dropped");
    let too_late = hub.session_prompt(chat.id.clone(), None, Prompt::new("too late")).await;
    assert_eq!(too_late.map_err(|e| e.code).err(), Some(ErrorCode::Conflict));
}

/// The workspace is left usable, and its chat says once that setup didn't finish, since every prompt after the first
/// would say the same.
#[tokio::test]
async fn a_failed_setup_is_recorded_with_its_code_and_a_chat_is_warned_once() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    waiting::task::until(TIMEOUT, "the first setup to finish", || hub.setup_gates.lock().is_empty()).await;
    answering_first(&hub, &workspace.id, &asked, |request| {
        let WorkerRequest::SetupWorkspace(_) = request else { return None };
        Some(Ok(WorkerResponse::SetupDone(SetupDone { code: Some(7) })))
    });

    hub.workspace_rerun_setup(workspace.id.clone()).await.unwrap();
    waiting::task::until(TIMEOUT, "setup to fail", || hub.setup_gates.lock().is_empty()).await;
    a_turn(&hub, &asked, &chat.id, Prompt::new("one"), None).await;
    a_turn(&hub, &asked, &chat.id, Prompt::new("two"), None).await;

    assert_eq!(hub.store.get_workspace(&workspace.id).unwrap().setup, SetupStatus::Failed { code: Some(7) });
    let warned = |body: &&EventBody| matches!(body, EventBody::Notice { message, .. } if message.contains("setup"));
    assert_eq!(log(&hub, &chat.id).iter().filter(warned).count(), 1);
    assert_eq!(asked.prompts_given(&chat.id).len(), 2, "both prompts reached the agent");
}

/// What setup made went with the worktree, since none of it is committed, so a restored workspace runs it again, once
/// its worktree is back.
#[tokio::test]
async fn restoring_runs_setup_again_once_the_worktree_is_back() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, _) = a_chat(&hub).await;
    waiting::task::until(TIMEOUT, "the first setup to finish", || hub.setup_gates.lock().is_empty()).await;

    hub.workspace_archive(workspace.id.clone(), false).await.unwrap();
    // Archiving closed its worker, and a stand-in has no command to start another by, so one takes its place.
    answering_first(&hub, &workspace.id, &asked, |_| None);
    hub.workspace_restore(workspace.id.clone()).await.unwrap();
    waiting::task::until(TIMEOUT, "setup to run again", || hub.setup_gates.lock().is_empty()).await;

    let moves = asked.requests(|request| {
        let named = [
            (matches!(request, WorkerRequest::SetupWorkspace(_)), "setup"),
            (matches!(request, WorkerRequest::RemoveWorkspace(_)), "remove"),
            (matches!(request, WorkerRequest::RestoreWorkspace(_)), "restore"),
        ];
        named.into_iter().find_map(|(asked, name)| asked.then_some(name))
    });
    assert_eq!(moves, ["setup", "remove", "restore", "setup"]);
    assert_eq!(hub.store.get_workspace(&workspace.id).unwrap().setup, SetupStatus::Succeeded);
}

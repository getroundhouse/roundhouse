//! A prompt sent to a chat whose workspace's worker can't be reached: recorded at once, and given to the agent once the
//! worker is back, as one sent during setup is given once setup has ended.

use std::time::Duration;

use protocol::event::EventBody;
use protocol::model::{Seq, SessionId};
use protocol::prompt::Prompt;

use crate::hub::Hub;
use crate::hub::carrying_on::Cause;
use crate::hub::stand_in_workers::{TIMEOUT, a_chat, hub_with_stand_in_workers};

fn log(hub: &Hub, chat: &SessionId) -> Vec<protocol::event::SessionEvent> {
    hub.store.read_events(chat, Seq::FIRST, 1000, u64::MAX, false).unwrap()
}

#[tokio::test]
async fn a_prompt_sent_while_the_worker_is_disconnected_is_recorded_and_given_to_the_agent_once_it_is_back() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, true, None);

    let (seq, _) = hub.session_prompt(chat.id.clone(), None, Prompt::new("while you were away"), None).await.unwrap();
    waiting::task::for_what_mustnt_happen(Duration::from_millis(300)).await;
    let waiting_events = log(&hub, &chat.id);
    let launched_while_away = asked.launches(&chat.id).len();
    hub.disconnected.clear(&workspace.id);
    waiting::task::until(TIMEOUT, "the prompt to be given", || asked.prompts_given(&chat.id).contains(&seq)).await;

    assert!(
        waiting_events.iter().any(|e| matches!(e.body, EventBody::UserPrompt { .. })),
        "the prompt is recorded as it's sent"
    );
    assert!(
        !waiting_events.iter().any(|e| matches!(e.body, EventBody::PromptDropped { .. })),
        "and not dropped: {waiting_events:#?}"
    );
    assert_eq!(launched_while_away, 0, "nothing was started on a worker that couldn't be reached");
}

/// A stop for idleness ends each workspace's worker, so it leaves a hub alone that can't reach one, which may be at work.
#[tokio::test]
async fn a_hub_that_cant_reach_a_workspaces_worker_isnt_idle() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (workspace, _) = a_chat(&hub).await;
    // Its setup, which a new workspace has, has to end first.
    waiting::task::until(TIMEOUT, "the hub to be idle", || hub.status().is_idle()).await;
    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, true, None);

    let refused = hub.shutdown_if_idle();

    assert!(hub.status().is_idle(), "nothing else is at work");
    assert_eq!(refused.unwrap_err().code, protocol::client::ErrorCode::Conflict);
}

/// So a chat whose agent was on a connection that dropped records its loss once the worker is found gone, though it's
/// idle, and once only.
#[tokio::test]
async fn a_chat_with_an_idle_agent_on_a_dropped_connection_records_its_loss_once() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    crate::hub::stand_in_workers::a_turn(&hub, &asked, &chat.id, Prompt::new("look into it"), None).await;
    let link = hub.workers.lock().get(&workspace.id).cloned().unwrap();
    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, true, Some(link.id));
    let lost = |hub: &Hub| {
        log(hub, &chat.id)
            .iter()
            .filter(|e| {
                matches!(e.body, EventBody::AgentExited { reason: protocol::event::ExitReason::WorkerLost, .. })
            })
            .count()
    };

    let told = hub.tell_agents_lost(&workspace.id).await;
    let told_again = hub.tell_agents_lost(&workspace.id).await;

    assert_eq!((told.len(), told_again.len(), lost(&hub)), (1, 0, 1));
}

/// A prompt waiting for setup to end holds up the next from being recorded and answered, and a prompt queued meanwhile
/// is answered at once too. Both sent reach the agent in the order they were sent once setup has ended.
#[tokio::test]
async fn a_prompt_waiting_for_setup_holds_up_neither_the_next_prompt_nor_a_queued_one() {
    use crate::hub::setup::SetupGate;
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    waiting::task::until(TIMEOUT, "the first setup to finish", || hub.setup_gates.lock().is_empty()).await;
    hub.setup_gates.lock().insert(workspace.id.clone(), SetupGate::running());
    let sent = |text: &'static str| hub.session_prompt(chat.id.clone(), None, Prompt::new(text), None);

    let quickly = Duration::from_secs(2);
    let first = tokio::time::timeout(quickly, sent("first")).await.expect("the first prompt was answered");
    let second = tokio::time::timeout(quickly, sent("second")).await.expect("the second prompt was answered");
    let queued =
        tokio::time::timeout(quickly, hub.session_queue(chat.id.clone(), "q".into(), Prompt::new("later"), None)).await;

    let (first, second) = (first.unwrap().0, second.unwrap().0);
    assert!(queued.is_ok(), "a queued prompt was answered");
    let recorded = log(&hub, &chat.id).iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(recorded, 2);
    assert!(asked.prompts_given(&chat.id).is_empty(), "nothing is given while setup runs");
    hub.setup_gates.lock().remove(&workspace.id);
    waiting::task::until(TIMEOUT, "both prompts to be given", || asked.prompts_given(&chat.id).len() == 2).await;
    assert_eq!(asked.prompts_given(&chat.id), [first, second], "in the order they were sent");
}

/// The same while the worker is disconnected.
#[tokio::test]
async fn prompts_sent_while_the_worker_is_disconnected_are_each_recorded_at_once_and_reach_the_agent_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    waiting::task::until(TIMEOUT, "the first setup to finish", || hub.setup_gates.lock().is_empty()).await;
    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, true, None);
    let sent = |text: &'static str| hub.session_prompt(chat.id.clone(), None, Prompt::new(text), None);

    let quickly = Duration::from_secs(2);
    let first = tokio::time::timeout(quickly, sent("first")).await.expect("the first prompt was answered");
    let second = tokio::time::timeout(quickly, sent("second")).await.expect("the second prompt was answered");

    let (first, second) = (first.unwrap().0, second.unwrap().0);
    assert!(asked.prompts_given(&chat.id).is_empty());
    hub.disconnected.clear(&workspace.id);
    waiting::task::until(TIMEOUT, "both prompts to be given", || asked.prompts_given(&chat.id).len() == 2).await;
    assert_eq!(asked.prompts_given(&chat.id), [first, second]);
}

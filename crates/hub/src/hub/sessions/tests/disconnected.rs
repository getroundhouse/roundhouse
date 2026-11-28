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
    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, true);

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
    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, true);

    let refused = hub.shutdown_if_idle();

    assert!(hub.status().is_idle(), "nothing else is at work");
    assert_eq!(refused.unwrap_err().code, protocol::client::ErrorCode::Conflict);
}

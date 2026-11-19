//! A prompt the agent can't take into the turn it's on, which the hub sends once that turn ends: one a run refuses while
//! its turn runs, and any sent after it.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::capability::Control;
use protocol::event::{EventBody, TurnOutcome};
use protocol::model::Seq;
use protocol::prompt::Prompt;
use protocol::worker::{WorkerRequest, WorkerResponse};

use super::turns::Chat;
use crate::hub::stand_in_workers::answering_first;

/// A chat whose agent, once started, refuses the second prompt it's sent the first time it's sent it, as Codex refuses
/// one while its turn can't be steered, such as a review.
async fn a_chat_that_refuses_a_steer(dir: &std::path::Path) -> Chat {
    let chat = Chat::idle(dir).await;
    let sent: Arc<Mutex<Vec<Seq>>> = Arc::default();
    answering_first(&chat.hub, &chat.workspace.id, &chat.asked, move |request| {
        let WorkerRequest::AgentControl(asked) = request else { return None };
        let Control::Prompt { seq, .. } = asked.control else { return None };
        let mut sent = sent.lock();
        if sent.contains(&seq) {
            return None;
        }
        sent.push(seq);
        (sent.len() == 2).then_some(Ok(WorkerResponse::AfterTheTurn))
    });
    chat
}

/// A run that refuses a prompt while its turn runs took nothing, so the hub sends it again once the turn ends, and one
/// sent after it, which the run would take, waits to follow it.
#[tokio::test]
async fn a_prompt_a_run_refuses_while_its_turn_runs_goes_again_once_the_turn_ends_and_the_next_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let chat = a_chat_that_refuses_a_steer(dir.path()).await;
    let (work, _) = chat.hub.session_prompt(chat.id.clone(), None, Prompt::new("work")).await.unwrap();
    assert_eq!(chat.given(1).await, [work]);
    chat.says([EventBody::PromptTaken { prompt: work }]).await;

    let (refused, _) = chat.hub.session_prompt(chat.id.clone(), None, Prompt::new("refused")).await.unwrap();
    let (later, _) = chat.hub.session_prompt(chat.id.clone(), None, Prompt::new("later")).await.unwrap();
    chat.caught_up().await;
    let meanwhile = chat.asked.prompts_given(&chat.id);
    chat.finishes(None, TurnOutcome::Success).await;
    let after = chat.given(4).await;

    assert_eq!(meanwhile, [work, refused], "refused, and the next kept to follow it");
    assert_eq!(after, [work, refused, refused, later], "both, in the order they were recorded");
    assert!(chat.log().iter().all(|e| !matches!(e.body, EventBody::PromptDropped { .. })));
}

/// A stopped turn leaves it to the person whether to go on, as for the queue, so what waited for its end is dropped,
/// and never reaches the agent.
#[tokio::test]
async fn a_stopped_turn_drops_what_waited_for_its_end() {
    let dir = tempfile::tempdir().unwrap();
    let chat = a_chat_that_refuses_a_steer(dir.path()).await;
    let (work, _) = chat.hub.session_prompt(chat.id.clone(), None, Prompt::new("work")).await.unwrap();
    assert_eq!(chat.given(1).await, [work]);
    chat.says([EventBody::PromptTaken { prompt: work }]).await;
    let (next, _) = chat.hub.session_prompt(chat.id.clone(), None, Prompt::new("next")).await.unwrap();

    chat.finishes(None, TurnOutcome::Cancelled).await;
    chat.caught_up().await;

    assert_eq!(chat.asked.prompts_given(&chat.id), [work, next], "refused, and never sent again");
    let dropped = chat.log().iter().any(|e| matches!(e.body, EventBody::PromptDropped { prompt } if prompt == next));
    assert!(dropped, "and the log says so");
    assert!(chat.record().untaken.unwrap_or_default().is_empty(), "so nothing's left for the agent to take");
}

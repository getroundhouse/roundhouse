//! A chat whose agent takes what it's given and ends its turn as a test says, and what the chat's log and record say
//! of it.

use std::path::Path;
use std::sync::Arc;

use protocol::client::ApiError;
use protocol::event::{AgentRunState, EventBody, SessionEvent, TurnOutcome};
use protocol::fixtures::turn_ended;
use protocol::model::{AttachmentId, QueuedPrompt, Seq, Session, SessionId, Workspace};
use protocol::prompt::Prompt;

use crate::hub::Hub;
use crate::hub::stand_in_workers::{Asked, TIMEOUT, a_chat, hub_with_stand_in_workers, the_agent_says};

/// A chat in a workspace of its own, on a hub whose workers are stand-ins, and what they've been asked.
pub(super) struct Chat {
    pub(super) hub: Arc<Hub>,
    pub(super) asked: Arc<Asked>,
    pub(super) workspace: Workspace,
    pub(super) id: SessionId,
}

impl Chat {
    /// One that has been sent nothing.
    pub(super) async fn idle(dir: &Path) -> Self {
        let (hub, asked) = hub_with_stand_in_workers(dir);
        let (workspace, chat) = a_chat(&hub).await;
        Self { hub, asked, workspace, id: chat.id }
    }

    /// One whose agent is at work on the prompt the chat was sent, which it took.
    pub(super) async fn at_work(dir: &Path) -> Self {
        let chat = Self::idle(dir).await;
        let (seq, _) = chat.hub.session_prompt(chat.id.clone(), None, Prompt::new("work"), None).await.unwrap();
        assert_eq!(chat.given(1).await, [seq]);
        chat.says([EventBody::PromptTaken { prompt: seq }]).await;
        chat
    }

    /// Where each prompt the agent has been given is recorded, once it has been given `count`.
    pub(super) async fn given(&self, count: usize) -> Vec<Seq> {
        let given = || {
            let given = self.asked.prompts_given(&self.id);
            if given.len() >= count { Ok(given) } else { Err(format!("the agent was given only {given:?}")) }
        };
        waiting::task::until_ok(TIMEOUT, given).await
    }

    /// Once the chat has done all it was asked to before, such as sending what was ready in its queue, since a chat
    /// carries out what it's asked in turn. Asking it to send a prompt that isn't queued does nothing else.
    pub(super) async fn caught_up(&self) {
        self.hub.session_send_queued(self.id.clone(), "not queued".into()).await.unwrap();
    }

    pub(super) async fn says(&self, said: impl IntoIterator<Item = EventBody>) {
        the_agent_says(&self.hub, &self.asked, &self.id, said).await;
    }

    /// The agent takes the prompt at `taking`, if there's one, then ends its turn with `outcome` and says it's idle, as
    /// Claude's CLI does.
    pub(super) async fn finishes(&self, taking: Option<Seq>, outcome: TurnOutcome) {
        let taken = taking.map(|prompt| EventBody::PromptTaken { prompt });
        let idle = EventBody::AgentState { state: AgentRunState::Idle };
        self.says(taken.into_iter().chain([turn_ended(outcome), idle])).await;
    }

    pub(super) async fn queue(&self, id: &str, prompt: Prompt<AttachmentId>) -> Result<Session, ApiError> {
        self.hub.session_queue(self.id.clone(), id.into(), prompt, None).await
    }

    pub(super) async fn change(&self, id: &str, text: &str) -> Result<Session, ApiError> {
        self.hub.session_change_queued(self.id.clone(), id.into(), text.to_string()).await
    }

    pub(super) fn record(&self) -> Session {
        self.hub.store.get_session(&self.id).unwrap().session
    }

    pub(super) fn log(&self) -> Vec<SessionEvent> {
        self.hub.store.read_events(&self.id, Seq::FIRST, 1000, u64::MAX, false).unwrap()
    }
}

/// Where the prompt queued under `id` was recorded, once it has been.
pub(super) fn sent_at(events: &[SessionEvent], id: &str) -> Option<Seq> {
    events.iter().find_map(|e| {
        let EventBody::UserPrompt { client_msg_id: Some(sent), .. } = &e.body else { return None };
        (sent.as_str() == id).then_some(e.seq)
    })
}

pub(super) fn turn_ends(events: &[SessionEvent]) -> Vec<Seq> {
    events.iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).map(|e| e.seq).collect()
}

pub(super) fn ids(queued: &[QueuedPrompt]) -> Vec<&str> {
    queued.iter().map(|q| q.client_msg_id.as_str()).collect()
}

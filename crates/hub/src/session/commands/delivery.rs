//! Getting what was recorded to the agent, in the order it was recorded, on a task of the chat's own, so waiting for the
//! agent to be reachable holds up nothing else the chat is asked.
//!
//! A chat's commands each have the chat's one command task record what they ask, answer the client, and hand what
//! has to reach the agent on to this task, which carries it out one at a time, in the order it was handed on. It waits
//! for setup to end and for the workspace's worker to be connected before it hands a prompt over, starts the agent if
//! it has to, as [`LiveSession::ensure_agent`] does, and goes on to the next once that one's been handed over.
//!
//! The line of what's to reach the agent is the log, with the store's queue for what's queued, not this task's: a
//! prompt is recorded the moment it's sent, and stays among the session's untaken ones, oldest first, until the agent
//! takes or drops it. What's handed on here is only each one's place in the log, its `seq`, which the task reads the
//! prompt back by when it's its turn, so nothing but what it's handing over now is its own. A prompt dropped meanwhile,
//! as one is when the agent is lost, is no longer untaken, so it's skipped.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::event::EventBody;
use protocol::model::Seq;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, warn};

use super::super::{LiveSession, Origin};
use super::requests::Asking;
use crate::hub::Hub;
use crate::store::Store;

/// What the command task hands the delivery task, each after what was handed on before it.
pub(in super::super) enum Outgoing {
    /// The prompt, or the request to compact, recorded at `seq`.
    Deliver { seq: Seq },
    /// Starts the agent, if it's waiting on nothing but a wait, as [`LiveSession::handle_wake_agent`] says.
    WakeAgent,
    /// Tells the run how the person talks with the agent, if that changed.
    TellTheMode,
    /// Asks the run to connect again to each tool server that's due.
    ReconnectToolServers,
    /// Sends what was kept for the turn's end, which has come.
    AfterTheTurn,
    /// Cancels the agent's turn, once what was handed on before it has been.
    Cancel(oneshot::Sender<Result<(), ApiError>>),
    /// Stops the agent, once what was handed on before it has been.
    Stop(oneshot::Sender<Result<(), ApiError>>),
    /// Resolves once everything handed on before it has been carried out, for a test to look at what that did.
    #[cfg(test)]
    Flush(oneshot::Sender<()>),
}

pub(crate) async fn run_delivery(session: Arc<LiveSession>, mut outgoing: mpsc::UnboundedReceiver<Outgoing>) {
    loop {
        let next = tokio::select! {
            next = outgoing.recv() => next,
            () = session.until_ended() => None,
        };
        let Some(next) = next else { return };
        let carrying_out = async {
            match next {
                Outgoing::Deliver { seq } => session.deliver_recorded(seq).await,
                Outgoing::WakeAgent => session.handle_wake_agent().await,
                Outgoing::TellTheMode => session.tell_the_mode().await,
                Outgoing::ReconnectToolServers => session.reconnect_tool_servers().await,
                Outgoing::AfterTheTurn => {
                    session.reconnect_tool_servers().await;
                    session.after_the_turn().await;
                }
                Outgoing::Cancel(reply) => drop(reply.send(session.handle_cancel().await)),
                Outgoing::Stop(reply) => drop(reply.send(session.handle_stop().await)),
                #[cfg(test)]
                Outgoing::Flush(done) => drop(done.send(())),
            }
        };
        tokio::select! {
            () = carrying_out => {}
            () = session.until_ended() => return,
        }
    }
}

impl LiveSession {
    /// Hands `next` to the delivery task, after what was handed to it before.
    pub(in super::super) fn hand_over(&self, next: Outgoing) {
        if self.outgoing.send(next).is_err() {
            warn!(name: "session.delivery_stopped", session_id = %self.id, "Handed on to a chat that's stopped");
        }
    }

    /// Returns once everything handed to the delivery task so far has been carried out.
    #[cfg(test)]
    pub(crate) async fn delivered(&self) {
        let (done, delivered) = oneshot::channel();
        self.hand_over(Outgoing::Flush(done));
        let _ = delivered.await;
    }

    /// Gets the prompt recorded at `seq` to the agent, once setup has ended and the worker is connected, unless the
    /// agent has taken or dropped it since. Failures go in the log, and so does what never reaches an agent, since no
    /// agent will say so.
    async fn deliver_recorded(&self, seq: Seq) {
        let Ok(hub) = self.hub() else { return };
        hub.wait_for_setup(&self.workspace_id).await;
        // A worker whose connection dropped is waited for, and the prompt, recorded, reaches its agent once it's back.
        hub.wait_until_connected(&self.workspace_id).await;
        if !self.state.lock().progress.queue.untaken().contains(&seq) {
            return;
        }
        // Before it, as a tool server the person has signed in to may not have reached the run, which then has it. It
        // isn't handed over yet, so it starts no turn that would put that off.
        self.reconnect_tool_servers().await;
        self.state.lock().handed_through = Some(seq);
        let asking = match self.read_asking(&hub, seq).await {
            Ok(asking) => asking,
            Err(e) => {
                error!(name: "session.prompt_unread", session_id = %self.id, error = %e, "Couldn't read a prompt back");
                return self.record_or_log(Origin::Hub, EventBody::PromptDropped { prompt: seq }).await;
            }
        };
        self.deliver(&hub, seq, asking).await;
    }

    /// What the log says was asked at `seq`.
    async fn read_asking(&self, hub: &Arc<Hub>, seq: Seq) -> Result<Asking, protocol::client::ApiError> {
        let id = self.id.clone();
        let read = move |store: &Store| store.read_events(&id, seq, 1, u64::MAX, false);
        let events = hub.blocking(read).await?;
        match events.into_iter().find(|event| event.seq == seq).map(|event| event.body) {
            Some(EventBody::UserPrompt { prompt, .. }) => Ok(Asking::Prompt(prompt)),
            Some(EventBody::CompactionAsked { keeping }) => Ok(Asking::Compact { keeping }),
            _ => Err(protocol::client::ApiError::new(
                protocol::client::ErrorCode::Internal,
                format!("event {seq} isn't something the agent is asked"),
            )),
        }
    }
}

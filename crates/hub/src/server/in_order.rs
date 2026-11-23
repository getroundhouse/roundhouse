//! What a connection takes in the order it was sent: what's pressed and new sizes, which go on to the terminal's
//! worker before the next request is read, and what it subscribes to, which one task of the connection's takes in turn
//! while the next requests are read and answered.

use std::sync::Arc;

use protocol::client::subscriptions::Subscribe;
use protocol::client::{ClientRequest, ClientResponse, HubFrame, ReplyOutcome};
use tokio::sync::mpsc;

use super::Connection;
use super::counted::Counted;
use super::subscribe::subscribe;
use crate::error::internal;
use crate::hub::{Hub, Queued};

/// How a request the connection read is answered.
pub(super) enum Answering {
    /// Once what's queued is done, such as the worker answering what was sent it already.
    Queued(Queued),
    /// On a task of its own, as the request, or why it couldn't be read.
    OnATask(Result<ClientRequest, serde_json::Error>),
}

/// What's pressed and new sizes, sent on to the terminal's worker as they're read, rather than on a task of their own,
/// since typing out of order isn't what was typed. Their replies come once that's done.
pub(super) async fn in_order(hub: &Arc<Hub>, request: Result<ClientRequest, serde_json::Error>) -> Answering {
    let queued = match request {
        Ok(ClientRequest::TerminalTyped(typed)) => hub.terminal_typed(typed).await,
        Ok(ClientRequest::TerminalFit(fit)) => hub.terminal_fit(fit).await,
        request => return Answering::OnATask(request),
    };
    Answering::Queued(queued.unwrap_or_else(|refused| Box::pin(async move { Err(refused) })))
}

/// One `Subscribe` a connection read: its call's ID, what it asks, and its place among the requests under way until
/// its reply is queued.
struct InTurn {
    id: u64,
    subscribe: Subscribe,
    under_way: Counted,
}

/// A connection's `Subscribe` requests, which one task takes one at a time, in the order they were read, and replies to
/// each, so a stop sent straight after its start, without waiting for the start's reply, can't leave it following. A
/// start waits for its level's list, so only the connection's next `Subscribe` waits on it, and what else the
/// connection sends is read and answered meanwhile. A list a start queues goes before its reply, which a client takes
/// to mean the hub has that level.
pub(super) struct Subscribes {
    queue: mpsc::UnboundedSender<InTurn>,
}

impl Subscribes {
    /// Starts the task that takes `connection`'s, which ends once this is dropped and it has taken each one queued.
    pub(super) fn start(hub: &Arc<Hub>, connection: &Arc<Connection>) -> Self {
        let (queue, mut queued) = mpsc::unbounded_channel::<InTurn>();
        let (hub, connection) = (hub.clone(), connection.clone());
        tokio::spawn(async move {
            while let Some(InTurn { id, subscribe: asked, under_way }) = queued.recv().await {
                let subscribed = subscribe(&hub, &connection, asked).await;
                drop(under_way);
                let outcome = ReplyOutcome::Response(ClientResponse::Subscribed(subscribed));
                // An error only means the connection has gone.
                let _ = connection.frames.send(HubFrame::Reply { id, outcome }.into()).await;
            }
        });
        Self { queue }
    }

    /// Queues `asked`, call `id`, after those read before it, counted among the requests under way by `under_way`.
    pub(super) async fn queue(&self, connection: &Connection, id: u64, asked: Subscribe, under_way: Counted) {
        if self.queue.send(InTurn { id, subscribe: asked, under_way }).is_err() {
            let error = internal("the task that takes the connection's subscriptions has ended");
            let _ = connection.frames.send(HubFrame::Reply { id, outcome: ReplyOutcome::Error(error) }.into()).await;
        }
    }
}

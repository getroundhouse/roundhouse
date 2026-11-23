//! What a connection takes in the order it was sent, before it reads the next request: what's pressed and new sizes,
//! which go on to the terminal's worker, and what it subscribes to.

use std::sync::Arc;

use protocol::client::{ClientRequest, ClientResponse};

use super::Connection;
use super::subscribe::subscribe;
use crate::hub::{Hub, Queued};

/// How a request the connection read is answered.
pub(super) enum Answering {
    /// Once what's queued is done, such as the worker answering what was sent it already.
    Queued(Queued),
    /// On a task of its own, as the request, or why it couldn't be read.
    OnATask(Result<ClientRequest, serde_json::Error>),
}

/// What's pressed and new sizes, sent on to the terminal's worker as they're read, rather than on a task of their own,
/// since typing out of order isn't what was typed. And what a client starts and stops following, since a stop sent
/// after a start, without waiting for the start's reply, would otherwise leave it following. Their replies come once
/// that's done.
pub(super) async fn in_order(
    hub: &Arc<Hub>,
    connection: &Arc<Connection>,
    request: Result<ClientRequest, serde_json::Error>,
) -> Answering {
    let queued = match request {
        Ok(ClientRequest::TerminalTyped(typed)) => hub.terminal_typed(typed).await,
        Ok(ClientRequest::TerminalFit(fit)) => hub.terminal_fit(fit).await,
        Ok(ClientRequest::Subscribe(asked)) => {
            let subscribed = subscribe(hub, connection, asked).await;
            return Answering::Queued(Box::pin(async move { Ok(ClientResponse::Subscribed(subscribed)) }));
        }
        request => return Answering::OnATask(request),
    };
    Answering::Queued(queued.unwrap_or_else(|refused| Box::pin(async move { Err(refused) })))
}

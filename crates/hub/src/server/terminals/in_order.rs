//! Sending on what's pressed and new sizes in the order they came.

use protocol::client::ClientRequest;

use crate::hub::{Hub, Queued};

/// How a request the connection read is answered.
pub(in crate::server) enum Answering {
    /// Once the worker has answered what was sent it already.
    Queued(Queued),
    /// On a task of its own, as the request, or why it couldn't be read.
    OnATask(Result<ClientRequest, serde_json::Error>),
}

/// What's pressed and new sizes, sent on to the terminal's worker as they're read, rather than on a task of their own,
/// since typing out of order isn't what was typed. Their replies come once the worker's have.
pub(in crate::server) async fn in_order(hub: &Hub, request: Result<ClientRequest, serde_json::Error>) -> Answering {
    let queued = match request {
        Ok(ClientRequest::TerminalTyped(typed)) => hub.terminal_typed(typed).await,
        Ok(ClientRequest::TerminalFit(fit)) => hub.terminal_fit(fit).await,
        request => return Answering::OnATask(request),
    };
    Answering::Queued(queued.unwrap_or_else(|refused| Box::pin(async move { Err(refused) })))
}

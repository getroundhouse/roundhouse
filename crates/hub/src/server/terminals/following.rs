//! Following a terminal for a client that attaches, and sending on what's typed and new sizes in the order they came.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::terminals::TerminalAttach;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame};
use protocol::model::TerminalId;
use protocol::terminal::{TerminalExit, TerminalUpdate};
use protocol::worker::TerminalShown;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

use crate::hub::{Hub, Numbered, Queued};
use crate::server::Connection;
use crate::server::subscribing::add_subscription;

/// How long to wait before asking again what a terminal shows, when its worker couldn't say. Long enough not to keep a
/// worker that can't busy, and short enough that a client sees the terminal soon after it can.
const SCREEN_AGAIN_AFTER: Duration = Duration::from_secs(1);

/// How a request the connection read is answered.
pub(in crate::server) enum Answering {
    /// Once the worker has answered what was sent it already.
    Queued(Queued),
    /// On a task of its own, as the request, or why it couldn't be read.
    OnATask(Result<ClientRequest, serde_json::Error>),
}

/// Typing and resizing, sent on to the terminal's worker as they're read, rather than on a task of their own, since
/// typing out of order isn't what was typed. Their replies come once the worker's have.
pub(in crate::server) async fn in_order(hub: &Hub, request: Result<ClientRequest, serde_json::Error>) -> Answering {
    let queued = match request {
        Ok(ClientRequest::TerminalInput(typed)) => hub.terminal_input(typed).await,
        Ok(ClientRequest::TerminalResize(resize)) => hub.terminal_resize(resize).await,
        request => return Answering::OnATask(request),
    };
    Answering::Queued(queued.unwrap_or_else(|refused| Box::pin(async move { Err(refused) })))
}

/// Starts following the terminal, for subscription `subscription_id`.
pub(in crate::server) fn attach(
    hub: &Arc<Hub>,
    connection: &Connection,
    attach: TerminalAttach,
) -> Result<ClientResponse, ApiError> {
    let TerminalAttach { id, subscription_id } = attach;
    let (link, updates) = hub.terminal_follow(&id)?;
    add_subscription(connection, subscription_id, || {
        let attached = Attached { subscription_id, frames: connection.frames.clone() };
        tokio::spawn(attached.follow(hub.clone(), link, id, updates)).abort_handle()
    })?;
    Ok(ClientResponse::Ok)
}

/// A client attached to a terminal.
pub(super) struct Attached {
    pub(super) subscription_id: u64,
    pub(super) frames: mpsc::Sender<HubFrame>,
}

impl Attached {
    /// Sends what the terminal shows, from the worker on link `link`, then each update after it, until its shell ends
    /// or the client stops following it. A client too far behind to be sent every update is sent what it shows again
    /// instead. `updates` has to have been subscribed to before, so none comes between the two.
    #[expect(
        clippy::disallowed_methods,
        reason = "the pause before asking again what a terminal whose shell runs on shows"
    )]
    pub(super) async fn follow(
        self,
        hub: Arc<Hub>,
        link: u64,
        id: TerminalId,
        mut updates: broadcast::Receiver<Numbered>,
    ) {
        'screen: loop {
            let TerminalShown { screen, next } = match hub.terminal_screen(&id, link).await {
                Ok(shown) => shown,
                // It has gone, with its shell or its worker.
                Err(e) if e.code == ErrorCode::NotFound => {
                    self.send(TerminalUpdate::Exited(TerminalExit { code: None })).await;
                    return;
                }
                // Its shell runs on, so the client waits for what it shows.
                Err(e) => {
                    warn!(terminal = %id, "asking what a terminal shows: {}", e.message);
                    tokio::time::sleep(SCREEN_AGAIN_AFTER).await;
                    continue;
                }
            };
            if !self.send(TerminalUpdate::Screen(screen)).await {
                return;
            }
            loop {
                let Numbered { at, update } = match updates.recv().await {
                    Ok(numbered) => numbered,
                    Err(RecvError::Lagged(_)) => continue 'screen,
                    // Its record has gone, with its shell or its worker.
                    Err(RecvError::Closed) => {
                        self.send(TerminalUpdate::Exited(TerminalExit { code: None })).await;
                        return;
                    }
                };
                // One the screen takes in already, or one from a newer worker, which a client couldn't read either.
                if at < next || update == TerminalUpdate::Unknown {
                    continue;
                }
                let ended = matches!(update, TerminalUpdate::Exited(_));
                if !self.send(update).await || ended {
                    return;
                }
            }
        }
    }

    /// Whether the client is still there to send to.
    async fn send(&self, update: TerminalUpdate) -> bool {
        self.frames.send(HubFrame::Terminal { subscription_id: self.subscription_id, update }).await.is_ok()
    }
}

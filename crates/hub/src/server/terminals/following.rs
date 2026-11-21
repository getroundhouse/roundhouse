//! Following a terminal for a client that attaches, and sending on what's typed and new sizes in the order they came.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::terminals::TerminalAttach;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame};
use protocol::model::TerminalId;
use protocol::terminal::{TerminalExit, TerminalUpdate, UpdateIndex};
use protocol::worker::TerminalShown;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc};
use tracing::warn;

use super::backlog::Backlog;
use crate::hub::{Hub, Numbered, Queued};
use crate::server::subscribing::add_subscription;
use crate::server::{Connection, Outgoing};

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
        Ok(ClientRequest::TerminalTyped(typed)) => hub.terminal_typed(typed).await,
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
    pub(super) frames: mpsc::Sender<Outgoing>,
}

/// Why following from a screen stopped.
enum Stopped {
    /// The client fell too far behind, so it's sent what the terminal shows again.
    Behind,
    /// The shell ended, or the client stopped following.
    Ended,
}

impl Attached {
    /// Sends what the terminal shows, from the worker on link `link`, then each update after it, until its shell ends
    /// or the client stops following it. While a frame of it is still being written to the client, what comes is held,
    /// so a slow client has at most one of its frames queued, and what's queued behind it, such as replies, waits for
    /// little. A client too far behind is sent what it shows again instead. It follows `updates` from each screen on,
    /// starting it again before it asks for each, so `updates` is only where it starts from.
    pub(super) async fn follow(
        self,
        hub: Arc<Hub>,
        link: u64,
        id: TerminalId,
        mut updates: broadcast::Receiver<Numbered>,
    ) {
        let room = Arc::new(Semaphore::new(1));
        loop {
            // Room first, so the worker is asked for a screen only once it can be sent, and once each time the client
            // falls behind, rather than once for each frame's worth of output that comes meanwhile.
            let Ok(written) = room.clone().acquire_owned().await else { return };
            // From what comes from here, which the screen takes in or is followed by.
            updates = updates.resubscribe();
            let Some(TerminalShown { screen, next }) = self.screen(&hub, link, &id).await else {
                let _ = self.send(TerminalUpdate::Exited(TerminalExit { code: None }), written).await;
                return;
            };
            if !self.send(TerminalUpdate::Screen(screen), written).await {
                return;
            }
            match self.follow_from(&mut updates, next, &room).await {
                Stopped::Behind => {}
                Stopped::Ended => return,
            }
        }
    }

    /// What the terminal shows, or None once it has gone, with its shell or its worker.
    #[expect(
        clippy::disallowed_methods,
        reason = "the pause before asking again what a terminal whose shell runs on shows"
    )]
    async fn screen(&self, hub: &Hub, link: u64, id: &TerminalId) -> Option<TerminalShown> {
        loop {
            match hub.terminal_screen(id, link).await {
                Ok(shown) => return Some(shown),
                Err(e) if e.code == ErrorCode::NotFound => return None,
                // Its shell runs on, so the client waits for what it shows.
                Err(e) => {
                    warn!(terminal = %id, "asking what a terminal shows: {}", e.message);
                    tokio::time::sleep(SCREEN_AGAIN_AFTER).await;
                }
            }
        }
    }

    /// Sends each update from `next` on, holding what comes while the last frame is still being written, until the
    /// client falls too far behind, or the following ends.
    async fn follow_from(
        &self,
        updates: &mut broadcast::Receiver<Numbered>,
        next: UpdateIndex,
        room: &Arc<Semaphore>,
    ) -> Stopped {
        let mut held = Backlog::default();
        // Once the end is held, after which nothing more comes.
        let mut ending = false;
        loop {
            tokio::select! {
                // Sending first, since under a flood there's always more to take in.
                biased;
                Ok(written) = room.clone().acquire_owned(), if !held.is_empty() => {
                    let Some(update) = held.take_oldest() else { continue };
                    let ended = matches!(update, TerminalUpdate::Exited(_));
                    if !self.send(update, written).await || ended {
                        return Stopped::Ended;
                    }
                }
                received = updates.recv(), if !ending => {
                    let update = match received {
                        // One the screen takes in already, or one from a newer worker, which a client couldn't read
                        // either.
                        Ok(Numbered { at, update }) if at < next || update == TerminalUpdate::Unknown => continue,
                        Ok(Numbered { update, .. }) => update,
                        Err(RecvError::Lagged(_)) => return Stopped::Behind,
                        // Its record has gone, with its shell or its worker.
                        Err(RecvError::Closed) => TerminalUpdate::Exited(TerminalExit { code: None }),
                    };
                    ending = matches!(update, TerminalUpdate::Exited(_));
                    held.hold(update);
                    if held.too_far_behind() {
                        return Stopped::Behind;
                    }
                }
                else => return Stopped::Ended,
            }
        }
    }

    /// Queues `update` for the client, holding `written` until it has been written. False once the client has gone.
    async fn send(&self, update: TerminalUpdate, written: OwnedSemaphorePermit) -> bool {
        let frame = HubFrame::Terminal { subscription_id: self.subscription_id, update };
        self.frames.send(Outgoing { frame, written: Some(written) }).await.is_ok()
    }
}

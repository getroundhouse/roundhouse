//! Watching a terminal for a client: sending it what it lacks of the newest picture the hub has, one frame on its way
//! at a time, so a client on a slow link is sent the newest of what the terminal shows rather than all that came
//! between, and what's queued behind it, such as replies, waits behind at most one frame of it.

use std::sync::Arc;

use protocol::client::terminals::TerminalWatch;
use protocol::client::{ApiError, ClientResponse, HubFrame};
use protocol::terminal::{PictureSending, TerminalExit, TerminalPicture, TerminalUpdate};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};

use crate::hub::{Followed, Hub, Watcher};
use crate::server::subscribing::add_subscription;
use crate::server::{Connection, Outgoing};

/// Starts watching the terminal, for subscription `subscription_id`.
pub(in crate::server) fn watch(
    hub: &Arc<Hub>,
    connection: &Connection,
    watch: TerminalWatch,
) -> Result<ClientResponse, ApiError> {
    let TerminalWatch { id, subscription_id } = watch;
    let watcher = hub.terminal_watch(&id)?;
    add_subscription(hub, connection, subscription_id, Followed::Terminal(id), || {
        let watching = Watching { subscription_id, frames: connection.frames.clone() };
        tokio::spawn(watching.watch(watcher)).abort_handle()
    })?;
    Ok(ClientResponse::Ok)
}

/// A client watching a terminal.
pub(in crate::server) struct Watching {
    pub(in crate::server) subscription_id: u64,
    pub(in crate::server) frames: mpsc::Sender<Outgoing>,
}

/// What a client is sent next.
enum Next {
    Picture(TerminalPicture),
    Exited(TerminalExit),
}

impl Watching {
    /// Sends what the client lacks of the hub's newest picture whenever its last frame has been written, until the
    /// shell ends or the client stops watching.
    pub(in crate::server) async fn watch(self, mut watcher: Watcher) {
        let room = Arc::new(Semaphore::new(1));
        let mut sending = PictureSending::default();
        loop {
            // Room first, so what's sent is the newest as of when there's room for it.
            let Ok(written) = room.clone().acquire_owned().await else { return };
            let next = match sending.rest() {
                Some(rest) => Next::Picture(rest),
                None => newest(&mut sending, &mut watcher).await,
            };
            let (frame, ended) = match next {
                Next::Picture(picture) => {
                    (HubFrame::TerminalPicture { subscription_id: self.subscription_id, picture }, false)
                }
                Next::Exited(exit) => {
                    let update = TerminalUpdate::Exited(exit);
                    (HubFrame::Terminal { subscription_id: self.subscription_id, update }, true)
                }
            };
            if !self.send(frame, written).await || ended {
                return;
            }
        }
    }

    /// Queues `frame` for the client, holding `written` until it has been written. False once the client has gone.
    async fn send(&self, frame: HubFrame, written: OwnedSemaphorePermit) -> bool {
        self.frames.send(Outgoing { frame, written: Some(written) }).await.is_ok()
    }
}

/// The first frame of a picture of the newest the hub has, once that has something the client lacks, or the shell's
/// end, once it has all the shell showed.
async fn newest(sending: &mut PictureSending, watcher: &mut Watcher) -> Next {
    loop {
        let now = watcher.watched.borrow_and_update().clone();
        if let Some(shown) = now.shown {
            let (kept, numbering) = (watcher.kept.clone(), shown.numbering);
            if let Some(picture) = sending.next((*shown).clone(), |number| kept.lock().row(numbering, number)) {
                return Next::Picture(picture);
            }
        }
        if let Some(exit) = now.exited {
            return Next::Exited(exit);
        }
        // Its record has gone, with its shell or its worker.
        if watcher.watched.changed().await.is_err() {
            return Next::Exited(TerminalExit { code: None });
        }
    }
}

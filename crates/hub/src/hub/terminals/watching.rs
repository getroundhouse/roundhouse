//! Clients watching a terminal, and telling its worker whether any client is, so it sends the hub pictures only while
//! one is, as `protocol::worker::TerminalWatch` says. The hub keeps a copy of what they show, which a client that
//! begins to watch is sent at once.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use protocol::client::terminals::TerminalRows;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::TerminalId;
use protocol::terminal::{TerminalPicture, TerminalRowsReply};
use protocol::worker;
use tokio::sync::{Mutex as AsyncMutex, watch};
use tracing::warn;

use super::copy::{Copy, Kept, Watched};
use super::terminal_calls::not_open;
use crate::hub::Hub;
use crate::worker_link::WorkerLink;

/// What the hub keeps of whether a terminal is watched.
#[derive(Default)]
pub(super) struct Watch {
    pub(super) copy: Mutex<Copy>,
    /// How many clients watch it, each counted while its [`Watcher`] lasts.
    watchers: AtomicUsize,
    /// What its worker was last told, under a lock, so what's told goes one at a time. `None` while that isn't known,
    /// since a telling didn't get its answer. A worker takes a terminal to be unwatched until it's told, as it does a
    /// new one, and every one a hub carries on with.
    told: AsyncMutex<Option<bool>>,
}

impl Watch {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self { told: AsyncMutex::new(Some(false)), ..Self::default() })
    }
}

/// A client watching an open terminal, counted while this lasts. As it goes, the worker is told whether any client
/// watches still.
pub(crate) struct Watcher {
    hub: Arc<Hub>,
    id: TerminalId,
    /// Not kept, so it goes with the terminal's record, and with it what `watched` is sent from.
    watch: Weak<Watch>,
    /// The newest whole picture the hub has, until the terminal's record goes.
    pub(crate) watched: watch::Receiver<Watched>,
    /// The rows that have left the screen, which say whether the client's copies of them are right.
    pub(crate) kept: Arc<Mutex<Kept>>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // Nothing is told of a terminal whose record has gone.
        let Some(watch) = self.watch.upgrade() else { return };
        // Taken off the count first, so the telling never counts it.
        watch.watchers.fetch_sub(1, Ordering::SeqCst);
        self.hub.terminal_watching_soon(&self.id);
    }
}

impl Hub {
    /// Watches an open terminal, for a client, counted while what this returns lasts. Its worker is told once it's the
    /// first.
    pub(crate) fn terminal_watch(self: &Arc<Self>, id: &TerminalId) -> Result<Watcher, ApiError> {
        self.open_terminal(id)?;
        let watch = self.terminals.open.lock().get(id).map(|open| open.watch.clone()).ok_or_else(|| not_open(id))?;
        watch.watchers.fetch_add(1, Ordering::SeqCst);
        let (watched, kept) = {
            let copy = watch.copy.lock();
            (copy.watched(), copy.kept())
        };
        self.terminal_watching_soon(id);
        Ok(Watcher { hub: self.clone(), id: id.clone(), watch: Arc::downgrade(&watch), watched, kept })
    }

    /// Tells the terminal's worker whether any client watches now. Ok once the worker knows, or can't be told, or the
    /// terminal has gone, and an error if the worker couldn't take it.
    pub(crate) async fn terminal_watching(&self, id: &TerminalId) -> Result<(), ApiError> {
        let Some(watch) = self.terminals.open.lock().get(id).map(|open| open.watch.clone()) else { return Ok(()) };
        let Ok((_, link)) = self.open_terminal(id) else { return Ok(()) };
        let mut told = watch.told.lock().await;
        // Counted under the lock, so what's told last is what's so once clients stop coming and going.
        let watching = watch.watchers.load(Ordering::SeqCst) > 0;
        // Told again while it's watched and the hub wants a fresh picture, as after one it couldn't take in, since
        // a new watch begins with one.
        let resync = watching && watch.copy.lock().wants_fresh();
        if *told == Some(watching) && !resync {
            return Ok(());
        }
        if !link.watches_terminals() {
            return Ok(());
        }
        if watching {
            watch.copy.lock().asked();
        }
        // Until it answers, since a telling cut short, as one for a client that goes is, may have reached it.
        *told = None;
        match link.call(worker::TerminalWatch { terminal: id.clone(), watching }).await {
            Ok(()) => *told = Some(watching),
            // From before pictures, so it sends none, and the client watching is shown nothing.
            Err(e) if e.code == ErrorCode::Unsupported => link.cant_watch_terminals(),
            Err(e) if e.code == ErrorCode::NotFound => {}
            Err(e) => return Err(e),
        }
        if !watching {
            watch.copy.lock().clear();
        }
        Ok(())
    }

    /// Tells the terminal's worker, as [`Hub::terminal_watching`] does, on a task of its own, for where nothing waits
    /// for it, such as a client that has gone.
    pub(crate) fn terminal_watching_soon(self: &Arc<Self>, id: &TerminalId) {
        let (hub, id) = (self.clone(), id.clone());
        // Spawning panics outside a runtime, and nothing stops a watcher being dropped outside one.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        runtime.spawn(async move {
            if let Err(e) = hub.terminal_watching(&id).await {
                warn!(terminal = %id, "telling a worker whether a client watches: {}", e.message);
            }
        });
    }

    /// A frame of a picture its worker sent of one of its terminals, taken into the hub's copy.
    pub(crate) fn terminal_picture(self: &Arc<Self>, link: &WorkerLink, id: &TerminalId, picture: TerminalPicture) {
        let Some(watch) =
            self.terminals.open.lock().get(id).filter(|open| open.on(link.id)).map(|open| open.watch.clone())
        else {
            return;
        };
        if watch.copy.lock().take(picture).is_err() {
            warn!(terminal = %id, "a picture lacked rows the hub didn't have, so its worker is asked to begin again");
            self.terminal_watching_soon(id);
        }
    }

    /// Rows of an open terminal's history, which its worker has.
    pub(crate) async fn terminal_rows(&self, asked: TerminalRows) -> Result<TerminalRowsReply, ApiError> {
        let TerminalRows { id, numbering, from, count } = asked;
        let (_, link) = self.open_terminal(&id)?;
        link.call(worker::TerminalRows { terminal: id, numbering, from, count }).await
    }
}

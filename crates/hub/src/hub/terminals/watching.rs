//! Clients watching a terminal, and telling its worker whether any client follows it, as the hub's list of who follows
//! what says, so it sends the hub pictures only while one does, as `protocol::worker::TerminalWatch` says. The hub
//! keeps a copy of what they show, which a client that begins to watch is sent at once.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::TerminalId;
use protocol::terminal::TerminalPicture;
use protocol::worker;
use tokio::sync::watch;
use tracing::warn;

use super::copy::{Copy, Kept, Watched};
use super::terminal_calls::not_open;
use crate::hub::tellings::Tellings;
use crate::hub::{Followed, Hub};
use crate::worker_link::WorkerLink;

/// What the hub keeps of whether a terminal is watched.
#[derive(Default)]
pub(super) struct Watch {
    pub(super) copy: Mutex<Copy>,
    /// Whether its worker was last told a client watches, with at most one telling waiting for its turn behind the one
    /// under way, since each reads who follows the terminal once it has its turn. `None` while that isn't known, since
    /// a telling didn't get its answer. A worker takes a terminal to be unwatched until it's told, as it does a new one,
    /// and every one a hub carries on with.
    told: Tellings<Option<bool>>,
}

impl Watch {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self { told: Tellings::new(Some(false)), ..Self::default() })
    }
}

/// What a client watching an open terminal is sent from. Only its subscription's entry in the hub's list of who
/// follows what counts it as watching.
pub(crate) struct Watcher {
    /// The newest whole picture the hub has, until the terminal's record goes.
    pub(crate) watched: watch::Receiver<Watched>,
    /// The rows that have left the screen, which say whether the client's copies of them are right.
    pub(crate) kept: Arc<Mutex<Kept>>,
}

impl Hub {
    /// What a client watching an open terminal is sent from.
    pub(crate) fn terminal_watch(&self, id: &TerminalId) -> Result<Watcher, ApiError> {
        self.open_terminal(id)?;
        let watch = self.terminals.open.lock().get(id).map(|open| open.watch.clone()).ok_or_else(|| not_open(id))?;
        let copy = watch.copy.lock();
        Ok(Watcher { watched: copy.watched(), kept: copy.kept() })
    }

    /// Tells the terminal's worker whether any client follows it now. Ok once the worker knows, or can't be told, or the
    /// terminal has gone, and an error if the worker couldn't take it, or didn't answer in the time one that answers at
    /// once is given, which leaves the next telling to tell it again.
    async fn terminal_watching(&self, id: &TerminalId) -> Result<(), ApiError> {
        let Some(watch) = self.terminals.open.lock().get(id).map(|open| open.watch.clone()) else { return Ok(()) };
        let mut told = watch.told.turn().await;
        let Ok((_, link)) = self.open_terminal(id) else { return Ok(()) };
        // Read under the lock, so what's told last is what's so once clients stop coming and going.
        let watching = self.followers.follows(&Followed::Terminal(id.clone()));
        // Told again while it's watched and the hub wants a fresh picture, as after one it couldn't take in, since
        // a new watch begins with one.
        let resync = watching && watch.copy.lock().wants_fresh();
        if *told == Some(watching) && !resync {
            return Ok(());
        }
        if watching {
            watch.copy.lock().asked();
        }
        // Until it answers, since a telling cut short, as one for a client that goes is, may have reached it.
        *told = None;
        match link.call_at_once(worker::TerminalWatch { terminal: id.clone(), watching }).await? {
            Ok(()) => *told = Some(watching),
            Err(e) if e.code == ErrorCode::NotFound => {}
            Err(e) => return Err(e),
        }
        if !watching {
            watch.copy.lock().clear();
        }
        Ok(())
    }

    /// Tells the terminal's worker, as [`Hub::terminal_watching`] does, on a task of its own, for where nothing waits
    /// for it, such as a client that has gone, unless a telling is waiting for its turn already, as [`Tellings`] says.
    pub(crate) fn terminal_watching_soon(self: &Arc<Self>, id: &TerminalId) {
        // Spawning panics outside a runtime, and nothing stops a watcher being dropped outside one.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        let Some(watch) = self.terminals.open.lock().get(id).map(|open| open.watch.clone()) else { return };
        if !watch.told.asked() {
            return;
        }
        let (hub, id) = (self.clone(), id.clone());
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
}

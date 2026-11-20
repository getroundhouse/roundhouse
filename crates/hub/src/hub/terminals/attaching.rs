//! A client attached to a terminal, and telling the terminal's worker whether any client is, so it sends what the
//! program writes only while one is, as [`worker::TerminalAttached`] says.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use protocol::client::{ApiError, ErrorCode};
use protocol::model::TerminalId;
use protocol::worker::{self, TerminalShown};
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::broadcast;
use tracing::warn;

use super::Numbered;
use super::terminal_calls::not_open;
use crate::hub::Hub;

/// What a terminal's worker was last told, under a lock, so what's told goes one at a time.
pub(super) type Telling = Arc<AsyncMutex<Told>>;

/// What a terminal's worker knows of whether any client is attached.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Told {
    /// That one is, or isn't.
    Attached(bool),
    /// Not known, since a telling didn't get its answer, so it's told again whatever it's told.
    Unknown,
}

/// What a worker takes a terminal to be before it's told, as it does a new one, and every one after a hello.
pub(super) fn untold() -> Telling {
    Arc::new(AsyncMutex::new(Told::Attached(true)))
}

/// How many clients are attached to a terminal, each counted while it follows the terminal.
pub(super) type Clients = Arc<AtomicUsize>;

/// A client following an open terminal, counted as attached while this lasts. It's the only way to ask what the
/// terminal shows, which tells the worker first that a client is attached, since a screen it's sent before that would
/// be followed by nothing. As it goes, the worker is told whether any client is attached still.
pub(crate) struct Following {
    hub: Arc<Hub>,
    id: TerminalId,
    /// The ID of the link to the worker that runs its shell, as it was when the client attached.
    link: u64,
    clients: Clients,
    updates: broadcast::Receiver<Numbered>,
}

impl Following {
    /// What the terminal shows, for the updates from here on, which it takes in or is followed by. `NotFound` once the
    /// terminal has gone, with its shell or its worker.
    pub(crate) async fn screen(&mut self) -> Result<TerminalShown, ApiError> {
        self.updates = self.updates.resubscribe();
        self.hub.terminal_attached(&self.id).await?;
        self.hub.terminal_screen(&self.id, self.link).await
    }

    /// What the worker sends of the terminal, from the last screen on.
    pub(crate) fn updates(&mut self) -> &mut broadcast::Receiver<Numbered> {
        &mut self.updates
    }
}

impl Drop for Following {
    fn drop(&mut self) {
        // Taken off the count first, so the telling never counts it.
        self.clients.fetch_sub(1, Ordering::SeqCst);
        self.hub.terminal_attached_soon(&self.id);
    }
}

impl Hub {
    /// Follows an open terminal, for a client that attaches, counted as attached while what this returns lasts.
    pub(crate) fn terminal_follow(self: &Arc<Self>, id: &TerminalId) -> Result<Following, ApiError> {
        let (_, link) = self.open_terminal(id)?;
        let (clients, updates) = {
            let open = self.terminals.open.lock();
            let open = open.get(id).ok_or_else(|| not_open(id))?;
            (open.clients.clone(), open.updates.subscribe())
        };
        clients.fetch_add(1, Ordering::SeqCst);
        Ok(Following { hub: self.clone(), id: id.clone(), link: link.id, clients, updates })
    }

    /// Tells the terminal's worker whether any client is attached now, unless that's what it was told last. Ok once the
    /// worker knows, or can't be told, or the terminal has gone, and an error if the worker couldn't take it.
    pub(super) async fn terminal_attached(&self, id: &TerminalId) -> Result<(), ApiError> {
        let Some((telling, clients)) =
            self.terminals.open.lock().get(id).map(|open| (open.told.clone(), open.clients.clone()))
        else {
            return Ok(());
        };
        let mut told = telling.lock().await;
        // Counted under the lock, so what's told last is what's so once clients stop coming and going.
        let attached = clients.load(Ordering::SeqCst) > 0;
        if *told == Told::Attached(attached) {
            return Ok(());
        }
        let Ok((_, link)) = self.open_terminal(id) else { return Ok(()) };
        if !link.tells_terminals_attached() {
            return Ok(());
        }
        // Until it answers, since a telling cut short, as one for a client that goes is, may have reached it.
        *told = Told::Unknown;
        match link.call(worker::TerminalAttached { terminal: id.clone(), attached }).await {
            Ok(()) => *told = Told::Attached(attached),
            // From before it could be told, so it sends everything, as it did.
            Err(e) if e.code == ErrorCode::Unsupported => link.cant_tell_terminals_attached(),
            Err(e) if e.code == ErrorCode::NotFound => {}
            Err(e) => return Err(e),
        }
        Ok(())
    }

    /// Tells the terminal's worker, as [`Hub::terminal_attached`] does, on a task of its own, for where nothing waits
    /// for it, such as a client that has gone.
    pub(super) fn terminal_attached_soon(self: &Arc<Self>, id: &TerminalId) {
        let (hub, id) = (self.clone(), id.clone());
        // Spawning panics outside a runtime, and nothing stops a following being dropped outside one.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        runtime.spawn(async move {
            if let Err(e) = hub.terminal_attached(&id).await {
                warn!(terminal = %id, "telling a worker whether a client is attached: {}", e.message);
            }
        });
    }
}

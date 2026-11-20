//! What the hub keeps of each terminal open, or being opened, in memory, and what its worker sends of it.

use std::collections::HashMap;
use std::pin::Pin;

use parking_lot::Mutex;
use protocol::client::{ApiError, ClientResponse};
use protocol::model::{Terminal, TerminalId, WorkspaceId};
use protocol::terminal::{TerminalUpdate, UpdateIndex};
use tokio::sync::{Mutex as AsyncMutex, broadcast, watch};

/// The updates held for the clients attached to a terminal. One that falls further behind gets a new screen.
pub(crate) const QUEUED_UPDATES: usize = 256;

/// An update of a terminal, and where it is among the terminal's, so a client is sent only those after its screen.
#[derive(Clone, Debug)]
pub(crate) struct Numbered {
    pub(crate) at: UpdateIndex,
    pub(crate) update: TerminalUpdate,
}

/// The reply to a request the worker has been sent already, to wait for.
pub(crate) type Queued = Pin<Box<dyn Future<Output = Result<ClientResponse, ApiError>> + Send>>;

#[derive(Default)]
pub(crate) struct Terminals {
    /// Held while a terminal's record changes and takes its revision, and while every record is listed, so a list has
    /// every terminal whose revision it takes in. A stored record gets that from the transaction it changes in, and
    /// one kept in memory has no transaction.
    pub(super) changing: AsyncMutex<()>,
    pub(super) open: Mutex<HashMap<TerminalId, Open>>,
}

/// A terminal being opened, or one open.
pub(super) struct Open {
    pub(super) workspace_id: WorkspaceId,
    /// The ID of the link to the worker that runs its shell, once the open knows which that is.
    pub(super) link: Option<u64>,
    pub(super) stage: Stage,
    pub(super) updates: broadcast::Sender<Numbered>,
}

pub(super) enum Stage {
    /// Being opened by the request that noted it, which says on `outcome` how that ended, for any other request to open
    /// it, which waits for that rather than starting it too. `ended` once its shell has ended, before it had a record,
    /// which the open refuses.
    Opening {
        outcome: Outcome,
        ended: bool,
    },
    Open(Terminal),
}

/// How an open ended, once it has.
pub(super) type Outcome = watch::Receiver<Option<Result<Terminal, ApiError>>>;

impl Open {
    pub(super) fn new(workspace_id: WorkspaceId, link: Option<u64>, stage: Stage) -> Self {
        Self { workspace_id, link, stage, updates: broadcast::channel(QUEUED_UPDATES).0 }
    }

    pub(super) fn record(&self) -> Option<&Terminal> {
        let Stage::Open(record) = &self.stage else { return None };
        Some(record)
    }

    pub(super) fn on(&self, link: u64) -> bool {
        self.link == Some(link)
    }

    /// Whether its shell has ended, if it's still being opened by the open that noted it with `noted`.
    pub(super) fn opening_by(&self, noted: &Outcome) -> Option<bool> {
        match &self.stage {
            Stage::Opening { outcome, ended } if outcome.same_channel(noted) => Some(*ended),
            Stage::Opening { .. } | Stage::Open(_) => None,
        }
    }
}

impl Terminals {
    /// How many terminals are open, or being opened, which stopping the hub's workers would end.
    pub(crate) fn count(&self) -> usize {
        self.open.lock().len()
    }

    /// Whether a terminal is open in the workspace, or being opened, which stopping its worker would end.
    pub(crate) fn open_in(&self, workspace_id: &WorkspaceId) -> bool {
        self.open.lock().values().any(|open| open.workspace_id == *workspace_id)
    }

    /// Lists the records along with `list`, under the lock a terminal's change takes, so the list's revision takes in
    /// none it lacks.
    pub(crate) async fn listed_with<T>(&self, list: impl Future<Output = T>) -> (T, Vec<Terminal>) {
        let _changing = self.changing.lock().await;
        let listed = list.await;
        let records = self.open.lock().values().filter_map(|open| open.record().cloned()).collect();
        (listed, records)
    }
}

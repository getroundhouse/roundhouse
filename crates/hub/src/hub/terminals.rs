//! A workspace's terminals as the hub keeps them: each one's record, in memory, since its shell lasts only as long as
//! its worker, and what the worker sends of it, for every client attached. The shells and what they show are the
//! workers', as "Terminals" in `docs/architecture.md` says.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::terminals::{TerminalClose, TerminalInput, TerminalResize};
use protocol::client::{ApiError, ClientResponse, ErrorCode, HubNotice};
use protocol::model::{RecordKey, Terminal, TerminalId, WorkspaceId};
use protocol::terminal::{TerminalUpdate, UpdateIndex};
use protocol::worker::{self, Request, RunningTerminal, TerminalShown};
use tokio::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard, broadcast, watch};
use tracing::warn;

use super::Hub;
use crate::store::Store;
use crate::worker_link::WorkerLink;

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
    /// every terminal whose revision it takes in.
    changing: AsyncMutex<()>,
    open: Mutex<HashMap<TerminalId, Open>>,
}

/// A terminal being opened, or one open.
struct Open {
    workspace_id: WorkspaceId,
    /// The ID of the link to the worker that runs its shell, once the open knows which that is.
    link: Option<u64>,
    stage: Stage,
    updates: broadcast::Sender<Numbered>,
}

enum Stage {
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
type Outcome = watch::Receiver<Option<Result<Terminal, ApiError>>>;

impl Open {
    fn new(workspace_id: WorkspaceId, link: Option<u64>, stage: Stage) -> Self {
        Self { workspace_id, link, stage, updates: broadcast::channel(QUEUED_UPDATES).0 }
    }

    fn record(&self) -> Option<&Terminal> {
        let Stage::Open(record) = &self.stage else { return None };
        Some(record)
    }

    fn on(&self, link: u64) -> bool {
        self.link == Some(link)
    }

    /// Whether its shell has ended, if it's still being opened by the open that noted it with `noted`.
    fn opening_by(&self, noted: &Outcome) -> Option<bool> {
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

impl Hub {
    /// Ends the terminal's shell, and the job it runs, and removes its record. Answers with the record as it last was.
    pub(crate) async fn terminal_close(&self, close: TerminalClose) -> Result<Terminal, ApiError> {
        let (record, link) = self.open_terminal(&close.id)?;
        link.call(worker::TerminalStop { terminal: close.id.clone() }).await?;
        // The worker said it ended, on its way to here, but nothing waits for that to be taken in.
        self.terminal_ended(&close.id, link.id).await;
        Ok(record)
    }

    /// What's typed, sent on to the terminal's worker before anything typed after it.
    pub(crate) async fn terminal_input(&self, typed: TerminalInput) -> Result<Queued, ApiError> {
        self.pass_on(&typed.id, worker::TerminalInput { terminal: typed.id.clone(), input: typed.input }).await
    }

    /// A new size, sent on to the terminal's worker in order with what's typed and the sizes after it.
    pub(crate) async fn terminal_resize(&self, resize: TerminalResize) -> Result<Queued, ApiError> {
        self.pass_on(&resize.id, worker::TerminalResize { terminal: resize.id.clone(), size: resize.size }).await
    }

    /// Sends `request` to the worker of open terminal `id`, after what was sent before it, and answers once the worker
    /// has.
    async fn pass_on(
        &self,
        id: &TerminalId,
        request: impl Request<Reply = ()> + Send + 'static,
    ) -> Result<Queued, ApiError> {
        let (_, link) = self.open_terminal(id)?;
        let replied = link.send(request).await?;
        Ok(Box::pin(async move { replied.await.map(|()| ClientResponse::Ok) }))
    }

    /// The ID of the link to the worker of an open terminal, and the updates it sends of it from now on, for a client
    /// that attaches.
    pub(crate) fn terminal_follow(&self, id: &TerminalId) -> Result<(u64, broadcast::Receiver<Numbered>), ApiError> {
        let (_, link) = self.open_terminal(id)?;
        let open = self.terminals.open.lock();
        let updates = open.get(id).ok_or_else(|| not_open(id))?.updates.subscribe();
        Ok((link.id, updates))
    }

    /// What an open terminal shows, as the worker on link `link` says, for a client that attaches. `NotFound` once the
    /// terminal has gone, as it has when the worker has let go of it, which it does only once the shell has ended, so
    /// that closes it.
    pub(crate) async fn terminal_screen(&self, id: &TerminalId, link: u64) -> Result<TerminalShown, ApiError> {
        let (_, on) = self.open_terminal(id).ok().filter(|(_, on)| on.id == link).ok_or_else(|| not_open(id))?;
        let shown = on.call(worker::TerminalShow { terminal: id.clone() }).await;
        if shown.as_ref().is_err_and(|e| e.code == ErrorCode::NotFound) {
            // Its end may not have been said yet.
            self.terminal_ended(id, link).await;
        }
        shown
    }

    /// What a worker sent of one of its terminals, passed on to every client attached. A shell's end closes its
    /// terminal.
    pub(crate) fn terminal_update(
        self: &Arc<Self>,
        link: &WorkerLink,
        id: TerminalId,
        at: UpdateIndex,
        update: TerminalUpdate,
    ) {
        let ended = matches!(update, TerminalUpdate::Exited(_));
        if let Some(open) = self.terminals.open.lock().get(&id).filter(|open| open.on(link.id)) {
            // None may be attached.
            let _ = open.updates.send(Numbered { at, update });
        }
        if ended {
            // On a task of its own, since this is called as the worker's frames are read, and taking the record away
            // waits for `changing`, which carrying on with the worker's terminals holds while it waits for the
            // worker's reply, which comes after this frame.
            let (hub, link) = (self.clone(), link.id);
            tokio::spawn(async move { hub.terminal_ended(&id, link).await });
        }
    }

    /// Removes the record of a terminal whose shell has ended, run by the worker on link `link`, and announces that.
    /// One still being opened is marked, for the open to refuse.
    pub(crate) async fn terminal_ended(&self, id: &TerminalId, link: u64) {
        let changing = self.terminals.changing.lock().await;
        {
            let mut open = self.terminals.open.lock();
            match open.get_mut(id).filter(|open| open.on(link)).map(|open| &mut open.stage) {
                Some(Stage::Opening { ended, .. }) => {
                    *ended = true;
                    return;
                }
                Some(Stage::Open(_)) => {}
                None => return,
            }
        }
        self.forget_terminals(&changing, |terminal_id, _| terminal_id == id).await;
    }

    /// Removes the records of every terminal whose shell ran on link `link`, since it has gone, and with it the
    /// shells.
    pub(crate) async fn terminals_lost(&self, link: u64) {
        let changing = self.terminals.changing.lock().await;
        self.forget_terminals(&changing, |_, open| open.on(link)).await;
    }

    /// Takes on the terminals that a worker an earlier hub left is running, announcing a record for each, and says how
    /// many it took on. One it can't give a record ends, so no shell runs that no client can see.
    pub(crate) async fn carry_on_terminals(&self, workspace_id: &WorkspaceId, link: &WorkerLink) -> usize {
        // Across the call, so the end of a shell the worker lists, which can reach the hub before the list does, or
        // just after it, waits to remove the record this makes.
        let _changing = self.terminals.changing.lock().await;
        let running = match link.call(worker::RunningTerminals).await {
            Ok(reply) => reply.terminals,
            // A worker from before terminals runs none.
            Err(e) if e.code == ErrorCode::Unsupported => return 0,
            Err(e) => {
                warn!(%workspace_id, "asking the worker an earlier hub left for its terminals: {}", e.message);
                return 0;
            }
        };
        let mut carried_on = 0;
        for RunningTerminal { terminal, opened_at_ms } in running {
            let rev = match self.blocking(Store::next_rev).await {
                Ok(rev) => rev,
                Err(e) => {
                    warn!(%terminal, "carrying on with a terminal, so it's ended: {}", e.message);
                    // Its end waits for `changing` on a task of its own, so this can wait for the stop holding it.
                    let _ = link.call(worker::TerminalStop { terminal }).await;
                    continue;
                }
            };
            let record = Terminal { id: terminal.clone(), workspace_id: workspace_id.clone(), opened_at_ms, rev };
            let open = Open::new(workspace_id.clone(), Some(link.id), Stage::Open(record.clone()));
            self.terminals.open.lock().insert(terminal, open);
            self.notify(HubNotice::Terminal(record));
            carried_on += 1;
        }
        carried_on
    }

    /// The record of a terminal that's open, and the link to its worker, if that's still the workspace's worker.
    fn open_terminal(&self, id: &TerminalId) -> Result<(Terminal, Arc<WorkerLink>), ApiError> {
        let (record, link_id) = {
            let open = self.terminals.open.lock();
            let open = open.get(id).ok_or_else(|| not_open(id))?;
            (open.record().cloned().ok_or_else(|| not_open(id))?, open.link)
        };
        let link = self.workers.lock().get(&record.workspace_id).filter(|link| Some(link.id) == link_id).cloned();
        Ok((record, link.ok_or_else(|| not_open(id))?))
    }

    /// Removes every terminal `which` picks, announcing each removal of one with a record, under `changing`.
    async fn forget_terminals(&self, _changing: &AsyncMutexGuard<'_, ()>, which: impl Fn(&TerminalId, &Open) -> bool) {
        let gone: Vec<_> = {
            let mut open = self.terminals.open.lock();
            let picked: Vec<_> = open.iter().filter(|(id, open)| which(id, open)).map(|(id, _)| id.clone()).collect();
            picked.into_iter().filter_map(|id| open.remove(&id)).filter_map(|open| open.record().cloned()).collect()
        };
        for record in gone {
            match self.blocking(Store::next_rev).await {
                Ok(rev) => self.notify(HubNotice::Removed { record: RecordKey::Terminal { id: record.id }, rev }),
                // Clients that miss it drop the terminal once it's not on the next list, or once it ends.
                Err(e) => warn!(terminal = %record.id, "announcing a terminal's removal: {}", e.message),
            }
        }
    }
}

mod opening;
#[cfg(test)]
mod tests;

fn not_open(id: &TerminalId) -> ApiError {
    ApiError::new(ErrorCode::NotFound, format!("terminal {id} isn't open"))
}

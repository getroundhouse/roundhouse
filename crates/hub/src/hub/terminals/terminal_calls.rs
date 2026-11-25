//! What the hub does with a terminal as a client asks: types into it, resizes it and closes it.

use std::collections::HashSet;
use std::sync::Arc;

use protocol::client::terminals::{TerminalClose, TerminalFit, TerminalTyped};
use protocol::client::{ApiError, ClientResponse, ErrorCode, HubNotice};
use protocol::model::{RecordKey, Terminal, TerminalId, WorkspaceId};
use protocol::terminal::TerminalUpdate;
use protocol::worker::{self, Request, RunningTerminal};
use tokio::sync::MutexGuard as AsyncMutexGuard;
use tracing::warn;

use crate::hub::Hub;
use crate::store::Store;
use crate::worker_link::{WorkerLink, WorkerLinkId, answered_at_once};

use super::{Open, Queued, Stage};

impl Hub {
    /// Tells the terminal's worker to end its shell, and the job it runs, as [`end_shell`] does, and answers with its
    /// record as it last was, without waiting for the shell to end. The record goes once the shell's end reaches the
    /// hub, as any shell's does.
    pub(crate) async fn terminal_close(&self, close: TerminalClose) -> Result<Terminal, ApiError> {
        let (record, link) = self.open_terminal(&close.id)?;
        end_shell(&link, &close.id).await?;
        Ok(record)
    }

    /// What's pressed, sent on to the terminal's worker before anything pressed after it, for the worker to turn into
    /// what the program reads.
    pub(crate) async fn terminal_typed(&self, typed: TerminalTyped) -> Result<Queued, ApiError> {
        self.pass_on(&typed.id, worker::TerminalTyped { terminal: typed.id.clone(), typed: typed.typed }).await
    }

    /// A new size, sent on to the terminal's worker in order with what's pressed, answered with where the cells the
    /// client named went, once the worker says, which it's given as long as a read of the terminal, as one it answers
    /// at once.
    pub(crate) async fn terminal_fit(&self, fit: TerminalFit) -> Result<Queued, ApiError> {
        let TerminalFit { id, size, numbering, keep } = fit;
        let (_, link) = self.open_terminal(&id)?;
        let replied = link.send(worker::TerminalFit { terminal: id.clone(), size, numbering, keep }).await?;
        Ok(Box::pin(async move { answered_at_once(replied).await?.map(ClientResponse::TerminalFit) }))
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

    /// What a worker sent of one of its terminals. A shell's end closes its terminal, and is passed on to every client
    /// watching. A worker from before rows still sends what its program writes, which reads as `Unknown`, and is
    /// dropped.
    pub(crate) fn terminal_update(self: &Arc<Self>, link: &WorkerLink, id: TerminalId, update: &TerminalUpdate) {
        let &TerminalUpdate::Exited(exit) = update else { return };
        if let Some(open) = self.terminals.open.lock().get(&id).filter(|open| open.on(link.id)) {
            open.watch.copy.lock().exited(exit);
        }
        // On a task of its own, since this is called as the worker's frames are read, and taking the record away waits
        // for `changing`, which a list of the hub's records holds while it reads the database.
        let (hub, link) = (self.clone(), link.id);
        tokio::spawn(async move { hub.terminal_ended(&id, link).await });
    }

    /// Removes the record of a terminal whose shell has ended, run by the worker on link `link`, and announces that.
    /// One still being opened is marked, for the open to refuse, and so is one the hub is carrying on with, for it to
    /// give no record.
    pub(crate) async fn terminal_ended(&self, id: &TerminalId, link: WorkerLinkId) {
        let changing = self.terminals.changing.lock().await;
        {
            let mut open = self.terminals.open.lock();
            match open.get_mut(id).filter(|open| open.on(link)).map(|open| &mut open.stage) {
                Some(Stage::Opening { ended, .. }) => {
                    *ended = true;
                    return;
                }
                Some(Stage::Open(_)) => {}
                None => {
                    if let Some(ended) = self.terminals.asked_what_runs.lock().get_mut(&link) {
                        ended.insert(id.clone());
                    }
                    return;
                }
            }
        }
        self.forget_terminals(&changing, |terminal_id, _| terminal_id == id).await;
    }

    /// Removes the records of every terminal whose shell ran on link `link`, since it has gone, and with it the
    /// shells.
    pub(crate) async fn terminals_lost(&self, link: WorkerLinkId) {
        let changing = self.terminals.changing.lock().await;
        self.forget_terminals(&changing, |_, open| open.on(link)).await;
    }

    /// Takes on the terminals that a worker an earlier hub left is running, announcing a record for each, and says how
    /// many it took on. One it can't give a record ends, so no shell runs that no client can see. A worker that doesn't
    /// say which it runs, within the time one that answers at once is given, is taken to run none, as one that fails
    /// to say is, and nothing waits for it meanwhile.
    pub(crate) async fn carry_on_terminals(&self, workspace_id: &WorkspaceId, link: &WorkerLink) -> usize {
        let (carried_on, unrecorded) = self.give_records(workspace_id, link).await;
        // Told once `changing` is let go, as an open that can't give its terminal a record tells its worker.
        for terminal in unrecorded {
            if let Err(e) = end_shell(link, &terminal).await {
                warn!(name: "terminal.unended", %terminal, error = e.message, "Couldn't end a shell given no record");
            }
        }
        carried_on
    }

    /// Gives a record to each terminal the worker on `link` says it runs, under `changing`, and says how many it gave
    /// one, and which it couldn't, whose shells are to end.
    async fn give_records(&self, workspace_id: &WorkspaceId, link: &WorkerLink) -> (usize, Vec<TerminalId>) {
        // Before it's asked, so the end of a shell the worker lists, which can reach the hub before the list does, is
        // kept for this to give no record. One that comes after the list waits for `changing`, and removes the record.
        self.terminals.asked_what_runs.lock().insert(link.id, HashSet::new());
        let answered = link.call_at_once(worker::RunningTerminals).await.unwrap_or_else(|late| Err(late.into()));
        let _changing = self.terminals.changing.lock().await;
        let ended = self.terminals.asked_what_runs.lock().remove(&link.id).unwrap_or_default();
        let running = match answered {
            Ok(reply) => reply.terminals,
            Err(e) => {
                warn!(
                    name: "terminal.recovery_unasked",
                    %workspace_id,
                    error = e.message,
                    "Couldn't ask the worker an earlier hub left for its terminals"
                );
                return (0, Vec::new());
            }
        };
        let (mut carried_on, mut unrecorded) = (0, Vec::new());
        for RunningTerminal { terminal, opened_at_ms } in
            running.into_iter().filter(|running| !ended.contains(&running.terminal))
        {
            let rev = match self.blocking(Store::next_rev).await {
                Ok(rev) => rev,
                Err(e) => {
                    warn!(
                        name: "terminal.recovery_failed",
                        %terminal,
                        error = e.message,
                        "Couldn't carry on with a terminal, so it's ended"
                    );
                    unrecorded.push(terminal);
                    continue;
                }
            };
            let record = Terminal { id: terminal.clone(), workspace_id: workspace_id.clone(), opened_at_ms, rev };
            let open = Open::new(workspace_id.clone(), Some(link.id), Stage::Open(record.clone()));
            self.terminals.open.lock().insert(terminal, open);
            self.notify(HubNotice::Terminal(record));
            carried_on += 1;
        }
        (carried_on, unrecorded)
    }

    /// The record of a terminal that's open, and the link to its worker, if that's still the workspace's worker.
    pub(super) fn open_terminal(&self, id: &TerminalId) -> Result<(Terminal, Arc<WorkerLink>), ApiError> {
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
                Err(e) => {
                    warn!(
                        name: "terminal.removal_untold",
                        terminal = %record.id,
                        error = e.message,
                        "Couldn't announce a terminal's removal"
                    );
                }
            }
        }
    }
}

/// Tells the worker on `link` to end the shell of `terminal`, and the job it runs, without waiting for it to end, which is
/// how the hub ends every shell. A worker answers only once the shell has ended, after its grace period, so one that has
/// stopped answering, but stays connected, would hold whatever waited for good. The shell's end reaches the hub as any
/// shell's does, which [`Hub::terminal_update`] takes in. Fails only if the stop can't be sent, as when the link has
/// ended.
pub(super) async fn end_shell(link: &WorkerLink, terminal: &TerminalId) -> Result<(), ApiError> {
    // Dropping the reply forgets the call, and the worker carries on with it.
    link.send(worker::TerminalStop { terminal: terminal.clone() }).await.map(drop)
}

pub(super) fn not_open(id: &TerminalId) -> ApiError {
    ApiError::new(ErrorCode::NotFound, format!("terminal {id} isn't open"))
}

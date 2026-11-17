//! Opening a terminal: noting it, starting its shell on the workspace's worker, and giving it its record, which only
//! the request that noted it does, so another for the same terminal waits for how that ends.

use std::sync::Arc;

use protocol::client::terminals::TerminalOpen;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Terminal, TerminalId, WorkspaceId};
use protocol::terminal::TerminalSize;
use protocol::worker;
use tokio::sync::watch;

use super::{Open, Outcome, Stage};
use crate::hub::{Hub, workspace_env};
use crate::store::{Store, now_ms};
use crate::worker_link::WorkerLink;

impl Hub {
    /// Starts the terminal's shell in the workspace's worktree, with the environment its agents get, and announces its
    /// record. One open already is answered with its record, and one being opened with how that ends.
    pub(crate) async fn terminal_open(self: &Arc<Self>, open: TerminalOpen) -> Result<Terminal, ApiError> {
        let TerminalOpen { workspace_id, id, size } = open;
        // Before it asks for the worker, so nothing replaces or stops that worker as it opens, and before the shell
        // starts, so what the worker sends of it, such as the shell's end, has somewhere to go.
        let noted = match self.terminals.open.lock().entry(id.clone()) {
            std::collections::hash_map::Entry::Occupied(opened) => {
                if opened.get().workspace_id != workspace_id {
                    return Err(ApiError::new(ErrorCode::Conflict, format!("terminal {id} is another workspace's")));
                }
                match &opened.get().stage {
                    Stage::Open(record) => return Ok(record.clone()),
                    Stage::Opening { outcome, .. } => Err(outcome.clone()),
                }
            }
            std::collections::hash_map::Entry::Vacant(opening) => {
                let (finished, outcome) = watch::channel(None);
                let stage = Stage::Opening { outcome: outcome.clone(), ended: false };
                opening.insert(Open::new(workspace_id.clone(), None, stage));
                Ok((finished, outcome))
            }
        };
        let (finished, noted) = match noted {
            Ok(noted) => noted,
            // Only the open that noted it goes on with it, so neither can take away what the other did.
            Err(mut outcome) => {
                let outcome = outcome.wait_for(Option::is_some).await.map(|outcome| outcome.clone());
                return outcome.ok().flatten().unwrap_or_else(|| Err(stopped_as_it_opened()));
            }
        };
        let opened_at_ms = now_ms();
        let opened = match self.terminal_start(&id, &workspace_id, size, opened_at_ms, &noted).await {
            Ok((link, started)) => self.terminal_started(&id, &link, opened_at_ms, started, &noted).await,
            Err(e) => Err(e),
        };
        if opened.is_err() {
            let mut open = self.terminals.open.lock();
            // Unless a worker that went has taken it already, and another open has noted it since.
            if open.get(&id).is_some_and(|open| open.opening_by(&noted).is_some()) {
                open.remove(&id);
            }
        }
        finished.send_replace(Some(opened.clone()));
        opened
    }

    /// Asks the workspace's worker to start the terminal's shell, once the open has said which worker that is.
    async fn terminal_start(
        self: &Arc<Self>,
        id: &TerminalId,
        workspace_id: &WorkspaceId,
        size: TerminalSize,
        opened_at_ms: i64,
        noted: &Outcome,
    ) -> Result<(Arc<WorkerLink>, Result<(), ApiError>), ApiError> {
        let ready = self.ready_worktree(workspace_id).await?;
        let (link, cwd) = (ready.link().clone(), ready.path().to_owned());
        let workspace = ready.into_ready().into_workspace();
        let repo = self.repo_of(&workspace).await?;
        let env = workspace_env(&workspace, &repo);
        if let Some(open) = self.terminals.open.lock().get_mut(id).filter(|open| open.opening_by(noted).is_some()) {
            open.link = Some(link.id);
        }
        let start = worker::TerminalStart { terminal: id.clone(), size, cwd, env, opened_at_ms };
        let started = link.call(start).await.map_err(|e| {
            if e.code != ErrorCode::Unsupported {
                return e;
            }
            // It's replaced by one of the hub's own build once nothing is at work in the workspace.
            ApiError::new(
                ErrorCode::Unavailable,
                "the workspace's worker is from before terminals, until nothing is at work in it",
            )
        });
        Ok((link, started))
    }

    /// Gives a terminal that has started its record, unless its shell has ended already. The open that noted it with
    /// `noted` takes away what it noted if this fails.
    async fn terminal_started(
        &self,
        id: &TerminalId,
        link: &WorkerLink,
        opened_at_ms: i64,
        started: Result<(), ApiError>,
        noted: &Outcome,
    ) -> Result<Terminal, ApiError> {
        let changing = self.terminals.changing.lock().await;
        let ended = self.terminals.open.lock().get(id).and_then(|open| open.opening_by(noted));
        match (started, ended) {
            (Err(e), _) => return Err(e),
            (Ok(()), Some(false)) => {}
            (Ok(()), Some(true)) => return Err(ApiError::new(ErrorCode::Worker, "the terminal's shell ended at once")),
            // Its worker went, taking what this open noted.
            (Ok(()), None) => {
                drop(changing);
                // If its worker has gone, so has the shell, and this fails with nothing to stop.
                let _ = link.call(worker::TerminalStop { terminal: id.clone() }).await;
                return Err(stopped_as_it_opened());
            }
        }
        let rev = match self.blocking(Store::next_rev).await {
            Ok(rev) => rev,
            Err(e) => {
                drop(changing);
                let _ = link.call(worker::TerminalStop { terminal: id.clone() }).await;
                return Err(e);
            }
        };
        let mut open = self.terminals.open.lock();
        // Still what this open noted, since nothing else takes that away while it opens, but a worker that goes, which
        // waits for `changing`.
        let open = open.get_mut(id).filter(|open| open.opening_by(noted).is_some()).ok_or_else(stopped_as_it_opened)?;
        let record = Terminal { id: id.clone(), workspace_id: open.workspace_id.clone(), opened_at_ms, rev };
        open.stage = Stage::Open(record.clone());
        self.notify(HubNotice::Terminal(record.clone()));
        Ok(record)
    }
}

fn stopped_as_it_opened() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the terminal's worker stopped as it opened")
}

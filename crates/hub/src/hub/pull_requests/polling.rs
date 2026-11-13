//! Background PR reads: choose due work, keep it within the shared budget, and release workers after reading.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use protocol::model::Workspace;
use tracing::debug;

use super::{Answer, Asked, Hub, When, calls_to_read, schedule};
use crate::hub::ready::ReadyWorkspace;
use crate::store::Store;

/// How often the hub looks for a pull request to read that nobody asked for, while the budget can spare the calls.
const PACE: Duration = Duration::from_secs(1);

/// The longest it waits for the budget before looking again, since what's due changes meanwhile.
const LONGEST_PAUSE: Duration = Duration::from_mins(1);

impl Hub {
    /// Reads the pull requests nobody asks for, one at a time, whichever [`Hub::to_read_in_turn`] says is next, as the
    /// budget can spare their calls. Runs until the hub stops.
    pub(crate) async fn watch_pull_requests(self: Arc<Self>) {
        let mut pause = PACE;
        loop {
            tokio::time::sleep(pause).await;
            pause = PACE;
            self.read_remote_due().await;
            let Some(workspace) = self.to_read_in_turn().await else { continue };
            if let Err(until) = self.read_in_turn(&workspace).await {
                pause = until.saturating_duration_since(Instant::now()).clamp(PACE, LONGEST_PAUSE);
            }
        }
    }

    /// The workspace whose pull request is due to be read now that nobody has asked, as [`schedule::due`] says. While
    /// no client is connected, only one an agent waits on, since nobody would see the rest. Once one connects again,
    /// whatever has come due meanwhile is read.
    pub(in crate::hub) async fn to_read_in_turn(&self) -> Option<Workspace> {
        // A store that fails says why as it does, and those waited on wait their turn with the rest meanwhile.
        let waited = self.blocking(Store::workspaces_waited_on).await.unwrap_or_default();
        let seen = self.clients.load(Ordering::Relaxed) > 0;
        if !seen && waited.is_empty() {
            return None;
        }
        let mut workspaces = self.workspace_list(false).await.ok()?;
        if !seen {
            workspaces.retain(|workspace| waited.contains(&workspace.id));
        }
        schedule::due(&workspaces, &self.pull_requests.schedules.lock(), &waited, Instant::now()).cloned()
    }

    /// Reads one nobody asked for, once the budget can spare its calls, starting its workspace's worker if it has
    /// none, and stopping it again after if nothing else has wanted it meanwhile. Says when it's worth trying again
    /// when the budget can't spare them.
    async fn read_in_turn(self: &Arc<Self>, workspace: &Workspace) -> Result<(), Instant> {
        let id = &workspace.id;
        let Ok(path) = ReadyWorkspace::new(workspace.clone()).map(|ready| ready.path().to_owned()) else {
            // So it doesn't come up again at once, ahead of the rest.
            self.pull_requests.wait_longer(id);
            return Ok(());
        };
        let calls = calls_to_read(workspace.pull_request.as_ref());
        self.pull_requests.budget.lock().allows(calls, Asked::InTurn, Instant::now())?;
        let link = match self.worker_to_read(id).await {
            Ok(link) => link,
            Err(e) => {
                debug!(%id, "starting a worker to read the pull request: {}", e.message);
                self.pull_requests.wait_longer(id);
                return Ok(());
            }
        };
        let spent = self.spend(&link, &path, calls, Asked::InTurn).await;
        if spent.is_ok()
            && let Err(e) =
                self.read_pull_request(id, path, &link, Asked::InTurn, When::Always).await.and_then(Answer::or_known)
        {
            debug!(%id, "reading the pull request in turn: {}", e.message);
        }
        self.stop_worker_only_read(workspace, &link).await;
        spent
    }
}

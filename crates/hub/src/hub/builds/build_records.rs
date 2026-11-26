use super::super::Hub;
use crate::hub::observations::{Keeper, WorkspaceWorkerBuild};
use crate::spans;
use crate::store::now_ms;
use crate::worker_link::WorkerLink;
use protocol::model::{OtherBuild, RunId, WorkspaceId};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tracing::{Instrument as _, debug, info, warn};

impl Hub {
    /// Replaces `link`, the workspace's worker, which is another build, once two looks in a row find nothing in the
    /// workspace at work. Gives up once the worker has gone, or been replaced, or the hub is stopping. Which build it
    /// is is kept first, as [`Hub::worker_build_heard`] says, so every caller says it.
    #[expect(
        clippy::disallowed_methods,
        reason = "how often a worker of another build is looked at, to replace it once nothing in its workspace is at work"
    )]
    pub(in super::super) async fn replace_once_idle(
        self: &Arc<Self>,
        workspace_id: WorkspaceId,
        link: Arc<WorkerLink>,
    ) {
        self.worker_build_heard(&workspace_id, &link).await;
        let hub = self.clone();
        let span = spans::workspace(&workspace_id);
        tokio::spawn(
            async move {
                let mut idle_before = false;
                loop {
                    tokio::select! {
                        () = tokio::time::sleep(hub.timers.look_every) => {}
                        () = link.wait_closed() => break,
                    }
                    let idle = hub.nothing_at_work_in(&workspace_id, &link);
                    if !idle {
                        // `other_builds` in the daemon's e2e tests counts these, to know two looks have kept it.
                        let build = link.build();
                        debug!(
                            name: "worker.other_build_kept",
                            %workspace_id,
                            build,
                            "Keeping a worker from another build, with something at work"
                        );
                    }
                    if idle && idle_before && hub.replace_if_idle(&workspace_id, &link).await {
                        break;
                    }
                    idle_before = idle;
                }
            }
            .instrument(span),
        );
    }

    /// Keeps that the workspace's worker, `link`, is another build than the hub's, as its hello said, current while
    /// it's the workspace's, so every client can say which build its agents are on, and gone with it, since the next is
    /// the hub's own. One that's no longer the workspace's isn't kept, so nothing is kept after it went. One too old to
    /// name its build is another all the same.
    pub(crate) async fn worker_build_heard(&self, workspace_id: &WorkspaceId, link: &Arc<WorkerLink>) {
        let build = OtherBuild { said: link.said_build().map(str::to_string) };
        let (id, keeper) = (workspace_id.clone(), Keeper::Worker(link.id));
        let still_its = || link.is_current(&self.workers.lock(), workspace_id);
        let kept = self.observation_said_by_while::<WorkspaceWorkerBuild>(id, keeper, build, now_ms(), still_its);
        if let Err(e) = kept.await {
            warn!(
                name: "worker.build_unkept",
                %workspace_id,
                error = e.message,
                "Couldn't keep which build its worker is"
            );
        }
    }

    /// Stops the workspace's worker, so the next request for the workspace starts this hub's build, unless something
    /// in the workspace is at work. Under the workspace's gate, so the next worker can't start until this one has
    /// stopped. Whether there's anything more to wait for: not once it's stopped, or the hub has replaced it already,
    /// or is stopping.
    ///
    /// Something that starts work records it before it asks for the worker, as a prompt is recorded before it's sent,
    /// so once nothing is at work, whatever wants the worker next finds it gone from the map, and waits for the gate.
    pub(super) async fn replace_if_idle(&self, workspace_id: &WorkspaceId, link: &Arc<WorkerLink>) -> bool {
        let workspace = match self.workspace(workspace_id).await {
            Ok(workspace) => workspace,
            // Looked at again.
            Err(e) => {
                warn!(
                    name: "worker.other_build_unreplaced",
                    %workspace_id,
                    error = e.message,
                    "Couldn't replace a worker from another build"
                );
                return false;
            }
        };
        let gate = self.worker_gate(workspace_id);
        let mut cleared = gate.lock().await;
        if !self.nothing_at_work_in(workspace_id, link) {
            return false;
        }
        {
            let mut workers = self.workers.lock();
            // A hub that's stopping stops or leaves it, as it does every worker.
            if !link.is_current(&workers, workspace_id) || self.shutting_down.load(Ordering::SeqCst) {
                return true;
            }
            workers.remove(workspace_id);
        }
        // So each agent's exit, which stopping it brings, is of an agent its session has let go of, and changes nothing
        // even if a prompt is recorded first. What each run kept goes now, since its exit may never reach the hub, and
        // one that does finds nothing kept by it.
        let let_go: Vec<RunId> = {
            let sessions = self.sessions.lock();
            let here = sessions.live.values().filter(|session| &session.workspace_id == workspace_id);
            here.filter_map(|session| session.let_go_of_agent_on(link.id)).collect()
        };
        for run in let_go {
            self.keeper_went(Keeper::Run(run)).await;
        }
        info!(
            name: "worker.other_build_stopped",
            %workspace_id,
            build = link.build(),
            "Stopping a worker from another build, with nothing at work"
        );
        self.stop_and_clear(&workspace, link, &mut cleared, "replacing a worker from another build").await;
        true
    }

    /// Whether nothing in the workspace would be cut short by stopping its worker: no session at work, which covers one
    /// waiting on an approval or with work in the background, no setup script, no terminal open, whose shell would
    /// end, and no call to the worker under way, such as a read or a title being written.
    pub(super) fn nothing_at_work_in(&self, workspace_id: &WorkspaceId, link: &WorkerLink) -> bool {
        let session_at_work = self
            .sessions
            .lock()
            .live
            .values()
            .any(|session| &session.workspace_id == workspace_id && session.status().is_active());
        !session_at_work
            && !self.setup_gates.lock().contains_key(workspace_id)
            && !self.terminals.open_in(workspace_id)
            && link.calls_under_way() == 0
    }
}

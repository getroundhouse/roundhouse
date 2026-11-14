use super::super::{BEFORE_SERVING, Hub};
use crate::error::provider_failed;
use crate::session::{CarriedOver, Item, LiveSession, Progress};
use crate::store::{Store, StoreError, cancelled_approval};
use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{
    InstanceId, ProviderName, RunCapabilities, RunId, Seq, SessionId, Workspace, WorkspaceId, WorkspaceStatus,
};
use protocol::provider::InstanceState;
use protocol::worker::RunningAgent;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

/// What an earlier hub left at work: its sessions at work, by workspace, every workspace that isn't archived, for
/// finding the workers it left running, and the prompts each session's agent hadn't taken.
pub(crate) struct LeftAtWork {
    pub(super) sessions: HashMap<WorkspaceId, Vec<SessionId>>,
    pub(super) workspaces: Vec<Workspace>,
    pub(super) untaken: HashMap<SessionId, Vec<Seq>>,
}

impl LeftAtWork {
    pub(crate) fn read(store: &Store) -> Result<Self, StoreError> {
        let mut sessions: HashMap<_, Vec<_>> = HashMap::new();
        for id in store.active_session_ids()? {
            sessions.entry(store.get_session(&id)?.session.workspace_id).or_default().push(id);
        }
        let workspaces = store.list_workspaces(false)?;
        Ok(Self { sessions, workspaces, untaken: store.untaken_prompts()?.into_iter().collect() })
    }
}

/// The prompts each session's agent hadn't taken, which a session takes as it carries on or is interrupted.
type Untaken = Arc<Mutex<HashMap<SessionId, Vec<Seq>>>>;

impl Hub {
    /// Before the hub serves anything, so no session is live yet, and each one starts with the agent it still has, if
    /// it has one. Each workspace's worker is reached on its own, so a slow one holds up only its own sessions.
    pub(crate) async fn carry_on(self: &Arc<Self>, left: LeftAtWork) {
        let LeftAtWork { mut sessions, workspaces, untaken } = left;
        let untaken: Untaken = Arc::new(Mutex::new(untaken));
        let mut carrying = JoinSet::new();
        // A worker can be running with no session at work, its agents idle, and the hub stops it as it stops, as it
        // would one it started.
        let being_moved: HashSet<WorkspaceId> = workspaces
            .iter()
            .filter(|workspace| matches!(workspace.status, WorkspaceStatus::Archiving | WorkspaceStatus::Restoring))
            .map(|workspace| workspace.id.clone())
            .collect();
        for workspace_id in self.with_instances_running(&workspaces).await {
            let at_work = sessions.remove(&workspace_id).unwrap_or_default();
            let being_moved = being_moved.contains(&workspace_id);
            carrying.spawn(self.clone().carry_on_in(workspace_id, at_work, being_moved, untaken.clone()));
        }
        for id in sessions.into_values().flatten() {
            let prompts = untaken.lock().remove(&id).unwrap_or_default();
            self.write(move |store| interrupt(store, &id, prompts)).await;
        }
        carrying.join_all().await;
        // Every agent still running has been carried on with, so whatever the rest claimed is gone with them.
        let running: Vec<SessionId> = self.sessions.lock().live.keys().cloned().collect();
        self.write(move |store| store.clear_claims_but(&running)).await;
        // A session that wasn't at work, and whose agent isn't running, never will take these.
        let untaken = std::mem::take(&mut *untaken.lock());
        for (id, prompts) in untaken {
            self.write(move |store| drop_untaken(store, &id, prompts)).await;
        }
    }

    /// Those of `workspaces` whose instance their provider says is running, which a worker may be on.
    pub(super) async fn with_instances_running(&self, workspaces: &[Workspace]) -> Vec<WorkspaceId> {
        let mut by_provider: HashMap<&ProviderName, Vec<&Workspace>> = HashMap::new();
        for workspace in workspaces {
            by_provider.entry(&workspace.provider).or_default().push(workspace);
        }
        let mut running = Vec::new();
        for (name, workspaces) in by_provider {
            let listed = match self.provider(name) {
                Ok(provider) => match tokio::time::timeout(BEFORE_SERVING, provider.list()).await {
                    Ok(listed) => listed.map_err(|e| provider_failed(&e)),
                    Err(_) => Err(ApiError::new(ErrorCode::Worker, format!("no answer within {BEFORE_SERVING:?}"))),
                },
                Err(e) => Err(e),
            };
            let instances: HashSet<InstanceId> = match listed {
                Ok(instances) => instances
                    .into_iter()
                    .filter(|instance| instance.state == InstanceState::Running)
                    .map(|instance| instance.instance_id)
                    .collect(),
                Err(e) => {
                    warn!(provider = %name, "listing the instances an earlier hub left: {}", e.message);
                    continue;
                }
            };
            let on_them = workspaces.into_iter().filter(|workspace| instances.contains(&workspace.instance_id));
            running.extend(on_them.map(|workspace| workspace.id.clone()));
        }
        running
    }

    /// `being_moved` is whether the earlier hub was archiving or restoring the workspace.
    pub(super) async fn carry_on_in(
        self: Arc<Self>,
        workspace_id: WorkspaceId,
        at_work: Vec<SessionId>,
        being_moved: bool,
        untaken: Untaken,
    ) {
        let take = |id: &SessionId| untaken.lock().remove(id).unwrap_or_default();
        let (link, running) = match self.worker_left_running(&workspace_id).await {
            Ok(reached) => reached,
            Err(e) => {
                warn!(%workspace_id, "the worker an earlier hub left couldn't be reached: {}", e.message);
                for id in at_work {
                    let prompts = take(&id);
                    self.write(move |store| interrupt(store, &id, prompts)).await;
                }
                return;
            }
        };
        // A session whose agent has gone since the earlier hub did, but said so to its worker, carries on with it too,
        // so what it said is taken as that agent's, such as the end of its turn and its exit.
        let said = link.held_runs().await;
        let mut gone = Vec::new();
        for id in &at_work {
            let running_agent = running.iter().find(|agent| agent.session_id == *id);
            if let Some(run) = running_agent.map(|agent| agent.run).or_else(|| said.get(id).copied()) {
                if running_agent.is_none() {
                    gone.push(id.clone());
                }
                let claims = running_agent.and_then(RunningAgent::claims);
                self.carry_on_session(id, link.id, run, take(id), claims).await;
            } else {
                let (id, prompts) = (id.clone(), take(id));
                self.write(move |store| interrupt(store, &id, prompts)).await;
            }
        }
        // An agent idle as the earlier hub went is still running too.
        for agent in running.iter().filter(|agent| !at_work.contains(&agent.session_id)) {
            self.carry_on_session(&agent.session_id, link.id, agent.run, take(&agent.session_id), agent.claims()).await;
        }
        link.release(&self).await;
        // After what its worker said, so an agent that said it exited isn't said to have gone twice.
        for id in gone {
            self.route(&id, Item::AgentLost { link: link.id, reason: ExitReason::HubRestart }).await;
        }
        let (agents, build) = (running.len(), link.build());
        info!(%workspace_id, agents, build, "connected again to the worker an earlier hub left");
        if self.sessions.lock().live.values().any(|session| session.workspace_id == workspace_id) {
            link.keep();
            // Its watch of the branches goes on from the earlier hub, but sends only what changes, and what it sent
            // while there was no hub is lost, so asking again has it send what they are now.
            match self.ready_workspace(&workspace_id).await {
                Ok(ready) => self.watch_branches(&ready, &link).await,
                Err(e) => debug!(%workspace_id, "watching the branches of a worker carried on with: {}", e.message),
            }
            if link.is_another_build(self.build) {
                self.record_worker_build(&workspace_id, link.said_build()).await;
                self.replace_once_idle(workspace_id, link);
            }
        } else if being_moved {
            // Only this worker can say how far that got while it's still at it, so it's kept for the hub to ask once
            // it's serving, as [`Hub::settle_moves`] says.
            link.keep();
        } else {
            self.end_left_running(workspace_id, Some(link));
        }
    }

    /// `claims` are what the agent's worker says its run claims, which a worker of protocol 3 doesn't, whose run said
    /// them as it initialized, and the record kept them, since a hub that stops records no exit.
    pub(super) async fn carry_on_session(
        self: &Arc<Self>,
        id: &SessionId,
        link: u64,
        run: RunId,
        untaken: Vec<Seq>,
        claims: Option<RunCapabilities>,
    ) {
        let owned = id.clone();
        let read = self
            .blocking(move |store| {
                if let Some(claims) = &claims {
                    store.set_claims(&owned, claims)?;
                }
                Ok((store.get_session(&owned)?, store.open_approvals(&owned)?, store.recorded_from_worker(&owned)?))
            })
            .await;
        let started = read.and_then(|(stored, approvals, recorded)| {
            let carried = CarriedOver { link, run, recorded, untaken };
            self.sessions.lock().keep(id, || LiveSession::start(self, stored, &approvals, Some(carried)))
        });
        if let Err(e) = started {
            warn!(session = %id, "carrying on with a session: {}", e.message);
        }
    }

    /// Something only the store has to know, which is logged if it fails, for the next hub to try again.
    pub(super) async fn write(&self, change: impl FnOnce(&Store) -> Result<(), StoreError> + Send + 'static) {
        if let Err(e) = self.blocking(change).await {
            warn!("recording what an earlier hub left: {}", e.message);
        }
    }
}

/// A session an earlier hub left at work whose agent has gone. A prompt it hadn't taken never will be, its open
/// approvals can't be answered, and then its agent's exit is recorded, which does what it does with the hub there: the
/// session is interrupted, what it ran in the background went with it, and its queue is held. But a session whose only
/// work was a wait, which the hub keeps, had nothing cut off, as when this hub stops with it, so it's still at work in
/// the background, with the wait listed and its queue free, and its agent is started again once the wait ends.
pub(super) fn interrupt(store: &Store, id: &SessionId, untaken: Vec<Seq>) -> Result<(), StoreError> {
    let open_approvals = store.open_approvals(id)?;
    let mut progress = Progress::recorded(&store.get_session(id)?.session, &open_approvals, None, untaken.clone());
    drop_untaken_in(store, id, &mut progress, untaken)?;
    for approval_id in open_approvals {
        progress.record_in(store, id, cancelled_approval(approval_id))?;
    }
    let exited =
        EventBody::AgentExited { reason: ExitReason::HubRestart, code: None, signal: None, stderr_tail: vec![] };
    progress.outlive_this_hub(store.sessions_waiting()?.contains(id));
    progress.record_in(store, id, exited)
}

/// Drops the prompts a session that isn't at work had left untaken, as [`interrupt`] does those of one that is.
fn drop_untaken(store: &Store, id: &SessionId, untaken: Vec<Seq>) -> Result<(), StoreError> {
    let mut progress =
        Progress::recorded(&store.get_session(id)?.session, &store.open_approvals(id)?, None, untaken.clone());
    drop_untaken_in(store, id, &mut progress, untaken)
}

fn drop_untaken_in(
    store: &Store,
    id: &SessionId,
    progress: &mut Progress,
    untaken: Vec<Seq>,
) -> Result<(), StoreError> {
    for prompt in untaken {
        progress.record_in(store, id, EventBody::PromptDropped { prompt })?;
    }
    Ok(())
}

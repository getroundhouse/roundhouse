use super::super::Hub;
use super::disconnected_workspaces::Cause;
use crate::error::provider_failed;
use crate::session::{CarriedOver, Item, LiveSession, Progress, Starting};
use crate::spans;
use crate::store::{Store, StoreError, cancelled_approval};
use crate::worker_link::{WorkerLink, WorkerLinkId};
use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{
    InstanceId, ProviderName, RunCapabilities, RunId, Seq, SessionId, Workspace, WorkspaceId, WorkspaceStatus,
};
use protocol::worker::RunningAgent;
use providers::ProviderError;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::task::JoinSet;
use tracing::{Instrument as _, debug, info, warn};

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

/// What became of a workspace's worker that the hub couldn't reach.
#[derive(PartialEq, Eq)]
pub(super) enum Carried {
    /// Its sessions were carried on with its agents, or interrupted if it couldn't be reached or has gone.
    Done,
    /// It hasn't dialed this hub, and nothing says it's gone, so its workspace is disconnected: nothing was ended or
    /// interrupted.
    NotDialed,
}

/// The prompts each session's agent hadn't taken, which a session takes as it carries on or is interrupted.
type Untaken = Arc<Mutex<HashMap<SessionId, Vec<Seq>>>>;

impl Hub {
    /// Before the hub serves anything, so no session is live yet, and each one starts with the agent it still has, if
    /// it has one. Each workspace's worker is reached on its own, so a slow one holds up only its own sessions.
    pub(crate) async fn carry_on(self: &Arc<Self>, left: LeftAtWork) {
        let LeftAtWork { mut sessions, workspaces, untaken } = left;
        let at_work_by_workspace = sessions.clone();
        // First, so a worker an earlier hub left, which dials again, is admitted.
        self.listen_for_workers().await;
        let untaken: Untaken = Arc::new(Mutex::new(untaken));
        let mut carrying = JoinSet::new();
        // A worker can be running with no session at work, its agents idle, and the hub stops it as it stops, as it
        // would one it started.
        let being_moved: HashSet<WorkspaceId> = workspaces
            .iter()
            .filter(|workspace| matches!(workspace.status, WorkspaceStatus::Archiving | WorkspaceStatus::Restoring))
            .map(|workspace| workspace.id.clone())
            .collect();
        for workspace_id in self.with_workers_running(&workspaces).await {
            let at_work = sessions.remove(&workspace_id).unwrap_or_default();
            let being_moved = being_moved.contains(&workspace_id);
            let span = spans::workspace(&workspace_id);
            carrying
                .spawn(self.clone().carry_on_in(workspace_id, at_work, being_moved, untaken.clone()).instrument(span));
        }
        for id in sessions.into_values().flatten() {
            let prompts = untaken.lock().remove(&id).unwrap_or_default();
            self.write(move |store| interrupt(store, &id, prompts)).await;
        }
        carrying.join_all().await;
        // Every agent still running has been carried on with, so whatever the rest claimed, and what was only true of
        // them, such as the tool servers they had, are gone with them. The agents of a workspace whose worker hasn't
        // dialed the hub may still be running, so their sessions are left as the earlier hub had them, until the worker
        // does.
        self.unkept_observations_gone().await;
        let waiting_for: Vec<SessionId> = {
            at_work_by_workspace
                .iter()
                .filter(|(id, _)| self.disconnected.contains(id))
                .flat_map(|(_, ids)| ids)
                .cloned()
                .collect()
        };
        let running: Vec<SessionId> =
            self.sessions.lock().live.keys().cloned().chain(waiting_for.iter().cloned()).collect();
        self.write(move |store| store.clear_claims_but(&running)).await;
        // A session that wasn't at work, and whose agent isn't running, never will take these.
        let untaken = std::mem::take(&mut *untaken.lock());
        for (id, prompts) in untaken.into_iter().filter(|(id, _)| !waiting_for.contains(id)) {
            self.write(move |store| drop_untaken(store, &id, prompts)).await;
        }
    }

    /// Those of `workspaces` whose instance a worker may serve, which may be one an earlier hub left: one the hub
    /// admitted a dialing worker to, which dials again, or one its provider says a worker serves, as the local one says
    /// of a worker an earlier build left listening.
    pub(in super::super) async fn with_workers_running(&self, workspaces: &[Workspace]) -> Vec<WorkspaceId> {
        let mut by_provider: HashMap<&ProviderName, Vec<&Workspace>> = HashMap::new();
        for workspace in workspaces {
            by_provider.entry(&workspace.provider).or_default().push(workspace);
        }
        let (within, mut running) = (self.timers.before_serving, Vec::new());
        let dialed: HashSet<InstanceId> = match self.blocking(Store::instances_with_dialing_workers).await {
            Ok(dialed) => dialed.into_iter().collect(),
            Err(e) => {
                warn!(name: "dialed.unread", error = e.message, "Couldn't read which instances have workers that dial the hub");
                HashSet::new()
            }
        };
        for (name, workspaces) in by_provider {
            let listed = match self.provider(name) {
                Ok(provider) => match tokio::time::timeout(within, provider.workers_running()).await {
                    Ok(Ok(listed)) => Ok(listed),
                    // As every adapter answers, since the hub knows their workers by their dialing it.
                    Ok(Err(ProviderError::Unsupported(_))) => Ok(Vec::new()),
                    Ok(Err(e)) => Err(provider_failed(&e)),
                    Err(_) => Err(ApiError::new(ErrorCode::Worker, format!("no answer within {within:?}"))),
                },
                Err(e) => Err(e),
            };
            let listed: HashSet<InstanceId> = match listed {
                Ok(listed) => listed.into_iter().collect(),
                Err(e) => {
                    warn!(
                        name: "recovery.instances_unlisted",
                        provider = %name,
                        error = e.message,
                        "Couldn't list the instances an earlier hub left"
                    );
                    HashSet::new()
                }
            };
            let on_them = workspaces
                .into_iter()
                .filter(|workspace| dialed.contains(&workspace.instance_id) || listed.contains(&workspace.instance_id));
            running.extend(on_them.map(|workspace| workspace.id.clone()));
        }
        running
    }

    /// `being_moved` is whether the earlier hub was archiving or restoring the workspace. A worker that hasn't dialed
    /// the hub within [`Timers::before_serving`](crate::Timers::before_serving), and that its provider doesn't say is
    /// gone, leaves the workspace disconnected, as [`Hub::wait_for_worker`] says, and the hub serves meanwhile.
    pub(super) async fn carry_on_in(
        self: Arc<Self>,
        workspace_id: WorkspaceId,
        at_work: Vec<SessionId>,
        being_moved: bool,
        untaken: Untaken,
    ) {
        if self.carry_on_workspace(&workspace_id, at_work, being_moved, untaken).await == Carried::NotDialed {
            info!(
                name: "recovery.worker_disconnected",
                %workspace_id,
                "The worker an earlier hub left hasn't dialed this one, so it's waited for"
            );
            // Its provider was asked as the hub waited, and didn't say it's gone, so nothing waits for that to be known.
            match self.workspace(&workspace_id).await {
                Ok(workspace) => {
                    self.disconnected.mark(&workspace_id, &workspace.instance_id, Cause::LeftByEarlierHub, true, None);
                    let span = spans::workspace(&workspace_id);
                    tokio::spawn(self.clone().wait_for_worker(workspace_id).instrument(span));
                }
                Err(e) => {
                    warn!(
                        name: "recovery.workspace_unread",
                        %workspace_id,
                        error = e.message,
                        "Couldn't read a workspace whose worker hasn't dialed, so it isn't waited for"
                    );
                }
            }
        }
    }

    /// What's left at work in the workspace, whether an earlier hub left it or its worker's connection dropped, carried
    /// on with or recovered, or left for its worker to dial.
    pub(super) async fn carry_on_workspace(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
        at_work: Vec<SessionId>,
        being_moved: bool,
        untaken: Untaken,
    ) -> Carried {
        let take = |id: &SessionId| untaken.lock().remove(id).unwrap_or_default();
        let (link, running) = match self.worker_left_running(workspace_id).await {
            Ok(reached) => reached,
            Err(e) if e.code == ErrorCode::Unavailable => return Carried::NotDialed,
            Err(e) => {
                warn!(
                    name: "recovery.worker_unreached",
                    %workspace_id,
                    error = e.message,
                    "The worker couldn't be reached, so its sessions are interrupted"
                );
                // Each chat whose agent was on a connection that dropped is told it's lost, as it is when a worker's
                // found gone as the hub runs, and one that's recorded it is interrupted no more.
                let told = self.tell_agents_lost(workspace_id).await;
                for id in at_work {
                    let prompts = take(&id);
                    if !told.contains(&id) {
                        self.interrupt_session(id, prompts).await;
                    }
                }
                return Carried::Done;
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
                let claims = running_agent.map(RunningAgent::claims);
                self.carry_on_session(id, link.id, run, take(id), claims).await;
            } else {
                self.interrupt_session(id.clone(), take(id)).await;
            }
        }
        // An agent idle as the earlier hub went is still running too.
        for agent in running.iter().filter(|agent| !at_work.contains(&agent.session_id)) {
            let claims = Some(agent.claims());
            self.carry_on_session(&agent.session_id, link.id, agent.run, take(&agent.session_id), claims).await;
        }
        link.release(self).await;
        let terminals = self.carry_on_terminals(workspace_id, &link).await;
        // After what its worker said, so an agent that said it exited isn't said to have gone twice.
        for id in gone {
            self.route(&id, Item::AgentLost { link: link.id, reason: ExitReason::HubRestart }).await;
        }
        let (agents, build) = (running.len(), link.build());
        info!(
            name: "recovery.worker_connected",
            %workspace_id,
            agents,
            terminals,
            build,
            "Connected again to the worker"
        );
        self.clone().keep_or_end(workspace_id.clone(), link, terminals > 0, being_moved).await;
        Carried::Done
    }

    /// Keeps `link`, to the worker an earlier hub left, while its workspace has a session, or it has a terminal, as
    /// `has_terminals` says, or while the earlier hub was moving the workspace. Otherwise it's ended.
    async fn keep_or_end(
        self: Arc<Self>,
        workspace_id: WorkspaceId,
        link: Arc<WorkerLink>,
        has_terminals: bool,
        being_moved: bool,
    ) {
        let has_sessions = self.sessions.lock().live.values().any(|session| session.workspace_id == workspace_id);
        if has_sessions || has_terminals {
            link.keep();
            // Its watch of the branches goes on from the earlier hub, but sends only what changes, and what it sent
            // while there was no hub is lost, so asking again has it send what they are now.
            match self.ready_workspace(&workspace_id).await {
                Ok(ready) => self.watch_branches(&ready, &link).await,
                Err(e) => {
                    debug!(
                        name: "recovery.branches_unwatched",
                        %workspace_id,
                        error = e.message,
                        "Couldn't watch the branches of a worker carried on with"
                    );
                }
            }
            if link.is_another_build(self.build) {
                self.replace_once_idle(workspace_id, link).await;
            }
        } else if being_moved {
            // Only this worker can say how far that got while it's still at it, so it's kept for the hub to ask once
            // it's serving, as [`Hub::settle_moves`] says.
            link.keep();
        } else {
            self.end_left_running(workspace_id, Some(link));
        }
    }

    /// `claims` are what the agent's worker says its run claims, while it's running, and they go on the session's
    /// record. It's a write outside any event, which repairs a record that lacks them, so the claims stay a record, as
    /// `docs/principles/observations.md` says. One that has gone since says none, and the record keeps what it held,
    /// since a hub that stops records no exit.
    pub(super) async fn carry_on_session(
        self: &Arc<Self>,
        id: &SessionId,
        link: WorkerLinkId,
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
                Ok((Starting::read(store, &owned)?, store.recorded_from_worker(&owned)?))
            })
            .await;
        let started = read.and_then(|(starting, recorded)| {
            let carried = CarriedOver { link, run, recorded, untaken };
            let capabilities = starting.stored.session.agent_capabilities.clone();
            let live = self.sessions.lock().keep(id, || LiveSession::start(self, starting, Some(carried.clone())))?;
            // One a client followed before its worker dialed was made with no agent, and takes this one now.
            live.carry_on_with(carried, capabilities);
            Ok(live)
        });
        match started {
            // What it said that's only its own is kept, as the earlier hub kept it.
            Ok(live) => live.run_keeps_its_own(self, run).await,
            Err(e) => {
                warn!(
                    name: "recovery.session_failed",
                    session_id = %id,
                    error = e.message,
                    "Couldn't carry on with a session"
                );
            }
        }
    }

    /// Something only the store has to know, which is logged if it fails, for the next hub to try again.
    pub(super) async fn write(&self, change: impl FnOnce(&Store) -> Result<(), StoreError> + Send + 'static) {
        if let Err(e) = self.blocking(change).await {
            warn!(name: "recovery.unrecorded", error = e.message, "Couldn't record what an earlier hub left");
        }
    }
}

/// A session an earlier hub left at work whose agent has gone. A prompt it hadn't taken never will be, its open
/// approvals can't be answered, and then its agent's exit is recorded, which does what it does with the hub there: the
/// session is interrupted, what it ran in the background went with it, and its queue is held. But a session whose only
/// work was a wait, which the hub keeps, had nothing cut off, as when this hub stops with it, so it's still at work in
/// the background, with the wait listed and its queue free, and its agent is started again once the wait ends.
/// It records only the hub's own events, none of which the agent's failures in a row carry on through, so it starts from
/// none.
pub(super) fn interrupt(store: &Store, id: &SessionId, untaken: Vec<Seq>) -> Result<(), StoreError> {
    let open_approvals = store.open_approvals(id)?;
    let session = store.get_session(id)?.session;
    let mut progress = Progress::recorded(&session, &open_approvals, None, untaken.clone(), None);
    drop_untaken_in(store, id, &mut progress, untaken)?;
    for approval_id in open_approvals {
        progress.record_in(store, id, cancelled_approval(approval_id))?;
    }
    let exited =
        EventBody::AgentExited { reason: ExitReason::HubRestart, code: None, signal: None, stderr_tail: vec![] };
    progress.outlive_this_hub(store.sessions_waiting()?.contains(id));
    progress.record_in(store, id, exited)
}

/// Drops the prompts a session that isn't at work had left untaken, as [`interrupt`] does those of one that is, from no
/// failures in a row, as it does.
fn drop_untaken(store: &Store, id: &SessionId, untaken: Vec<Seq>) -> Result<(), StoreError> {
    let session = store.get_session(id)?.session;
    let mut progress = Progress::recorded(&session, &store.open_approvals(id)?, None, untaken.clone(), None);
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

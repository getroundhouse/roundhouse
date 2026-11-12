//! Carrying on with what an earlier hub left at work. It left its workers running, so this hub connects to each again,
//! and each session whose agent is still running carries on with it, with its open approvals, what it runs in the
//! background, and the prompts the agent hasn't taken yet. A session whose agent has gone is interrupted, as one whose
//! worker went away is, and so is each session on an instance with no worker running, or one that can't be reached,
//! such as one that's stopping.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{InstanceId, ProviderName, RunId, Seq, SessionId, SessionStatus, Workspace, WorkspaceId};
use protocol::provider::InstanceState;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use super::{BEFORE_SERVING, Hub};

#[cfg(test)]
mod tests;
use crate::error::provider_failed;
use crate::session::{CarriedOver, Item, LiveSession};
use crate::store::{SessionChange, StatusChange, Store, StoreError, cancelled_approval};

/// What an earlier hub left at work: its sessions at work, by workspace, every workspace that isn't archived, for
/// finding the workers it left running, and the prompts each session's agent hadn't taken.
pub(crate) struct LeftAtWork {
    sessions: HashMap<WorkspaceId, Vec<SessionId>>,
    workspaces: Vec<Workspace>,
    untaken: HashMap<SessionId, Vec<Seq>>,
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
        for workspace_id in self.with_instances_running(&workspaces).await {
            let at_work = sessions.remove(&workspace_id).unwrap_or_default();
            carrying.spawn(self.clone().carry_on_in(workspace_id, at_work, untaken.clone()));
        }
        for id in sessions.into_values().flatten() {
            let prompts = untaken.lock().remove(&id).unwrap_or_default();
            self.write(move |store| interrupt(store, &id, prompts)).await;
        }
        carrying.join_all().await;
        // A session that wasn't at work, and whose agent isn't running, never will take these.
        let untaken = std::mem::take(&mut *untaken.lock());
        for (id, prompts) in untaken {
            self.write(move |store| drop_untaken(store, &id, prompts)).await;
        }
    }

    /// Those of `workspaces` whose instance their provider says is running, which a worker may be on.
    async fn with_instances_running(&self, workspaces: &[Workspace]) -> Vec<WorkspaceId> {
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

    async fn carry_on_in(self: Arc<Self>, workspace_id: WorkspaceId, at_work: Vec<SessionId>, untaken: Untaken) {
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
            let running_run = running.iter().find(|agent| agent.session_id == *id).map(|agent| agent.run);
            if let Some(run) = running_run.or_else(|| said.get(id).copied()) {
                if running_run.is_none() {
                    gone.push(id.clone());
                }
                self.carry_on_session(id, link.id, run, take(id)).await;
            } else {
                let (id, prompts) = (id.clone(), take(id));
                self.write(move |store| interrupt(store, &id, prompts)).await;
            }
        }
        // An agent idle as the earlier hub went is still running too.
        for agent in running.iter().filter(|agent| !at_work.contains(&agent.session_id)) {
            self.carry_on_session(&agent.session_id, link.id, agent.run, take(&agent.session_id)).await;
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
            if link.is_another_build() {
                self.replace_once_idle(workspace_id, link);
            }
        } else {
            self.end_left_running(workspace_id, Some(link));
        }
    }

    async fn carry_on_session(self: &Arc<Self>, id: &SessionId, link: u64, run: RunId, untaken: Vec<Seq>) {
        let owned = id.clone();
        let read = self
            .blocking(move |store| {
                Ok((store.get_session(&owned)?, store.open_approvals(&owned)?, store.recorded_from_worker(&owned)?))
            })
            .await;
        let started = read.and_then(|(stored, approvals, recorded)| {
            let carried = CarriedOver { link, run, recorded, untaken };
            self.sessions.lock().keep(id, || LiveSession::start(self, stored, approvals, Some(carried)))
        });
        if let Err(e) = started {
            warn!(session = %id, "carrying on with a session: {}", e.message);
        }
    }

    /// Something only the store has to know, which is logged if it fails, for the next hub to try again.
    async fn write(&self, change: impl FnOnce(&Store) -> Result<(), StoreError> + Send + 'static) {
        if let Err(e) = self.blocking(change).await {
            warn!("recording what an earlier hub left: {}", e.message);
        }
    }
}

/// A session an earlier hub left at work whose agent has gone. A prompt it hadn't taken never will be, its open
/// approvals can't be answered, and it took what it was running in the background with it. Its queue is held, as when
/// the agent exits mid-turn with the hub still there. But a session whose only work was a wait, which the hub keeps,
/// had nothing cut off, so it's still at work in the background, with the wait listed and its queue free, and its
/// agent is started again once the wait ends.
fn interrupt(store: &Store, id: &SessionId, untaken: Vec<Seq>) -> Result<(), StoreError> {
    drop_untaken(store, id, untaken)?;
    for approval_id in store.open_approvals(id)? {
        store.append(id, None, &cancelled_approval(approval_id), None, None, None)?;
    }
    let exited =
        EventBody::AgentExited { reason: ExitReason::HubRestart, code: None, signal: None, stderr_tail: vec![] };
    let only_waiting = store.sessions_waiting()?.contains(id) && {
        let session = store.get_session(id)?.session;
        session.status.only_waits(&session.background_tasks)
    };
    let status = if only_waiting {
        SessionStatus::Background.into()
    } else {
        StatusChange { background_tasks: Some(Vec::new()), ..SessionStatus::Interrupted.into() }
    };
    let change = SessionChange { status: Some(status), queue_held: Some(!only_waiting), recorded: None };
    store.append(id, None, &exited, None, None, Some(change))?;
    Ok(())
}

fn drop_untaken(store: &Store, id: &SessionId, untaken: Vec<Seq>) -> Result<(), StoreError> {
    for prompt in untaken {
        store.append(id, None, &EventBody::PromptDropped { prompt }, None, None, None)?;
    }
    Ok(())
}

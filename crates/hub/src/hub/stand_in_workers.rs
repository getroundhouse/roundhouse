//! A hub whose every new workspace has a stand-in for its worker, for the hub's own tests of what it does as it makes
//! a workspace, and with one once it's made, such as giving it its first chat and sending that chat a prompt.
//!
//! A hub asks a workspace's provider for its instance before it wants the instance's worker, and takes the worker it
//! has for a workspace, in its map of them, rather than start one. So the provider here puts a stand-in for the worker
//! in that map as it's asked for the instance, as a test does for `w` with [`serving`](super::tests::serving), and the
//! hub never asks it to start a worker, or how to reach one.

use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use protocol::model::{InstanceId, ProviderName, Workspace};
use protocol::worker::{
    AgentStarted, AttachmentWritten, NamesGitHasReply, SetupDone, WorkerRequest, WorkerResponse, WorkspaceReady,
};
use providers::{Capabilities, Instance, Provider, ProviderError, Size, WorkerBinary, WorkerCommand};

use super::Hub;
use super::tests::{claimed, hub_provided_by};
use crate::worker_link::WorkerLink;

/// A hub as [`hub_in`](super::tests::hub_in) makes one, which gives each workspace it makes a stand-in for its worker,
/// and what those are asked. Each answers as [`answer`] says.
pub(super) fn hub_with_stand_in_workers(dir: &Path) -> (Arc<Hub>, Arc<Asked>) {
    let asked = Arc::new(Asked::default());
    let hub = Arc::new_cyclic(|hub| {
        let provider = WithWorkers { hub: hub.clone(), asked: asked.clone() };
        hub_provided_by(dir, Arc::new(provider))
    });
    (hub, asked)
}

/// What a hub's stand-ins for its workspaces' workers have been asked.
#[derive(Default)]
pub(super) struct Asked(Mutex<Vec<WorkerRequest>>);

impl Asked {
    /// What `picked` makes of each request it picks, in the order each worker took them, which is the order the hub
    /// sent them in. A worker has taken a request by the time the hub has its answer. One the hub doesn't wait for,
    /// such as an agent's launch, it takes some time after, so a test waits for it.
    pub(super) fn requests<T>(&self, picked: impl Fn(&WorkerRequest) -> Option<T>) -> Vec<T> {
        self.0.lock().iter().filter_map(picked).collect()
    }
}

/// What a stand-in worker answers, which refuses nothing. Git has no name a workspace might take, so each worktree
/// asked for is made, in `/ws`, on the branch asked for, its setup script passes, a file a prompt carries is written
/// to it, and an agent starts there and takes what it's sent. Anything else is answered as done, with nothing to say.
fn answer(request: WorkerRequest) -> WorkerResponse {
    if let WorkerRequest::NamesGitHas(_) = request {
        return WorkerResponse::NamesGitHas(NamesGitHasReply { names: Vec::new() });
    }
    if let WorkerRequest::PrepareWorkspace(prepare) = request {
        return WorkerResponse::WorkspaceReady(WorkspaceReady {
            path: format!("/ws/{}", prepare.workspace_name),
            branch: prepare.branch,
            base_ref: prepare.base_ref.unwrap_or_else(|| "origin/main".to_string()),
            warnings: Vec::new(),
            branches: None,
        });
    }
    if let WorkerRequest::SetupWorkspace(_) = request {
        return WorkerResponse::SetupDone(SetupDone { code: Some(0) });
    }
    if let WorkerRequest::AttachmentWrite(write) = request {
        let path = format!("{}/.context/attachments/{}", write.path, write.name);
        return WorkerResponse::AttachmentWritten(AttachmentWritten { path });
    }
    if let WorkerRequest::AgentStart(spec) = request {
        let capabilities = Some(Box::new(claimed()));
        return WorkerResponse::AgentStarted(AgentStarted {
            pid: 7,
            run: spec.run,
            already_running: false,
            capabilities,
        });
    }
    WorkerResponse::Ok
}

/// A provider whose every instance comes with a stand-in for its worker, which the hub has from the moment the
/// instance is made. It goes by the local provider's name, so `w`, which a test's store has as the local provider's,
/// is its too.
struct WithWorkers {
    hub: Weak<Hub>,
    asked: Arc<Asked>,
}

#[async_trait]
impl Provider for WithWorkers {
    fn name(&self) -> &'static str {
        ProviderName::LOCAL
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn starts_or_stops_within(&self) -> Duration {
        Duration::from_secs(1)
    }

    /// Has a stand-in be the worker of the workspace whose instance this is, in place of any it had.
    async fn create(
        &self,
        instance_id: &InstanceId,
        _: Option<&InstanceId>,
        _: Option<Size>,
        _: &str,
    ) -> Result<(), ProviderError> {
        let hub = self.hub.upgrade().expect("a hub asks for an instance");
        let workspaces = hub.store.list_workspaces(true).expect("the store lists its workspaces");
        let its = |workspace: &Workspace| workspace.instance_id == *instance_id;
        let workspace = workspaces.into_iter().find(its).expect("a hub records a workspace before its instance");
        let asked = self.asked.clone();
        let link = WorkerLink::answering(move |request| {
            asked.0.lock().push(request.clone());
            Some(Ok(answer(request)))
        });
        hub.workers.lock().insert(workspace.id, link);
        Ok(())
    }

    async fn start(&self, _: &InstanceId, _: &[WorkerBinary]) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn stop(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn kill(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn destroy(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Instance>, ProviderError> {
        Ok(Vec::new())
    }

    /// Refused, since a stand-in has no command to reach it by: once one has gone, its workspace has no worker.
    async fn connect(&self, _: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        Err(ProviderError::Unsupported("run a worker"))
    }

    async fn make_base_image(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported("make base images"))
    }
}

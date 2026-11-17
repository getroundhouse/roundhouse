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
use protocol::capability::{Control, turns};
use protocol::client::ApiError;
use protocol::event::EventBody;
use protocol::model::{
    AgentKind, InstanceId, ProviderName, Seq, Session, SessionId, SessionSettingsChange, Workspace, WorkspaceId,
};
use protocol::worker::{
    AgentStarted, AttachmentWritten, NamesGitHasReply, SetupDone, TitleFor, TitleReply, WorkerRequest, WorkerResponse,
    WorkspaceReady,
};
use providers::{Capabilities, Instance, Provider, ProviderError, Size, WorkerBinary, WorkerCommand};

use super::Hub;
use super::tests::{claimed, flushed, hub_provided_by};
use crate::session::Item;
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
        let capabilities = Some(Box::new(claims()));
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

/// A workspace made from the repository `r`, whose worktree its worker's stand-in makes, and a chat of Claude's in it,
/// which nothing has been sent.
pub(super) async fn a_chat(hub: &Arc<Hub>) -> (Workspace, Session) {
    let workspace = hub.workspace_create("r".into(), None, None, None).await.expect("a stand-in makes the worktree");
    let unchanged = SessionSettingsChange::default();
    let chat = hub.session_create(workspace.id.clone(), AgentKind::Claude, unchanged, None).await.expect("a new chat");
    (workspace, chat)
}

impl Asked {
    /// Where each prompt a stand-in was asked to give the agent of `session` is recorded, in the order it was asked.
    pub(super) fn prompts_given(&self, session: &SessionId) -> Vec<Seq> {
        self.requests(|request| {
            let WorkerRequest::AgentControl(asked) = request else { return None };
            let Control::Prompt { seq, .. } = asked.control else { return None };
            (asked.session_id == *session).then_some(seq)
        })
    }
}

/// What the agent of the chat `session` says, as its worker passes it on: each of `said`, as the run its stand-in last
/// started for the chat, once the chat has taken them all in. A stand-in's agent says nothing by itself, so a test says
/// for it what an agent would, such as that it took a prompt, and that its turn has ended.
pub(super) async fn the_agent_says(
    hub: &Arc<Hub>,
    asked: &Asked,
    session: &SessionId,
    said: impl IntoIterator<Item = EventBody>,
) {
    let launched = |request: &WorkerRequest| {
        let WorkerRequest::AgentStart(spec) = request else { return None };
        (spec.session_id == *session).then_some(spec.run)
    };
    let run = asked.requests(launched).pop().expect("only an agent that was started says anything");
    for body in said {
        hub.route(session, Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None })
            .await;
    }
    flushed(&hub.live_session(session).await.expect("a chat that's there")).await;
}

/// Has the worker of `workspace` be a stand-in that answers a call for a title as the worker does once it has asked its
/// agent's CLI, with what `title` makes of the call: the title the model gave, tidied as the worker tidies it, or why
/// there's none. It answers everything else as [`answer`] says, and keeps each request in `asked`, as the stand-in it
/// replaces did, so a test sets it before anything is asked of the workspace's worker that it's about.
pub(super) fn titling(
    hub: &Hub,
    workspace: &WorkspaceId,
    asked: &Arc<Asked>,
    title: impl Fn(&TitleFor) -> Result<String, ApiError> + Send + 'static,
) {
    let asked = asked.clone();
    let link = WorkerLink::answering(move |request| {
        asked.0.lock().push(request.clone());
        if let WorkerRequest::TitleFor(call) = &request {
            return Some(title(call).map(|title| WorkerResponse::Title(TitleReply { title })));
        }
        Some(Ok(answer(request)))
    });
    hub.workers.lock().insert(workspace.clone(), link);
}

/// What a stand-in's agent claims as it starts: what Claude claims as the hub's tests describe it, as [`claimed`] says,
/// and cancelling a turn, as Claude's CLI claims it, so a test can stop a turn as the person does.
fn claims() -> protocol::capability::Capabilities {
    let mut claims = claimed();
    claims.turns.cancel = Some(turns::Cancel { stops: turns::CancelStops::Nothing });
    claims
}

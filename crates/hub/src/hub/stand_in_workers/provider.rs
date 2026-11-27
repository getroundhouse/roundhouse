use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use protocol::capability::{context, settings, turns};
use protocol::client::ApiError;
use protocol::dialing::WorkerTicket;
use protocol::model::{AgentAccount, CacheTtl, ChangeSummary, InstanceId, ProviderName, Workspace};
use protocol::worker::{
    AgentStarted, ArchivedBranch, ArchivedBundle, ArchivedGit, AttachmentWritten, NamesGitHasReply, ServeTools,
    SetupDone, ToolsServing, WorkerRequest, WorkerResponse, WorkspaceReady,
};
use providers::{
    Capabilities, Instance, Provider, ProviderError, Size, WorkerBinary, WorkerCommand, WorkerEnvironment,
};

use super::super::Hub;
use super::super::tests::{claimed, described, hub_provided_by};
use crate::worker_link::WorkerLink;

/// A hub as [`hub_in`](super::super::tests::hub_in) makes one, which gives each workspace it makes a stand-in for its
/// worker, and what those are asked. Each answers as [`answer`] says.
pub(in super::super) fn hub_with_stand_in_workers(dir: &Path) -> (Arc<Hub>, Arc<Asked>) {
    hub_whose_stand_ins_answer_first(dir, vec![described()], |_| None)
}

/// The same, whose agents are as `agents` says, and whose every stand-in answers what `first` answers, from the moment
/// its workspace's instance is made, and everything else as [`answer`] says, as [`answering_first`](super::answering_first)
/// has a workspace's do once it's made. So a test can have git have a name, or a worktree not be made.
pub(in super::super) fn hub_whose_stand_ins_answer_first(
    dir: &Path,
    agents: Vec<protocol::model::AgentDescription>,
    first: impl Fn(&WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + Sync + 'static,
) -> (Arc<Hub>, Arc<Asked>) {
    hub_with_environment(dir, WorkerEnvironment::HUB_MACHINE, agents, first)
}

/// The same, whose provider declares `environment` for its workers.
pub(in super::super) fn hub_with_environment(
    dir: &Path,
    environment: WorkerEnvironment,
    agents: Vec<protocol::model::AgentDescription>,
    first: impl Fn(&WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + Sync + 'static,
) -> (Arc<Hub>, Arc<Asked>) {
    let (asked, first) = (Arc::new(Asked::default()), Arc::new(first));
    let hub = Arc::new_cyclic(|hub| {
        let provider = WithWorkers { hub: hub.clone(), asked: asked.clone(), first, environment };
        hub_provided_by(dir, Arc::new(provider), agents)
    });
    (hub, asked)
}

type DestroyHook = Box<dyn Fn(&InstanceId) + Send + Sync>;

/// What a hub's stand-ins for its workspaces' workers have been asked, and how often their provider was asked to
/// create an instance or start a worker, plus a switch to make `Provider::environment` fail on demand.
#[derive(Default)]
pub(in super::super) struct Asked {
    pub(super) requests: Mutex<Vec<WorkerRequest>>,
    starts: AtomicUsize,
    creates: AtomicUsize,
    destroys: AtomicUsize,
    fail_environment: AtomicBool,
    fail_destroy: AtomicBool,
    on_destroy: Mutex<Option<DestroyHook>>,
}

impl Asked {
    /// What `picked` makes of each request it picks, in the order each worker took them, which is the order the hub
    /// sent them in. A worker has taken a request by the time the hub has its answer. One the hub doesn't wait for,
    /// such as an agent's launch, it takes some time after, so a test waits for it.
    pub(in super::super) fn requests<T>(&self, picked: impl Fn(&WorkerRequest) -> Option<T>) -> Vec<T> {
        self.requests.lock().iter().filter_map(picked).collect()
    }

    /// How often the provider was asked to start a worker.
    pub(in super::super) fn workers_started(&self) -> usize {
        self.starts.load(Ordering::SeqCst)
    }

    /// How often the provider was asked to create an instance.
    pub(in super::super) fn instances_created(&self) -> usize {
        self.creates.load(Ordering::SeqCst)
    }

    /// How often the provider was asked to destroy an instance.
    pub(in super::super) fn instances_destroyed(&self) -> usize {
        self.destroys.load(Ordering::SeqCst)
    }

    /// Makes subsequent `Provider::environment` calls fail, so a test can verify operations after creation don't call
    /// the provider's adapter.
    pub(in super::super) fn fail_environment_reads(&self) {
        self.fail_environment.store(true, Ordering::SeqCst);
    }

    /// Controls whether subsequent `Provider::destroy` calls fail.
    pub(in super::super) fn fail_destroys(&self, fail: bool) {
        self.fail_destroy.store(fail, Ordering::SeqCst);
    }

    /// Runs `hook` at the start of every `Provider::destroy`, so a test can assert state (such as a saved archive
    /// bundle on disk) is durable before destruction begins.
    pub(in super::super) fn on_destroy(&self, hook: impl Fn(&InstanceId) + Send + Sync + 'static) {
        *self.on_destroy.lock() = Some(Box::new(hook));
    }
}

/// What a stand-in worker answers, which refuses nothing. Git has no name a workspace might take, so each worktree
/// asked for is made, in `/ws`, on the branch asked for, with nothing in it to lose and nothing changed, its setup
/// script passes, a file a prompt carries is written to it, its tools are served on the port asked for, or else on
/// 4100, and an agent starts there and takes what it's sent. Nobody
/// is logged in to an agent's CLI on its machine, so the account it's asked an agent runs under is nobody's. Anything
/// else is answered as done, with nothing to say.
pub(super) fn answer(request: WorkerRequest) -> WorkerResponse {
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
    if let WorkerRequest::SummarizeChanges(_) = request {
        return WorkerResponse::ChangeSummary(ChangeSummary::new("c0ffee".into(), Vec::new()));
    }
    if let WorkerRequest::GitStatus(_) = request {
        return WorkerResponse::GitStatus(protocol::model::GitStatus::default());
    }
    if let WorkerRequest::AttachmentWrite(write) = request {
        let path = format!("{}/.context/attachments/{}", write.path, write.name);
        return WorkerResponse::AttachmentWritten(AttachmentWritten { path });
    }
    if let WorkerRequest::AccountFor(_) = request {
        return WorkerResponse::Account(AgentAccount { logged_in: false, ..AgentAccount::default() });
    }
    if let WorkerRequest::ServeTools(ServeTools { port }) = request {
        return WorkerResponse::ToolsServing(ToolsServing { port: port.unwrap_or(4100) });
    }
    if let WorkerRequest::AgentStart(spec) = request {
        let capabilities = Box::new(claims());
        return WorkerResponse::AgentStarted(AgentStarted {
            pid: 7,
            run: spec.run,
            already_running: false,
            capabilities,
        });
    }
    if let WorkerRequest::CaptureArchive(_) = request {
        return WorkerResponse::ArchivedGit(ArchivedGit {
            branches: vec![ArchivedBranch { name: "birch/start".into(), commit: "c0ffee".into() }],
            bundle: Some(ArchivedBundle::from(b"BODY".as_slice())),
            bundle_omitted: false,
        });
    }
    WorkerResponse::Ok
}

/// What a stand-in's agent claims as it starts: what Claude claims as the hub's tests describe it, as [`claimed`] says,
/// cancelling a turn and taking a change of its model, mode and effort as it runs, the prompt cache it marks, and its
/// account's limits, as Claude's CLI claims them, so a test can stop a turn, or change a chat's settings, as the person
/// does, or have its agent ask its model, or report what it heard of its limits.
fn claims() -> protocol::capability::Capabilities {
    let mut claims = claimed();
    let ttls = vec![CacheTtl::FiveMinutes, CacheTtl::OneHour];
    claims.context.cache = Some(context::Cache { kind: context::CacheKind::Marked { ttls } });
    claims.turns.cancel = Some(turns::Cancel { stops: turns::CancelStops::Nothing });
    let (now, next_turn) = (Some(settings::Takes::Now), Some(settings::Takes::NextTurn));
    let live = settings::LiveChanges { model: next_turn, mode: now, effort: next_turn, ..Default::default() };
    claims.settings.live_changes = Some(live);
    claims.account.limits = Some(protocol::capability::Claimed {});
    claims
}

/// A provider whose every instance comes with a stand-in for its worker, which the hub has from the moment the
/// instance is made. It goes by the local provider's name, so `w`, which a test's store has as the local provider's,
/// is its too.
struct WithWorkers {
    hub: Weak<Hub>,
    asked: Arc<Asked>,
    first: Arc<First>,
    environment: WorkerEnvironment,
}

/// What a test has a stand-in answer before anything else, if it's what the stand-in is asked.
type First = dyn Fn(&WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + Sync;

#[async_trait]
impl Provider for WithWorkers {
    fn name(&self) -> &'static str {
        ProviderName::LOCAL
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    async fn environment(&self) -> Result<WorkerEnvironment, ProviderError> {
        if self.asked.fail_environment.load(Ordering::SeqCst) {
            return Err(ProviderError::io("environment", std::io::Error::other("adapter unavailable")));
        }
        Ok(self.environment)
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
    ) -> Result<(), ProviderError> {
        self.asked.creates.fetch_add(1, Ordering::SeqCst);
        let hub = self.hub.upgrade().expect("a hub asks for an instance");
        let workspaces = hub.store.list_workspaces(true).expect("the store lists its workspaces");
        let its = |workspace: &Workspace| workspace.instance_id == *instance_id;
        let workspace = workspaces.into_iter().find(its).expect("a hub records a workspace before its instance");
        let (asked, first) = (self.asked.clone(), self.first.clone());
        let link = WorkerLink::answering(move |request| {
            asked.requests.lock().push(request.clone());
            Some(first(&request).unwrap_or_else(|| Ok(answer(request))))
        });
        hub.workers.lock().insert(workspace.id, link);
        Ok(())
    }

    async fn start(&self, _: &InstanceId, _: &[WorkerBinary], _: Option<&WorkerTicket>) -> Result<(), ProviderError> {
        self.asked.starts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn end_worker(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn kill_worker(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn destroy(&self, id: &InstanceId) -> Result<(), ProviderError> {
        self.asked.destroys.fetch_add(1, Ordering::SeqCst);
        if let Some(hook) = self.asked.on_destroy.lock().as_ref() {
            hook(id);
        }
        if self.asked.fail_destroy.load(Ordering::SeqCst) {
            return Err(ProviderError::io("destroy", std::io::Error::other("transient destroy failure")));
        }
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

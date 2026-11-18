//! A hub whose every new workspace has a stand-in for its worker, for the hub's own tests of what it does as it makes
//! a workspace, and with one once it's made, such as giving it its first chat and sending that chat a prompt.
//!
//! A hub asks a workspace's provider for its instance before it wants the instance's worker, and takes the worker it
//! has for a workspace, in its map of them, rather than start one. So the provider here puts a stand-in for the worker
//! in that map as it's asked for the instance, as a test does for `w` with [`serving`](super::tests::serving), and the
//! hub never asks it to start a worker, or how to reach one.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use protocol::capability::conversation::Start;
use protocol::capability::settings::Settings;
use protocol::capability::{Control, Report, context, settings, turns};
use protocol::client::ApiError;
use protocol::event::{AgentRunState, EventBody, TurnOutcome};
use protocol::fixtures::turn_ended;
use protocol::model::{
    AgentAccount, AgentKind, AgentSessionId, AttachmentId, CacheRequest, CacheTtl, InstanceId, ProviderName, RunId,
    Seq, Session, SessionId, Workspace, WorkspaceId,
};
use protocol::prompt::Prompt;
use protocol::worker::{
    AccountFor, AgentSpec, AgentStarted, AttachmentWritten, NamesGitHasReply, SetupDone, TitleFor, TitleReply,
    WorkerRequest, WorkerResponse, WorkspaceReady,
};
use providers::{Capabilities, Instance, Provider, ProviderError, Size, WorkerBinary, WorkerCommand};

use super::Hub;
use super::tests::{claimed, described, flushed, hub_provided_by};
use crate::session::Item;
use crate::worker_link::WorkerLink;

/// A hub as [`hub_in`](super::tests::hub_in) makes one, which gives each workspace it makes a stand-in for its worker,
/// and what those are asked. Each answers as [`answer`] says.
pub(super) fn hub_with_stand_in_workers(dir: &Path) -> (Arc<Hub>, Arc<Asked>) {
    hub_whose_stand_ins_answer_first(dir, vec![described()], |_| None)
}

/// The same, whose agents are as `agents` says, and whose every stand-in answers what `first` answers, from the moment
/// its workspace's instance is made, and everything else as [`answer`] says, as [`answering_first`] has a workspace's
/// do once it's made. So a test can have git have a name, or a worktree not be made.
pub(super) fn hub_whose_stand_ins_answer_first(
    dir: &Path,
    agents: Vec<protocol::model::AgentDescription>,
    first: impl Fn(&WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + Sync + 'static,
) -> (Arc<Hub>, Arc<Asked>) {
    let (asked, first) = (Arc::new(Asked::default()), Arc::new(first));
    let hub = Arc::new_cyclic(|hub| {
        let provider = WithWorkers { hub: hub.clone(), asked: asked.clone(), first };
        hub_provided_by(dir, Arc::new(provider), agents)
    });
    (hub, asked)
}

/// What a hub's stand-ins for its workspaces' workers have been asked, and how often their provider was asked to start
/// one, which a hub does for a workspace whose stand-in it no longer has.
#[derive(Default)]
pub(super) struct Asked(Mutex<Vec<WorkerRequest>>, AtomicUsize);

impl Asked {
    /// What `picked` makes of each request it picks, in the order each worker took them, which is the order the hub
    /// sent them in. A worker has taken a request by the time the hub has its answer. One the hub doesn't wait for,
    /// such as an agent's launch, it takes some time after, so a test waits for it.
    pub(super) fn requests<T>(&self, picked: impl Fn(&WorkerRequest) -> Option<T>) -> Vec<T> {
        self.0.lock().iter().filter_map(picked).collect()
    }

    /// How often the provider was asked to start a worker.
    pub(super) fn workers_started(&self) -> usize {
        self.1.load(Ordering::SeqCst)
    }
}

/// What a stand-in worker answers, which refuses nothing. Git has no name a workspace might take, so each worktree
/// asked for is made, in `/ws`, on the branch asked for, with nothing in it to lose, its setup script passes, a file a
/// prompt carries is written to it, and an agent starts there and takes what it's sent. Nobody is logged in to an
/// agent's CLI on its machine, so the account it's asked an agent runs under is nobody's. Anything else is answered as
/// done, with nothing to say.
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
    first: Arc<First>,
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
        let (asked, first) = (self.asked.clone(), self.first.clone());
        let link = WorkerLink::answering(move |request| {
            asked.0.lock().push(request.clone());
            Some(first(&request).unwrap_or_else(|| Ok(answer(request))))
        });
        hub.workers.lock().insert(workspace.id, link);
        Ok(())
    }

    async fn start(&self, _: &InstanceId, _: &[WorkerBinary]) -> Result<(), ProviderError> {
        self.asked.1.fetch_add(1, Ordering::SeqCst);
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
    let unchanged = Settings::default();
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
    let run = last_started(asked, session).expect("only an agent that was started says anything");
    for body in said {
        hub.route(session, Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None })
            .await;
    }
    flushed(&hub.live_session(session).await.expect("a chat that's there")).await;
}

/// The run a stand-in last started for the chat `session`, if any.
fn last_started(asked: &Asked, session: &SessionId) -> Option<RunId> {
    asked.launches(session).pop().map(|spec| spec.run)
}

impl Asked {
    /// Each launch of the agent of `session` a stand-in was asked for, in the order it was asked.
    pub(super) fn launches(&self, session: &SessionId) -> Vec<AgentSpec> {
        self.requests(|request| {
            let WorkerRequest::AgentStart(spec) = request else { return None };
            (spec.session_id == *session).then(|| spec.clone())
        })
    }
}

/// How long a test gives a hub with stand-ins for its workers to do what it does on a task of its own, such as giving
/// the agent a prompt once it's recorded, or learning whose a launch's login is, or to answer what needs a worker,
/// such as a move that stops the agent. Far longer than any of that takes with a stand-in, so only what never comes
/// runs out of it.
pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

/// Sends the chat `session` `prompt`, which its agent takes once the hub has given it, and then ends its turn, as
/// Claude's CLI does. An agent the prompt launched first says it has opened the conversation the hub launched it on, as
/// the CLI does as it starts, and the agent asks its model `asking` before it takes the prompt, if anything. Gives back
/// the launch whose agent took it, which is the one that ran last.
pub(super) async fn a_turn(
    hub: &Arc<Hub>,
    asked: &Asked,
    session: &SessionId,
    prompt: Prompt<AttachmentId>,
    asking: Option<CacheRequest>,
) -> AgentSpec {
    let before = asked.launches(session).len();
    let (seq, _) = hub.session_prompt(session.clone(), None, prompt).await.expect("a prompt the hub takes");
    let given = || {
        let given = asked.prompts_given(session);
        if given.contains(&seq) { Ok(()) } else { Err(format!("the agent was given only {given:?}")) }
    };
    waiting::task::until_ok(TIMEOUT, given).await;
    let launches = asked.launches(session);
    let spec = launches.last().expect("an agent was launched to give it to").clone();
    if launches.len() > before {
        the_agent_says(hub, asked, session, [opened(&spec.start)]).await;
    }
    if let Some(request) = asking {
        the_agent_asks_its_model(hub, asked, session, request).await;
    }
    let ended = [
        EventBody::PromptTaken { prompt: seq },
        turn_ended(TurnOutcome::Success),
        EventBody::AgentState { state: AgentRunState::Idle },
    ];
    the_agent_says(hub, asked, session, ended).await;
    spec
}

/// What Claude's CLI says as it starts on `start`: that it has opened the conversation it was given, or a new one of
/// its own when it was given none.
fn opened(start: &Start) -> EventBody {
    let conversation = match start {
        Start::New { conversation: Some(conversation) } | Start::Resume { conversation } => conversation.clone(),
        Start::New { conversation: None } | Start::Fork { .. } | Start::Import { .. } => {
            AgentSessionId::from("a-conversation-of-its-own")
        }
    };
    EventBody::AgentInit {
        capabilities: None,
        agent_session_id: Some(conversation),
        model: None,
        cwd: None,
        mode: None,
        agent_version: None,
    }
}

/// Has the worker of `workspace` be a stand-in that answers what `first` answers, as a test says, and everything else
/// as [`answer`] says, and keeps each request in `asked`, as the stand-in it replaces did. So a test sets it before
/// anything it's about is asked of the workspace's worker.
pub(super) fn answering_first(
    hub: &Hub,
    workspace: &WorkspaceId,
    asked: &Arc<Asked>,
    first: impl Fn(&WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
) {
    let asked = asked.clone();
    let link = WorkerLink::answering(move |request| {
        asked.0.lock().push(request.clone());
        Some(first(&request).unwrap_or_else(|| Ok(answer(request))))
    });
    hub.workers.lock().insert(workspace.clone(), link);
}

/// Has the worker of `workspace` be a stand-in that answers a call for a title as the worker does once it has asked its
/// agent's CLI, with what `title` makes of the call: the title the model gave, tidied as the worker tidies it, or why
/// there's none, as [`answering_first`] has it.
pub(super) fn titling(
    hub: &Hub,
    workspace: &WorkspaceId,
    asked: &Arc<Asked>,
    title: impl Fn(&TitleFor) -> Result<String, ApiError> + Send + 'static,
) {
    answering_first(hub, workspace, asked, move |request| {
        let WorkerRequest::TitleFor(call) = request else { return None };
        Some(title(call).map(|title| WorkerResponse::Title(TitleReply { title })))
    });
}

/// Has the worker of `workspace` be a stand-in on a machine whose agent's CLI is logged in as `machine` says, each time
/// it's asked, so a test can log in as someone else meanwhile: the account it reads, as the worker answers once it has
/// asked the CLI, or why it couldn't say, as [`answering_first`] has it.
pub(super) fn logged_in(
    hub: &Hub,
    workspace: &WorkspaceId,
    asked: &Arc<Asked>,
    machine: impl Fn(&AccountFor) -> Result<AgentAccount, ApiError> + Send + 'static,
) {
    answering_first(hub, workspace, asked, move |request| {
        let WorkerRequest::AccountFor(call) = request else { return None };
        Some(machine(call).map(WorkerResponse::Account))
    });
}

/// What a worker reads of the account Claude's CLI runs under on a machine where `email` is logged in to it, on `plan`.
pub(super) fn logged_in_as(email: &str, plan: &str) -> AgentAccount {
    AgentAccount {
        id: Some(format!("claude-{email}").into()),
        logged_in: true,
        model_provider: Some("Anthropic API".to_string()),
        plan: Some(plan.to_string()),
        login: Some("Claude account".to_string()),
        email: Some(email.to_string()),
    }
}

/// One of the requests the agent of the chat `session` makes of its model, as its worker passes it on, from the run
/// its stand-in last started for the chat, once the chat has taken it in, as [`the_agent_says`] has the agent speak.
pub(super) async fn the_agent_asks_its_model(
    hub: &Arc<Hub>,
    asked: &Asked,
    session: &SessionId,
    request: CacheRequest,
) {
    let run = last_started(asked, session).expect("only an agent that was started asks its model anything");
    let report = Report::Cache { request };
    hub.route(session, Item::Report { run, report, seq: None, held_to_claims: true }).await;
    flushed(&hub.live_session(session).await.expect("a chat that's there")).await;
}

/// What a stand-in's agent claims as it starts: what Claude claims as the hub's tests describe it, as [`claimed`] says,
/// cancelling a turn and taking a change of its model, mode and effort as it runs, and the prompt cache it marks, as
/// Claude's CLI claims them, so a test can stop a turn, or change a chat's settings, as the person does, or have its
/// agent ask its model.
fn claims() -> protocol::capability::Capabilities {
    let mut claims = claimed();
    let ttls = vec![CacheTtl::FiveMinutes, CacheTtl::OneHour];
    claims.context.cache = Some(context::Cache { kind: context::CacheKind::Marked { ttls } });
    claims.turns.cancel = Some(turns::Cancel { stops: turns::CancelStops::Nothing });
    let (now, next_turn) = (Some(settings::Takes::Now), Some(settings::Takes::NextTurn));
    let live = settings::LiveChanges { model: next_turn, mode: now, effort: next_turn, ..Default::default() };
    claims.settings.live_changes = Some(live);
    claims
}

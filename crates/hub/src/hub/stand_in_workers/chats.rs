use std::sync::Arc;
use std::time::Duration;

use protocol::capability::conversation::Start;
use protocol::capability::settings::Settings;
use protocol::capability::{Control, Report};
use protocol::client::ApiError;
use protocol::event::{AgentRunState, EventBody, TurnOutcome};
use protocol::fixtures::turn_ended;
use protocol::model::{
    AgentAccount, AgentKind, AgentSessionId, AttachmentId, CacheRequest, RunId, Seq, Session, SessionId, Workspace,
    WorkspaceId,
};
use protocol::prompt::Prompt;
use protocol::worker::{AccountFor, AgentSpec, TitleFor, TitleReply, WorkerRequest, WorkerResponse};

use super::super::Hub;
use super::super::tests::flushed;
use super::provider::{Asked, answer};
use crate::session::Item;
use crate::worker_link::{Answer, WorkerLink};

/// A workspace made from the repository `r`, whose worktree its worker's stand-in makes, and a chat of Claude's in it,
/// which nothing has been sent.
pub(in super::super) async fn a_chat(hub: &Arc<Hub>) -> (Workspace, Session) {
    let workspace = hub.workspace_create("r".into(), None, None, None).await.expect("a stand-in makes the worktree");
    let unchanged = Settings::default();
    let chat = hub.session_create(workspace.id.clone(), AgentKind::Claude, unchanged, None).await.expect("a new chat");
    (workspace, chat)
}

impl Asked {
    /// Where each prompt a stand-in was asked to give the agent of `session` is recorded, in the order it was asked.
    pub(in super::super) fn prompts_given(&self, session: &SessionId) -> Vec<Seq> {
        self.requests(|request| {
            let WorkerRequest::AgentControl(asked) = request else { return None };
            let Control::Prompt { seq, .. } = asked.control else { return None };
            (asked.session_id == *session).then_some(seq)
        })
    }

    /// Each launch of the agent of `session` a stand-in was asked for, in the order it was asked.
    pub(in super::super) fn launches(&self, session: &SessionId) -> Vec<AgentSpec> {
        self.requests(|request| {
            let WorkerRequest::AgentStart(spec) = request else { return None };
            (spec.session_id == *session).then(|| spec.clone())
        })
    }
}

/// What the agent of the chat `session` says, as its worker passes it on: each of `said`, as the run its stand-in last
/// started for the chat, once the chat has taken them all in. A stand-in's agent says nothing by itself, so a test says
/// for it what an agent would, such as that it took a prompt, and that its turn has ended.
pub(in super::super) async fn the_agent_says(
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

/// How long a test gives a hub with stand-ins for its workers to do what it does on a task of its own, such as giving
/// the agent a prompt once it's recorded, or learning whose a launch's login is, or to answer what needs a worker,
/// such as a move that stops the agent. Far longer than any of that takes with a stand-in, so only what never comes
/// runs out of it.
pub(in super::super) const TIMEOUT: Duration = Duration::from_secs(10);

/// Sends the chat `session` `prompt`, which its agent takes once the hub has given it, and then ends its turn, as
/// Claude's CLI does. An agent the prompt launched first says it has opened the conversation the hub launched it on, as
/// the CLI does as it starts, and the agent asks its model `asking` before it takes the prompt, if anything. Gives back
/// the launch whose agent took it, which is the one that ran last.
pub(in super::super) async fn a_turn(
    hub: &Arc<Hub>,
    asked: &Asked,
    session: &SessionId,
    prompt: Prompt<AttachmentId>,
    asking: Option<CacheRequest>,
) -> AgentSpec {
    let before = asked.launches(session).len();
    let (seq, _) = hub.session_prompt(session.clone(), None, prompt, None).await.expect("a prompt the hub takes");
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
pub(in super::super) fn answering_first(
    hub: &Hub,
    workspace: &WorkspaceId,
    asked: &Arc<Asked>,
    first: impl Fn(&WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
) {
    answering_first_in_time(hub, workspace, asked, move |request| first(request).map(Answer::Now));
}

/// The same, with `first` saying whether to answer at once or once the test sends the answer, as [`Answer`] says.
pub(in super::super) fn answering_first_in_time(
    hub: &Hub,
    workspace: &WorkspaceId,
    asked: &Arc<Asked>,
    first: impl Fn(&WorkerRequest) -> Option<Answer> + Send + 'static,
) {
    let asked = asked.clone();
    let link = WorkerLink::answering_in_time(move |request| {
        asked.requests.lock().push(request.clone());
        Some(first(&request).unwrap_or_else(|| Answer::Now(Ok(answer(request)))))
    });
    hub.workers.lock().insert(workspace.clone(), link);
}

/// Has the worker of `workspace` be a stand-in that answers a call for a title as the worker does once it has asked its
/// agent's CLI, with what `title` makes of the call: the title the model gave, tidied as the worker tidies it, or why
/// there's none, as [`answering_first`] has it.
pub(in super::super) fn titling(
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
pub(in super::super) fn logged_in(
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
pub(in super::super) fn logged_in_as(email: &str, plan: &str) -> AgentAccount {
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
async fn the_agent_asks_its_model(hub: &Arc<Hub>, asked: &Asked, session: &SessionId, request: CacheRequest) {
    let run = last_started(asked, session).expect("only an agent that was started asks its model anything");
    let report = Report::Cache { request };
    hub.route(session, Item::Report { run, report, seq: None, held_to_claims: true }).await;
    flushed(&hub.live_session(session).await.expect("a chat that's there")).await;
}

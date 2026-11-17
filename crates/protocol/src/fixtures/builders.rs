use serde_json::Value;

use crate::event::{Asking, EventBody, FailureKind, RequestFailure, SessionEvent, TurnOutcome};
use crate::model::{
    Account, AgentKind, ApprovalId, BranchName, ChildStatus, ChildWorktree, Draft, Effort, FoundGithubLogin, GhLogin,
    GhLoginWorks, GithubAccount, GithubApp, GithubAppId, GithubLoginKind, GithubReach, GithubRepositoryName,
    GithubSignIn, GithubSignInStatus, GithubUser, GithubUserId, HeldGithubLogin, MadeBy, ModelName, PermissionMode,
    Place, Reach, Rev, Seq, Session, SessionStatus, SetupStatus, Subagent, SubagentId, SubagentStatus, TitleSource,
    ToolUseId, Workspace, WorkspaceStatus,
};

/// The session `id`, in the workspace `w`, idle with nothing in its log. Its agent is Claude, running Opus with full
/// access at extra-high effort, as a chat that names none of them does. Like a record from a hub that doesn't keep
/// them, it says nothing of a wait, of the approvals and prompts its agent has open, or of whether the agent has opened
/// its conversation.
pub fn session(id: &str) -> Session {
    Session {
        id: id.into(),
        workspace_id: "w".into(),
        agent: AgentKind::Claude,
        agent_capabilities: None,
        model: ModelName::named("opus"),
        permission_mode: PermissionMode::FullAccess,
        effort: Effort::Xhigh,
        status: SessionStatus::Idle,
        last_seq: Seq::default(),
        created_at_ms: 0,
        updated_at_ms: 0,
        title: None,
        closed_at_ms: None,
        reply_seq: Seq::default(),
        reply_outcome: None,
        turn_started_at_ms: None,
        user_active_at_ms: None,
        read_seq: Seq::default(),
        background_tasks: Vec::new(),
        wait: None,
        waits: None,
        context: None,
        prompt_cache: None,
        account: None,
        account_source: None,
        queued: Vec::new(),
        queue_held: false,
        draft: Draft::default(),
        answer_draft: None,
        open_approvals: None,
        untaken: None,
        agent_opened: None,
        rev: Rev::default(),
        title_source: TitleSource::default(),
    }
}

/// The workspace `id`, in the repository `r`, named for its ID and on a branch of that name, and ready, with its setup
/// done and the first ports a hub sets aside. Its worker runs on this machine and hasn't said where the worktree is.
pub fn workspace(id: &str) -> Workspace {
    Workspace {
        id: id.into(),
        repo_id: "r".into(),
        name: id.into(),
        branch: id.into(),
        base_ref: None,
        path: None,
        status: WorkspaceStatus::Ready,
        provider: "local".into(),
        instance_id: id.into(),
        created_at_ms: 0,
        archived_at_ms: None,
        user_active_at_ms: None,
        setup: SetupStatus::Succeeded,
        port_base: 55_000,
        rev: Rev::default(),
        title: None,
        title_source: TitleSource::default(),
        pull_request: None,
        comment_draft: None,
        branches: None,
        pull_request_branch: None,
        worker_build: None,
    }
}

/// The workspace `w`'s other worktree `id`, which the agent of the session `s` made on the branch `w/<id>`, where a
/// hub puts one for a workspace whose own worktree is at `/ws/w`, and which is there, with its branches not read.
pub fn child_worktree(id: &str) -> ChildWorktree {
    ChildWorktree {
        id: id.into(),
        workspace_id: "w".into(),
        path: format!("/ws/.children/w/{id}"),
        branch: BranchName::from(format!("w/{id}")),
        session_id: "s".into(),
        made_by: MadeBy::Agent,
        created_at_ms: 0,
        status: ChildStatus::Ready,
        branches: None,
        rev: Rev::default(),
    }
}

/// The account `id`, as the hub found it on a worker's machine before its agent said anything of it: no model
/// provider, plan, login or email, no limits heard of, and never used. Its login isn't one the hub holds.
pub fn account(id: &str) -> Account {
    Account {
        id: id.into(),
        model_provider: None,
        plan: None,
        login: None,
        email: None,
        name: None,
        login_agent: None,
        limits: Vec::new(),
        held_login: None,
        used_at_ms: None,
        rev: Rev::default(),
    }
}

/// The GitHub account `id` of the user `ada`, whose number is 1, holding a token connected at 0 that doesn't end.
pub fn github_account(id: &str) -> GithubAccount {
    GithubAccount {
        id: id.into(),
        user: GithubUser { id: GithubUserId::new(1), name: "ada".into() },
        held_login: Some(HeldGithubLogin {
            how: GithubLoginKind::Token,
            connected_at_ms: 0,
            ends_at_ms: None,
            needs_sign_in: false,
        }),
        installation: None,
        rev: Rev::default(),
    }
}

/// The app of your own `id`, `ada-roundhouse`, made by ada, whose bot GitHub has numbered.
pub fn github_app(id: u64) -> GithubApp {
    GithubApp {
        id: GithubAppId::new(id),
        slug: "ada-roundhouse".into(),
        name: "Ada's Roundhouse".to_string(),
        owner: GithubUser { id: GithubUserId::new(1), name: "ada".into() },
        bot: Some(GithubUser { id: GithubUserId::new(41), name: "ada-roundhouse[bot]".into() }),
        rev: Rev::default(),
    }
}

/// The sign-in with GitHub `id`, waiting for the person to type `WDJB-MJHT` at GitHub's address for it, until 900 s.
pub fn github_sign_in(id: &str) -> GithubSignIn {
    let code = "WDJB-MJHT".into();
    let address = "https://github.com/login/device".to_string();
    GithubSignIn {
        id: id.into(),
        status: GithubSignInStatus::Waiting { code, address, code_ends_at_ms: 900_000 },
        rev: Rev::default(),
    }
}

/// The login found on `provider`'s machines, whose `gh` is logged in as `ada`, with a token GitHub took.
pub fn found_github_login(provider: &str) -> FoundGithubLogin {
    FoundGithubLogin {
        provider: provider.into(),
        login: GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Yes },
        rev: Rev::default(),
    }
}

/// Whether the default can push to the repository `repo`'s, which is `example/project` on GitHub: it can, as `ada`,
/// as GitHub answered in this run of the hub, at the Unix epoch.
pub fn github_reach(repo: &str) -> GithubReach {
    let repository = GithubRepositoryName::named("example/project");
    GithubReach {
        repo: repo.into(),
        reach: Reach::Pushes { repository, username: "ada".into() },
        answered_at_ms: Some(0),
        stale: false,
        rev: Rev::default(),
    }
}

/// `body` as the event at `seq` in the session `s`'s log, from the agent itself, as a hub that doesn't record runs or
/// places sends it.
pub fn event(seq: Seq, body: EventBody) -> SessionEvent {
    SessionEvent {
        session_id: "s".into(),
        seq,
        ts_ms: 0,
        run: None,
        subagent: None,
        place: Place::default(),
        body,
        raw: None,
    }
}

/// The end of a turn that came out as `outcome`, saying nothing else of it, as an agent that says only that ends one.
/// A test about another of its fields sets that on what this returns, since an event can't take struct update syntax.
pub fn turn_ended(outcome: TurnOutcome) -> EventBody {
    EventBody::TurnEnded {
        outcome,
        result: None,
        errors: Vec::new(),
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
        models: Vec::new(),
        tokens: None,
        only_compacted: false,
        request_failed: false,
    }
}

/// A request to the model that failed on the account's usage limit, saying `message`, as the agent sends it, the
/// agent's own unless `subagent` names the subagent whose it was.
pub fn request_failed(message: &str, subagent: Option<&str>) -> EventBody {
    let failure = RequestFailure { message: message.into(), kind: FailureKind::Limit, resets_at_ms: None };
    EventBody::RequestFailed { subagent: subagent.map(Into::into), failure, again: None }
}

/// The call `call` starting a subagent named by that call, in the background if `in_background`, saying nothing else of
/// it, as an adapter that says only that starts one.
pub fn subagent_started(call: &str, in_background: bool) -> EventBody {
    let subagent = Some(SubagentId::of_call(&call.into()));
    EventBody::SubagentStarted {
        call: call.into(),
        subagent,
        kind: None,
        description: None,
        prompt: None,
        in_background,
    }
}

/// The subagent the call `call` started in the session `s`, named by that call, working in the foreground, with nothing
/// else known of it yet.
pub fn subagent(call: &str) -> Subagent {
    Subagent {
        session_id: "s".into(),
        id: Some(call.into()),
        call: call.into(),
        kind: None,
        description: None,
        prompt: None,
        in_background: false,
        model: None,
        status: SubagentStatus::Working,
        started_at_ms: 0,
        ended_at_ms: None,
        context_tokens: None,
        calls: 0,
        lately: None,
        report: None,
        rev: Rev::default(),
    }
}

/// A request for approval, as [`EventBody::ApprovalRequested`] holds one, for a test to set what it's about and turn
/// into the event's body with `into`.
#[derive(Clone)]
pub struct ApprovalRequest {
    pub approval_id: ApprovalId,
    pub tool_name: String,
    pub tool_use_id: Option<ToolUseId>,
    pub input: Value,
    pub description: Option<String>,
    pub suggestions: Value,
    pub asking: Option<Asking>,
    pub reason: Option<String>,
    pub asked_by: Option<SubagentId>,
}

/// The agent itself asking to use `tool`, under the approval `id`, with no input. Like a request recorded before
/// adapters said, it says nothing of what it asks, of why, or of the call it's for.
pub fn approval_request(id: &str, tool: &str) -> ApprovalRequest {
    ApprovalRequest {
        approval_id: id.into(),
        tool_name: tool.into(),
        tool_use_id: None,
        input: Value::Null,
        description: None,
        suggestions: Value::Null,
        asking: None,
        reason: None,
        asked_by: None,
    }
}

impl From<ApprovalRequest> for EventBody {
    fn from(request: ApprovalRequest) -> Self {
        let ApprovalRequest {
            approval_id,
            tool_name,
            tool_use_id,
            input,
            description,
            suggestions,
            asking,
            reason,
            asked_by,
        } = request;
        EventBody::ApprovalRequested {
            request: None,
            approval_id,
            tool_name,
            tool_use_id,
            input,
            description,
            suggestions: Box::new(suggestions),
            asking: asking.map(Box::new),
            reason,
            asked_by,
        }
    }
}

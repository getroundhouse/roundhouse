use crate::capability::approvals::{AfterDenial, Asks, Decision, Remember, Request};
use crate::capability::asking::{Choose, PlanContext, Question, QuestionOption, TypedAnswer};
use crate::capability::settings::Effort;
use crate::event::{EventBody, FailureKind, RequestFailure, SessionEvent, TurnOutcome};
use crate::model::{
    Account, AgentKind, ApprovalId, BranchName, ChildStatus, ChildWorktree, Draft, FoundGithubLogin, GhLogin,
    GhLoginWorks, GithubAccount, GithubApp, GithubAppId, GithubLoginKind, GithubReach, GithubRepositoryName,
    GithubSignIn, GithubSignInStatus, GithubUser, GithubUserId, HeldGithubLogin, MadeBy, ModelName, Place, Reach, Rev,
    Seq, Session, SessionStatus, SetupStatus, Subagent, SubagentId, SubagentStatus, TitleSource, ToolUseId, Workspace,
    WorkspaceStatus,
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
        mode: super::full_access(),
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
        todos: None,
        account: None,
        account_source: None,
        queued: Vec::new(),
        queue_held: false,
        draft: Draft::default(),
        decision_draft: None,
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
        reserve: None,
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
    pub request: Request,
    pub tool_use_id: Option<ToolUseId>,
    pub asked_by: Option<SubagentId>,
}

/// The agent itself asking what `asks` says, under the approval `id`, suggesting nothing, and with no reason or call
/// named.
pub fn approval_request(id: &str, asks: Asks) -> ApprovalRequest {
    let request = Request { asks, suggested: Vec::new(), always: false, why: None };
    ApprovalRequest { approval_id: id.into(), request, tool_use_id: None, asked_by: None }
}

impl ApprovalRequest {
    /// It, with the agent saying it asks because `why`.
    #[must_use]
    pub fn because(mut self, why: &str) -> Self {
        self.request.why = Some(why.into());
        self
    }
}

impl From<ApprovalRequest> for EventBody {
    fn from(request: ApprovalRequest) -> Self {
        let ApprovalRequest { approval_id, request, tool_use_id, asked_by } = request;
        EventBody::ApprovalRequested { approval_id, request: Box::new(request), tool_use_id, asked_by }
    }
}

/// A question with `text`, offering an option for each of `labels`, of which one can be chosen, or several where
/// `several`, with any other answer typed instead.
pub fn question(text: &str, labels: &[&str], several: bool) -> Question {
    let option = |label: &&str| QuestionOption { label: (*label).into(), description: None, preview: None };
    Question {
        text: text.into(),
        header: None,
        options: labels.iter().map(option).collect(),
        choose: if several { Choose::Several } else { Choose::One },
        typed: TypedAnswer::Yes,
    }
}

/// Asking to run `command`, as a typed call that says nothing more of it.
pub fn to_run(command: &str) -> Asks {
    Asks::Call { call: crate::capability::calls::Call::Command(super::typed_command(command)) }
}

/// Asking `questions`, in order.
pub fn asking(questions: Vec<Question>) -> Asks {
    Asks::Questions { questions }
}

/// Asking for the plan `plan` to be approved, with no file of it.
pub fn plan(plan: &str) -> Asks {
    Asks::Plan { plan: plan.into(), file: None }
}

/// The record that closes the request `id` with what the person decided, `decision`.
pub fn decided(id: &str, decision: Decision) -> EventBody {
    EventBody::ApprovalResolved { approval_id: id.into(), decision: Some(Box::new(decision)), by_setting: false }
}

/// The record that closes the request `id`, which nobody decided, as a stop does.
pub fn cancelled(id: &str) -> EventBody {
    EventBody::ApprovalResolved { approval_id: id.into(), decision: None, by_setting: false }
}

/// Allowing a use once, as it was asked.
pub fn allow_once() -> Decision {
    Decision::Allow { remember: Remember::Once }
}

/// Approving a plan, to be carried out in the conversation it was planned in.
pub fn approve_plan() -> Decision {
    Decision::ApprovePlan { context: PlanContext::Kept }
}

/// Refusing a use, in `words` when the person wrote some, and letting the agent carry on.
pub fn deny(words: Option<&str>) -> Decision {
    Decision::Deny { words: words.map(str::to_string), then: AfterDenial::Continue, keep: None }
}

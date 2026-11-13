//! Made-up records and events, for the tests of every crate. Each is valid from the fewest arguments, and a test sets
//! what it's about with struct update syntax, so a field added to a record changes its builder here rather than every
//! test that makes one. A block is an enum's variant, which that syntax can't set, so a block has a builder for each
//! shape tests make, such as [`of_kind`].

use serde_json::Value;

use crate::event::{Asking, CallKind, ContentBlock, Denied, EventBody, SessionEvent, TurnOutcome};
use crate::model::{
    Account, AgentKind, ApprovalId, BranchName, Call, CallResult, ChildStatus, ChildWorktree, Draft, Effort, MadeBy,
    ModelName, PermissionMode, Place, RecentWork, Rev, Seq, Session, SessionStatus, SetupStatus, Subagent,
    SubagentStatus, TitleSource, ToolUseId, Workspace, WorkspaceStatus,
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
        context: None,
        prompt_cache: None,
        account: None,
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

/// The subagent the call `call` started in the session `s`, working in the foreground, with nothing else known of it
/// yet.
pub fn subagent(call: &str) -> Subagent {
    Subagent {
        session_id: "s".into(),
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
        latest_call: None,
        recent: RecentWork::default(),
        report: None,
        rev: Rev::default(),
    }
}

/// A call to the tool `name`, as the block an agent makes it in, saying no kind, as one recorded before adapters said
/// has none.
pub fn tool_use(id: &str, name: &str, input: Value) -> ContentBlock {
    ContentBlock::ToolUse { id: id.into(), name: name.into(), input, call: None }
}

/// What came back for the call `id`, as the block it comes in: its text, and whether it failed. It has no images,
/// changed no file, isn't encrypted, isn't said to be denied, and says no kind, as one recorded before adapters said
/// has none.
pub fn tool_result(id: &str, text: &str, is_error: bool) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id.into(),
        is_error,
        text: text.into(),
        images: Vec::new(),
        change: None,
        encrypted: false,
        call: None,
        exit_code: None,
        denied: None,
    }
}

/// `block`, a call or a result, saying it's of `kind`, as one recorded since adapters said does.
///
/// # Panics
///
/// If `block` is neither, since only those say a kind, and a test that handed it another would otherwise go on with a
/// block that says none.
pub fn of_kind(mut block: ContentBlock, kind: CallKind) -> ContentBlock {
    match &mut block {
        ContentBlock::ToolUse { call, .. } | ContentBlock::ToolResult { call, .. } => *call = Some(kind),
        ContentBlock::Text { .. }
        | ContentBlock::Thinking { .. }
        | ContentBlock::Image { .. }
        | ContentBlock::Unknown => panic!("only a call or a result says a kind: {block:?}"),
    }
    block
}

/// What came back for the command `id` that exited with `code` having printed `printed`, as recorded since adapters
/// said: a failed result that says it answers a command, with how it exited beside what it printed.
pub fn command_failed(id: &str, printed: &str, code: i32) -> ContentBlock {
    let mut block = of_kind(tool_result(id, printed, true), CallKind::Command);
    if let ContentBlock::ToolResult { exit_code, .. } = &mut block {
        *exit_code = Some(code);
    }
    block
}

/// What came back for the call `id` the person denied, as recorded since adapters said: what the agent was told,
/// `text`, as a failure, and what the person said, if anything.
pub fn tool_denied(id: &str, text: &str, said: Option<&str>) -> ContentBlock {
    let mut block = tool_result(id, text, true);
    if let ContentBlock::ToolResult { denied, .. } = &mut block {
        *denied = Some(Denied { said: said.map(str::to_string) });
    }
    block
}

/// A call to the tool `name`, as a subagent's record has it, saying no kind.
pub fn call(id: &str, name: &str, input: Value) -> Call {
    Call { id: id.into(), name: name.into(), input, kind: None }
}

/// What came back for a call, as a subagent's record has it: all of `text`, from a call that didn't fail.
pub fn call_result(text: &str) -> CallResult {
    CallResult {
        is_error: false,
        text: text.into(),
        cut: false,
        images: Vec::new(),
        encrypted: false,
        exit_code: None,
        denied: None,
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
    pub asked_by: Option<ToolUseId>,
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

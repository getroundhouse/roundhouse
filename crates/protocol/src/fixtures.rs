//! Made-up records and events, for the tests of every crate. Each is valid from the fewest arguments, and a test sets
//! what it's about with struct update syntax, so a field added to a record changes its builder here rather than every
//! test that makes one.

use crate::event::{EventBody, SessionEvent};
use crate::model::{
    AgentKind, Draft, Effort, ModelName, PermissionMode, Place, RecentWork, Rev, Seq, Session, SessionStatus,
    SetupStatus, Subagent, SubagentStatus, TitleSource, Workspace, WorkspaceStatus,
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
        last_seq: 0,
        created_at_ms: 0,
        updated_at_ms: 0,
        title: None,
        closed_at_ms: None,
        reply_seq: 0,
        turn_started_at_ms: None,
        user_active_at_ms: None,
        read_seq: 0,
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
        calls: 0,
        latest_call: None,
        recent: RecentWork::default(),
        report: None,
        rev: Rev::default(),
    }
}

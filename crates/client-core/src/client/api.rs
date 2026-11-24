//! One typed call for each hub request.

use protocol::capability::approvals::Decision;
use protocol::client::{
    HubStatus, admin, agent, comments, conversation, log, proposals, records, references, sessions, waits, workspaces,
};
use protocol::event::SessionEvent;
use protocol::find::Found;
use protocol::model::{
    AgentKind, ApprovalId, AttachmentId, ChangedFileAt, ClientMessageId, ClientRequestId, CommentId, DeviceId,
    DiffComment, DiffSide, Ending, FileDiff, GitDiff, GitStatus, HubSettings, HubSettingsChange, MessageId,
    OwnPullRequest, Proposal, ReferenceAsk, ReferenceRead, Repo, RepoId, Seq, Session, SessionId, SpokenThrough,
    SubagentId, TerminalId, WaitId, Workspace, WorkspaceId,
};

use super::{Client, Result, hub_is_missing};

impl Client {
    pub fn hub_status(&self) -> Result<HubStatus> {
        self.call(admin::HubStatus)
    }

    pub fn hub_shutdown(&self) -> Result<()> {
        match self.call(admin::HubShutdown) {
            // The hub did what was asked before its reply got out.
            Err(e) if hub_is_missing(&e) => Ok(()),
            other => other,
        }
    }

    /// Asks the hub to stop if that cuts nothing off, which it decides. A busy one refuses with `Conflict`.
    pub fn hub_shutdown_if_idle(&self) -> Result<()> {
        match self.call(admin::HubShutdownIfIdle) {
            // The hub did what was asked before its reply got out.
            Err(e) if hub_is_missing(&e) => Ok(()),
            other => other,
        }
    }

    /// Changes the hub's own settings, and returns them as they are now.
    pub fn hub_configure(&self, change: HubSettingsChange) -> Result<HubSettings> {
        self.call(admin::HubConfigure { settings: change })
    }

    pub fn repo_add(&self, path: &str) -> Result<Repo> {
        self.call(records::RepoAdd { path: path.to_string() })
    }

    pub fn repo_list(&self) -> Result<Vec<Repo>> {
        Ok(self.call(records::RepoList)?.repos)
    }

    /// Sending the same `client_request_id` again returns the workspace the first request made.
    pub fn workspace_create(
        &self,
        repo_id: &RepoId,
        name: Option<String>,
        base_ref: Option<String>,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Workspace> {
        self.call(workspaces::WorkspaceCreate { repo_id: repo_id.clone(), name, base_ref, client_request_id })
    }

    pub fn workspace_list(&self, include_archived: bool) -> Result<Vec<Workspace>> {
        Ok(self.call(workspaces::WorkspaceList { include_archived })?.workspaces)
    }

    pub fn workspace_archive(&self, id: &WorkspaceId, force: bool) -> Result<Workspace> {
        self.call(workspaces::WorkspaceArchive { id: id.clone(), force })
    }

    pub fn workspace_restore(&self, id: &WorkspaceId) -> Result<Workspace> {
        self.call(workspaces::WorkspaceRestore { id: id.clone() })
    }

    /// See [`workspaces::WorkspaceRename`].
    pub fn workspace_rename(&self, id: &WorkspaceId, title: &str) -> Result<Workspace> {
        self.call(workspaces::WorkspaceRename { id: id.clone(), title: title.to_string() })
    }

    /// See [`workspaces::WorkspaceRegenerateTitle`]. It waits on a model, usually for a second or two.
    pub fn workspace_regenerate_title(&self, id: &WorkspaceId) -> Result<Workspace> {
        self.call(workspaces::WorkspaceRegenerateTitle { id: id.clone() })
    }

    pub fn workspace_status(&self, id: &WorkspaceId) -> Result<GitStatus> {
        self.call(workspaces::WorkspaceStatus { id: id.clone() })
    }

    pub fn workspace_diff(&self, id: &WorkspaceId) -> Result<GitDiff> {
        self.call(workspaces::WorkspaceDiff { id: id.clone() })
    }

    /// See [`workspaces::WorkspaceFileDiff`]: the page of the diff of the entry `at` names, as a summary names it, from
    /// the patch's line `first_line`, which is 0 or the `next_line` of the page before. `from` is where a rename moved
    /// it from. Nothing of it is kept. A hub whose worker is older than this refuses it, as `is_unsupported` says, and
    /// a worker from before `untracked` answers with the tracked entry at the path. `docs/principles/observations.md`
    /// plans the Changes tab's reads of it.
    pub fn workspace_file_diff(
        &self,
        id: &WorkspaceId,
        at: &ChangedFileAt,
        from: Option<&str>,
        first_line: u32,
    ) -> Result<FileDiff> {
        let ChangedFileAt { untracked, path } = at.clone();
        let from = from.map(str::to_string);
        self.call(workspaces::WorkspaceFileDiff { id: id.clone(), path, untracked, from, first_line })
    }

    pub fn workspace_setup_log(&self, id: &WorkspaceId) -> Result<String> {
        Ok(self.call(workspaces::WorkspaceSetupLog { id: id.clone() })?.text)
    }

    pub fn workspace_rerun_setup(&self, id: &WorkspaceId) -> Result<Workspace> {
        self.call(workspaces::WorkspaceRerunSetup { id: id.clone() })
    }

    pub fn workspace_pull_request(&self, id: &WorkspaceId) -> Result<Option<OwnPullRequest>> {
        Ok(self.call(workspaces::WorkspacePullRequest { id: id.clone() })?.pull_request)
    }

    /// See [`references::ReferencesRead`]. It waits on GitHub for those the hub hasn't read lately.
    pub fn references_read(&self, workspace: &WorkspaceId, asks: Vec<ReferenceAsk>) -> Result<Vec<ReferenceRead>> {
        Ok(self.call(references::ReferencesRead { workspace: workspace.clone(), references: asks })?.references)
    }

    /// Sending the same `client_request_id` again returns the comment the first request made.
    pub fn comment_add(
        &self,
        workspace_id: &WorkspaceId,
        path: &str,
        side: DiffSide,
        line: u32,
        text: &str,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<DiffComment> {
        self.call(comments::CommentAdd {
            workspace_id: workspace_id.clone(),
            path: path.to_string(),
            side,
            line,
            text: text.to_string(),
            client_request_id,
        })
    }

    pub fn comment_list(&self, workspace_id: &WorkspaceId) -> Result<Vec<DiffComment>> {
        Ok(self.call(comments::CommentList { workspace_id: workspace_id.clone() })?.comments)
    }

    /// Replaces a comment's text. Sending it again sets the same text.
    pub fn comment_edit(&self, id: &CommentId, text: &str) -> Result<DiffComment> {
        self.call(comments::CommentEdit { id: id.clone(), text: text.to_string() })
    }

    pub fn comment_delete(&self, id: &CommentId) -> Result<()> {
        self.call(comments::CommentDelete { id: id.clone() })
    }

    pub fn session_list(&self, workspace_id: &WorkspaceId) -> Result<Vec<Session>> {
        Ok(self.call(sessions::SessionList { workspace_id: workspace_id.clone() })?.sessions)
    }

    pub fn session_ensure(&self, workspace_id: &WorkspaceId, agent: AgentKind) -> Result<Session> {
        self.call(sessions::SessionEnsure { workspace_id: workspace_id.clone(), agent })
    }

    pub fn session_get(&self, id: &SessionId) -> Result<Session> {
        self.call(sessions::SessionGet { id: id.clone() })
    }

    /// Which of the session's events hold `query`, as [`protocol::find::Needle::is_in_event`] says.
    pub fn session_find(&self, id: &SessionId, subagent: Option<&SubagentId>, query: &str) -> Result<Found> {
        self.call(log::SessionFind { id: id.clone(), subagent: subagent.cloned(), query: query.to_string() })
    }

    /// Returns the events and the session's newest `seq`.
    pub fn session_read(
        &self,
        id: &SessionId,
        from_seq: Seq,
        limit: u32,
        include_raw: bool,
    ) -> Result<(Vec<SessionEvent>, Seq)> {
        let read = log::SessionRead { id: id.clone(), from_seq, limit, max_bytes: None, include_raw };
        let log::Events { events, last_seq } = self.call(read)?;
        Ok((events, last_seq))
    }

    /// The request waiting in `session` under `approval`, as its record says, read from the log, or `None` where none
    /// waits under it, as for one decided already.
    pub fn waiting_request(&self, session: &Session, approval: &ApprovalId) -> Result<Option<SessionEvent>> {
        let open = session.open_approvals.as_deref().unwrap_or_default();
        let Some(waiting) = open.iter().find(|waiting| waiting.approval_id == *approval) else { return Ok(None) };
        let (events, _) = self.session_read(&session.id, waiting.seq, 1, false)?;
        Ok(events.into_iter().find(|event| event.seq == waiting.seq))
    }

    /// Answers the revision `revision` of the proposal `id`, choosing `chosen`, one option for each decision, with
    /// `note`, and the hub tells its agent. Sending the same `client_request_id` again answers with the proposal as it
    /// was answered.
    pub fn proposal_answer(&self, answer: proposals::ProposalAnswer) -> Result<Proposal> {
        self.call(answer)
    }

    /// Asks for another round of the revision `revision` of the proposal `id`, with `note`, which says what the next
    /// revision should change, and the hub tells its agent. Sending the same `client_request_id` again answers with the
    /// proposal as it was answered.
    pub fn proposal_another_round(&self, round: proposals::ProposalAnotherRound) -> Result<Proposal> {
        self.call(round)
    }

    /// See [`agent::SessionDecide`].
    pub fn session_decide(&self, id: &SessionId, approval_id: &ApprovalId, decision: Decision) -> Result<()> {
        self.call(agent::SessionDecide { id: id.clone(), approval_id: approval_id.clone(), decision })
    }

    pub fn session_close(&self, id: &SessionId) -> Result<Session> {
        self.call(sessions::SessionClose { id: id.clone() })
    }

    /// See [`waits::SessionCallOffWait`].
    pub fn session_call_off_wait(&self, id: &SessionId, wait: &WaitId) -> Result<Session> {
        self.call(waits::SessionCallOffWait { id: id.clone(), wait: wait.clone() })
    }

    pub fn session_reopen(&self, id: &SessionId) -> Result<Session> {
        self.call(sessions::SessionReopen { id: id.clone() })
    }

    /// See [`conversation::SessionConversationStart`].
    pub fn session_conversation_start(&self, id: &SessionId) -> Result<Session> {
        self.call(conversation::SessionConversationStart { id: id.clone() })
    }

    /// See [`conversation::SessionConversationEnd`].
    pub fn session_conversation_end(&self, id: &SessionId) -> Result<Session> {
        self.call(conversation::SessionConversationEnd { id: id.clone() })
    }

    /// See [`conversation::SessionReplyHeard`].
    pub fn session_reply_heard(&self, id: &SessionId, message: MessageId, ending: Ending) -> Result<()> {
        self.call(conversation::SessionReplyHeard { id: id.clone(), message, ending })
    }

    /// See [`conversation::SessionSpeaking`].
    pub fn session_speaking(&self, id: &SessionId, through: SpokenThrough) -> Result<()> {
        self.call(conversation::SessionSpeaking { id: id.clone(), through })
    }

    /// See [`sessions::SessionMarkRead`].
    pub fn session_mark_read(&self, id: &SessionId, through_seq: Seq) -> Result<Session> {
        self.call(sessions::SessionMarkRead { id: id.clone(), through_seq })
    }

    /// See [`sessions::SessionMarkUnread`].
    pub fn session_mark_unread(&self, id: &SessionId) -> Result<Session> {
        self.call(sessions::SessionMarkUnread { id: id.clone() })
    }

    /// See [`sessions::SessionRename`].
    pub fn session_rename(&self, id: &SessionId, title: &str) -> Result<Session> {
        self.call(sessions::SessionRename { id: id.clone(), title: title.to_string() })
    }

    pub fn session_cancel(&self, id: &SessionId) -> Result<()> {
        self.call(agent::SessionCancel { id: id.clone() })
    }

    /// Asks the agent to compact its conversation, keeping what `keeping` says. Refused while it's on a turn.
    pub fn session_compact(&self, id: &SessionId, keeping: Option<&str>) -> Result<()> {
        self.call(agent::SessionCompact { id: id.clone(), keeping: keeping.map(str::to_string) })
    }
}

/// A new ID for a request that creates something. A retry sends the same one, so the hub makes the
/// record once.
pub fn new_request_id() -> ClientRequestId {
    ClientRequestId::from(uuid::Uuid::new_v4().to_string())
}

/// A new ID for a prompt. A retry sends the same one, so the hub records the prompt once.
pub fn new_message_id() -> ClientMessageId {
    ClientMessageId::from(uuid::Uuid::new_v4().to_string())
}

/// A new ID for a file to attach, chosen before it's added, so a view can show it at once and a retry adds it once.
pub fn new_attachment_id() -> AttachmentId {
    AttachmentId::from(uuid::Uuid::new_v4().to_string())
}

/// An ID for a terminal about to be opened, new to every hub.
pub fn new_terminal_id() -> TerminalId {
    TerminalId::from(uuid::Uuid::new_v4().to_string())
}

/// An ID for the device a client runs on, which it makes once and keeps, new to every hub.
pub fn new_device_id() -> DeviceId {
    DeviceId::from(uuid::Uuid::new_v4().to_string())
}

//! One typed call for each hub request.

use std::sync::atomic::Ordering;

use protocol::client::{
    HubStatus, Request, admin, agent, comments, log, records, sessions, subscriptions, waits, workspaces,
};
use protocol::event::{ApprovalDecision, SessionEvent};
use protocol::find::Found;
use protocol::model::{
    AgentKind, ApprovalId, AttachmentId, ClientMessageId, ClientRequestId, CommentId, DiffComment, DiffSide, GitDiff,
    GitStatus, HubSettings, HubSettingsChange, PullRequest, Records, Repo, RepoId, Rev, Seq, Session, SessionId,
    Subagent, TerminalId, ToolUseId, WaitId, Workspace, WorkspaceId,
};

use super::{Client, Result, Subscription, hub_is_missing, subscription_channel};

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

    /// Asks the hub to stop if nothing would notice, which it decides. A busy one refuses with `Conflict`, and one
    /// older than this request refuses it, as `is_unsupported` says.
    pub fn hub_shutdown_if_idle(&self) -> Result<()> {
        match self.call(admin::HubShutdownIfIdle) {
            // The hub did what was asked before its reply got out.
            Err(e) if hub_is_missing(&e) => Ok(()),
            other => other,
        }
    }

    /// Changes the hub's own settings, and returns them as they are now. A hub older than this refuses it, as
    /// `is_unsupported` says.
    pub fn hub_configure(&self, change: HubSettingsChange) -> Result<HubSettings> {
        self.call(admin::HubConfigure { settings: change })
    }

    pub fn repo_add(&self, path: &str) -> Result<Repo> {
        self.call(records::RepoAdd { path: path.to_string() })
    }

    /// Every record as of one revision. Follow the feed first. A hub older than this refuses it, as
    /// `is_unsupported` says.
    pub fn records_list(&self) -> Result<Records> {
        self.call(records::RecordsList)
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

    /// See [`workspaces::WorkspaceRename`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn workspace_rename(&self, id: &WorkspaceId, title: &str) -> Result<Workspace> {
        self.call(workspaces::WorkspaceRename { id: id.clone(), title: title.to_string() })
    }

    /// See [`workspaces::WorkspaceRegenerateTitle`]. It waits on a model, usually for a second or two. A hub older
    /// than this refuses it, as `is_unsupported` says.
    pub fn workspace_regenerate_title(&self, id: &WorkspaceId) -> Result<Workspace> {
        self.call(workspaces::WorkspaceRegenerateTitle { id: id.clone() })
    }

    pub fn workspace_status(&self, id: &WorkspaceId) -> Result<GitStatus> {
        self.call(workspaces::WorkspaceStatus { id: id.clone() })
    }

    pub fn workspace_diff(&self, id: &WorkspaceId) -> Result<GitDiff> {
        self.call(workspaces::WorkspaceDiff { id: id.clone() })
    }

    pub fn workspace_setup_log(&self, id: &WorkspaceId) -> Result<String> {
        Ok(self.call(workspaces::WorkspaceSetupLog { id: id.clone() })?.text)
    }

    pub fn workspace_rerun_setup(&self, id: &WorkspaceId) -> Result<Workspace> {
        self.call(workspaces::WorkspaceRerunSetup { id: id.clone() })
    }

    pub fn workspace_pull_request(&self, id: &WorkspaceId) -> Result<Option<PullRequest>> {
        Ok(self.call(workspaces::WorkspacePullRequest { id: id.clone() })?.pull_request)
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

    /// Which of the session's events hold `query`, as [`protocol::find::Needle::is_in_event`] says. A hub older than
    /// this refuses it as unsupported.
    pub fn session_find(&self, id: &SessionId, subagent: Option<&ToolUseId>, query: &str) -> Result<Found> {
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

    pub fn session_subscribe(&self, id: &SessionId, from_seq: Seq) -> Result<Subscription> {
        self.subscribe(|subscription_id| subscriptions::SessionSubscribe { id: id.clone(), from_seq, subscription_id })
    }

    /// The session's subagents, oldest first, with the revision they're as of, which a [`HubNotice::Subagent`] newer
    /// than it replaces. A hub from before subagents had records refuses it, which
    /// [`super::ClientError::is_unsupported`] says.
    ///
    /// [`HubNotice::Subagent`]: protocol::client::HubNotice::Subagent
    pub fn session_subagents(&self, id: &SessionId) -> Result<(Vec<Subagent>, Rev)> {
        let records::Subagents { subagents, rev } = self.call(records::SessionSubagents { id: id.clone() })?;
        Ok((subagents, rev))
    }

    /// Follows changes to repositories, workspaces and sessions. See [`subscriptions::FeedSubscribe`].
    pub fn feed_subscribe(&self) -> Result<Subscription> {
        self.subscribe(|subscription_id| subscriptions::FeedSubscribe { subscription_id })
    }

    /// Sends the subscription `request` makes for an ID of its own.
    pub(super) fn subscribe<R: Request<Reply = ()>>(&self, request: impl FnOnce(u64) -> R) -> Result<Subscription> {
        // Registered before the request goes out, because items can arrive before the reply.
        let subscription_id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (items_tx, items) = subscription_channel();
        self.inner.subscriptions.lock().insert(subscription_id, items_tx);
        if let Err(e) = self.call(request(subscription_id)) {
            self.inner.subscriptions.lock().remove(&subscription_id);
            return Err(e);
        }
        Ok(Subscription { id: subscription_id, items, connection: self.inner.clone() })
    }

    pub fn session_approve(&self, id: &SessionId, approval_id: &ApprovalId, decision: ApprovalDecision) -> Result<()> {
        self.call(agent::SessionApprove { id: id.clone(), approval_id: approval_id.clone(), decision })
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

    /// See [`sessions::SessionMarkRead`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_mark_read(&self, id: &SessionId, through_seq: Seq) -> Result<Session> {
        self.call(sessions::SessionMarkRead { id: id.clone(), through_seq })
    }

    /// See [`sessions::SessionMarkUnread`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_mark_unread(&self, id: &SessionId) -> Result<Session> {
        self.call(sessions::SessionMarkUnread { id: id.clone() })
    }

    /// See [`sessions::SessionRename`]. A hub older than this refuses it, as `is_unsupported` says.
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

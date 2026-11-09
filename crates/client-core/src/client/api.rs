//! One typed call for each hub request.

use std::sync::atomic::Ordering;

use protocol::client::{ClientRequest, ClientResponse, HubStatus};
use protocol::event::{ApprovalDecision, SessionEvent};
use protocol::model::{
    AgentKind, ApprovalId, ClientMessageId, ClientRequestId, CommentId, DiffComment, DiffSide, GitDiff, GitStatus,
    PullRequest, Records, Repo, RepoId, Seq, Session, SessionId, Workspace, WorkspaceId,
};

use super::{Client, Result, Subscription, hub_is_missing, subscription_channel, unexpected};

impl Client {
    pub fn hub_status(&self) -> Result<HubStatus> {
        let reply = self.call(ClientRequest::HubStatus)?;
        let ClientResponse::HubStatus(status) = reply else {
            return unexpected(&reply);
        };
        Ok(status)
    }

    pub fn hub_shutdown(&self) -> Result<()> {
        match self.expect_ok(ClientRequest::HubShutdown) {
            // The hub did what was asked before its reply got out.
            Err(e) if hub_is_missing(&e) => Ok(()),
            other => other,
        }
    }

    pub fn repo_add(&self, path: &str) -> Result<Repo> {
        let reply = self.call(ClientRequest::RepoAdd { path: path.to_string() })?;
        let ClientResponse::Repo(repo) = reply else {
            return unexpected(&reply);
        };
        Ok(repo)
    }

    /// Every record as of one revision. Follow the feed first. A hub older than this refuses it, as
    /// `is_unsupported` says.
    pub fn records_list(&self) -> Result<Records> {
        let reply = self.call(ClientRequest::RecordsList)?;
        let ClientResponse::Records(records) = reply else {
            return unexpected(&reply);
        };
        Ok(records)
    }

    pub fn repo_list(&self) -> Result<Vec<Repo>> {
        let reply = self.call(ClientRequest::RepoList)?;
        let ClientResponse::Repos { repos } = reply else {
            return unexpected(&reply);
        };
        Ok(repos)
    }

    /// Sending the same `client_request_id` again returns the workspace the first request made.
    pub fn workspace_create(
        &self,
        repo_id: &RepoId,
        name: Option<String>,
        base_ref: Option<String>,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceCreate {
            repo_id: repo_id.clone(),
            name,
            base_ref,
            client_request_id,
        })
    }

    pub fn workspace_list(&self, include_archived: bool) -> Result<Vec<Workspace>> {
        let reply = self.call(ClientRequest::WorkspaceList { include_archived })?;
        let ClientResponse::Workspaces { workspaces } = reply else {
            return unexpected(&reply);
        };
        Ok(workspaces)
    }

    pub fn workspace_archive(&self, id: &WorkspaceId, force: bool) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceArchive { id: id.clone(), force })
    }

    pub fn workspace_restore(&self, id: &WorkspaceId) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceRestore { id: id.clone() })
    }

    pub fn workspace_status(&self, id: &WorkspaceId) -> Result<GitStatus> {
        let reply = self.call(ClientRequest::WorkspaceStatus { id: id.clone() })?;
        let ClientResponse::GitStatus(status) = reply else {
            return unexpected(&reply);
        };
        Ok(status)
    }

    pub fn workspace_diff(&self, id: &WorkspaceId) -> Result<GitDiff> {
        let reply = self.call(ClientRequest::WorkspaceDiff { id: id.clone() })?;
        let ClientResponse::GitDiff(diff) = reply else {
            return unexpected(&reply);
        };
        Ok(diff)
    }

    pub fn workspace_setup_log(&self, id: &WorkspaceId) -> Result<String> {
        let reply = self.call(ClientRequest::WorkspaceSetupLog { id: id.clone() })?;
        let ClientResponse::SetupLog { text } = reply else {
            return unexpected(&reply);
        };
        Ok(text)
    }

    pub fn workspace_rerun_setup(&self, id: &WorkspaceId) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceRerunSetup { id: id.clone() })
    }

    pub fn workspace_pull_request(&self, id: &WorkspaceId) -> Result<Option<PullRequest>> {
        let reply = self.call(ClientRequest::WorkspacePullRequest { id: id.clone() })?;
        let ClientResponse::PullRequest { pull_request } = reply else {
            return unexpected(&reply);
        };
        Ok(pull_request)
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
        let request = ClientRequest::CommentAdd {
            workspace_id: workspace_id.clone(),
            path: path.to_string(),
            side,
            line,
            text: text.to_string(),
            client_request_id,
        };
        let reply = self.call(request)?;
        let ClientResponse::Comment(comment) = reply else {
            return unexpected(&reply);
        };
        Ok(comment)
    }

    pub fn comment_list(&self, workspace_id: &WorkspaceId) -> Result<Vec<DiffComment>> {
        let reply = self.call(ClientRequest::CommentList { workspace_id: workspace_id.clone() })?;
        let ClientResponse::Comments { comments } = reply else {
            return unexpected(&reply);
        };
        Ok(comments)
    }

    pub fn comment_delete(&self, id: &CommentId) -> Result<()> {
        self.expect_ok(ClientRequest::CommentDelete { id: id.clone() })
    }

    /// Sending the same `client_request_id` again returns the session the first request made.
    pub fn session_create(
        &self,
        workspace_id: &WorkspaceId,
        agent: AgentKind,
        model: Option<String>,
        permission_mode: Option<String>,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Session> {
        let request = ClientRequest::SessionCreate {
            workspace_id: workspace_id.clone(),
            agent,
            model,
            permission_mode,
            client_request_id,
        };
        self.expect_session(request)
    }

    pub fn session_list(&self, workspace_id: &WorkspaceId) -> Result<Vec<Session>> {
        let reply = self.call(ClientRequest::SessionList { workspace_id: workspace_id.clone() })?;
        let ClientResponse::Sessions { sessions } = reply else {
            return unexpected(&reply);
        };
        Ok(sessions)
    }

    /// Makes a workspace with the hub's own choice of name and branch, and joins or starts its chat with Claude,
    /// leaving the model and approval mode to the user's own Claude settings.
    pub fn workspace_start(&self, repo_id: &RepoId) -> Result<(Workspace, Session)> {
        let workspace = self.workspace_create(repo_id, None, None, Some(crate::new_request_id()))?;
        let session = self.session_ensure(&workspace.id, AgentKind::Claude)?;
        Ok((workspace, session))
    }

    pub fn session_ensure(&self, workspace_id: &WorkspaceId, agent: AgentKind) -> Result<Session> {
        self.expect_session(ClientRequest::SessionEnsure { workspace_id: workspace_id.clone(), agent })
    }

    pub fn session_get(&self, id: &SessionId) -> Result<Session> {
        self.expect_session(ClientRequest::SessionGet { id: id.clone() })
    }

    /// Returns the events and the session's newest `seq`.
    pub fn session_read(
        &self,
        id: &SessionId,
        from_seq: Seq,
        limit: u32,
        include_raw: bool,
    ) -> Result<(Vec<SessionEvent>, Seq)> {
        let request = ClientRequest::SessionRead { id: id.clone(), from_seq, limit, max_bytes: None, include_raw };
        let reply = self.call(request)?;
        let ClientResponse::Events { events, last_seq } = reply else {
            return unexpected(&reply);
        };
        Ok((events, last_seq))
    }

    pub fn session_subscribe(&self, id: &SessionId, from_seq: Seq) -> Result<Subscription> {
        // Registered before the request goes out, because events can arrive before the reply.
        let subscription_id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (items_tx, items) = subscription_channel();
        self.inner.subscriptions.lock().insert(subscription_id, items_tx);
        let reply = self.expect_ok(ClientRequest::SessionSubscribe { id: id.clone(), from_seq, subscription_id });
        if let Err(e) = reply {
            self.inner.subscriptions.lock().remove(&subscription_id);
            return Err(e);
        }
        Ok(Subscription { id: subscription_id, items, connection: self.inner.clone() })
    }

    /// Follows changes to repositories, workspaces and sessions. See [`ClientRequest::FeedSubscribe`].
    pub fn feed_subscribe(&self) -> Result<Subscription> {
        let subscription_id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (items_tx, items) = subscription_channel();
        self.inner.subscriptions.lock().insert(subscription_id, items_tx);
        if let Err(e) = self.expect_ok(ClientRequest::FeedSubscribe { subscription_id }) {
            self.inner.subscriptions.lock().remove(&subscription_id);
            return Err(e);
        }
        Ok(Subscription { id: subscription_id, items, connection: self.inner.clone() })
    }

    /// Returns the prompt's `seq`, and whether it had already been recorded.
    pub fn session_prompt(
        &self,
        id: &SessionId,
        text: &str,
        client_msg_id: Option<ClientMessageId>,
    ) -> Result<(Seq, bool)> {
        let request = ClientRequest::SessionPrompt { id: id.clone(), text: text.to_string(), client_msg_id };
        let reply = self.call(request)?;
        let ClientResponse::PromptRecorded { seq, duplicate } = reply else {
            return unexpected(&reply);
        };
        Ok((seq, duplicate))
    }

    pub fn session_approve(&self, id: &SessionId, approval_id: &ApprovalId, decision: ApprovalDecision) -> Result<()> {
        self.expect_ok(ClientRequest::SessionApprove { id: id.clone(), approval_id: approval_id.clone(), decision })
    }

    pub fn session_configure(
        &self,
        id: &SessionId,
        model: Option<String>,
        permission_mode: Option<String>,
    ) -> Result<Session> {
        self.expect_session(ClientRequest::SessionConfigure { id: id.clone(), model, permission_mode })
    }

    pub fn session_close(&self, id: &SessionId) -> Result<Session> {
        self.expect_session(ClientRequest::SessionClose { id: id.clone() })
    }

    pub fn session_reopen(&self, id: &SessionId) -> Result<Session> {
        self.expect_session(ClientRequest::SessionReopen { id: id.clone() })
    }

    pub fn session_cancel(&self, id: &SessionId) -> Result<()> {
        self.expect_ok(ClientRequest::SessionCancel { id: id.clone() })
    }

    fn expect_ok(&self, request: ClientRequest) -> Result<()> {
        let reply = self.call(request)?;
        let ClientResponse::Ok = reply else {
            return unexpected(&reply);
        };
        Ok(())
    }

    fn expect_workspace(&self, request: ClientRequest) -> Result<Workspace> {
        let reply = self.call(request)?;
        let ClientResponse::Workspace(workspace) = reply else {
            return unexpected(&reply);
        };
        Ok(workspace)
    }

    fn expect_session(&self, request: ClientRequest) -> Result<Session> {
        let reply = self.call(request)?;
        let ClientResponse::Session(session) = reply else {
            return unexpected(&reply);
        };
        Ok(session)
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

//! One typed call for each hub request.

use std::sync::atomic::Ordering;

use protocol::client::{ClientRequest, ClientResponse, HubStatus};
use protocol::event::{ApprovalDecision, SessionEvent};
use protocol::model::{
    AgentKind, DiffComment, DiffSide, GitDiff, GitStatus, PullRequest, Repo, Seq, Session, Workspace,
};

use super::{Client, Result, Subscription, hub_is_missing, unexpected};

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

    pub fn repo_list(&self) -> Result<Vec<Repo>> {
        let reply = self.call(ClientRequest::RepoList)?;
        let ClientResponse::Repos { repos } = reply else {
            return unexpected(&reply);
        };
        Ok(repos)
    }

    pub fn workspace_create(&self, repo_id: &str, name: Option<String>, base_ref: Option<String>) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceCreate { repo_id: repo_id.to_string(), name, base_ref })
    }

    pub fn workspace_list(&self, include_archived: bool) -> Result<Vec<Workspace>> {
        let reply = self.call(ClientRequest::WorkspaceList { include_archived })?;
        let ClientResponse::Workspaces { workspaces } = reply else {
            return unexpected(&reply);
        };
        Ok(workspaces)
    }

    pub fn workspace_archive(&self, id: &str, force: bool) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceArchive { id: id.to_string(), force })
    }

    pub fn workspace_restore(&self, id: &str) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceRestore { id: id.to_string() })
    }

    pub fn workspace_status(&self, id: &str) -> Result<GitStatus> {
        let reply = self.call(ClientRequest::WorkspaceStatus { id: id.to_string() })?;
        let ClientResponse::GitStatus(status) = reply else {
            return unexpected(&reply);
        };
        Ok(status)
    }

    pub fn workspace_diff(&self, id: &str) -> Result<GitDiff> {
        let reply = self.call(ClientRequest::WorkspaceDiff { id: id.to_string() })?;
        let ClientResponse::GitDiff(diff) = reply else {
            return unexpected(&reply);
        };
        Ok(diff)
    }

    pub fn workspace_setup_log(&self, id: &str) -> Result<String> {
        let reply = self.call(ClientRequest::WorkspaceSetupLog { id: id.to_string() })?;
        let ClientResponse::SetupLog { text } = reply else {
            return unexpected(&reply);
        };
        Ok(text)
    }

    pub fn workspace_rerun_setup(&self, id: &str) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceRerunSetup { id: id.to_string() })
    }

    pub fn workspace_pull_request(&self, id: &str) -> Result<Option<PullRequest>> {
        let reply = self.call(ClientRequest::WorkspacePullRequest { id: id.to_string() })?;
        let ClientResponse::PullRequest { pull_request } = reply else {
            return unexpected(&reply);
        };
        Ok(pull_request)
    }

    pub fn comment_add(
        &self,
        workspace_id: &str,
        path: &str,
        side: DiffSide,
        line: u32,
        text: &str,
    ) -> Result<DiffComment> {
        let request = ClientRequest::CommentAdd {
            workspace_id: workspace_id.to_string(),
            path: path.to_string(),
            side,
            line,
            text: text.to_string(),
        };
        let reply = self.call(request)?;
        let ClientResponse::Comment(comment) = reply else {
            return unexpected(&reply);
        };
        Ok(comment)
    }

    pub fn comment_list(&self, workspace_id: &str) -> Result<Vec<DiffComment>> {
        let reply = self.call(ClientRequest::CommentList { workspace_id: workspace_id.to_string() })?;
        let ClientResponse::Comments { comments } = reply else {
            return unexpected(&reply);
        };
        Ok(comments)
    }

    pub fn comment_delete(&self, id: &str) -> Result<()> {
        self.expect_ok(ClientRequest::CommentDelete { id: id.to_string() })
    }

    pub fn session_create(
        &self,
        workspace_id: &str,
        agent: AgentKind,
        model: Option<String>,
        permission_mode: Option<String>,
    ) -> Result<Session> {
        let request =
            ClientRequest::SessionCreate { workspace_id: workspace_id.to_string(), agent, model, permission_mode };
        self.expect_session(request)
    }

    pub fn session_list(&self, workspace_id: &str) -> Result<Vec<Session>> {
        let reply = self.call(ClientRequest::SessionList { workspace_id: workspace_id.to_string() })?;
        let ClientResponse::Sessions { sessions } = reply else {
            return unexpected(&reply);
        };
        Ok(sessions)
    }

    pub fn session_ensure(&self, workspace_id: &str, agent: AgentKind) -> Result<Session> {
        self.expect_session(ClientRequest::SessionEnsure { workspace_id: workspace_id.to_string(), agent })
    }

    pub fn session_get(&self, id: &str) -> Result<Session> {
        self.expect_session(ClientRequest::SessionGet { id: id.to_string() })
    }

    /// Returns the events and the session's newest `seq`.
    pub fn session_read(
        &self,
        id: &str,
        from_seq: Seq,
        limit: u32,
        include_raw: bool,
    ) -> Result<(Vec<SessionEvent>, Seq)> {
        let request = ClientRequest::SessionRead { id: id.to_string(), from_seq, limit, max_bytes: None, include_raw };
        let reply = self.call(request)?;
        let ClientResponse::Events { events, last_seq } = reply else {
            return unexpected(&reply);
        };
        Ok((events, last_seq))
    }

    pub fn session_subscribe(&self, id: &str, from_seq: Seq) -> Result<Subscription> {
        // Registered before the request goes out, because events can arrive before the reply.
        let subscription_id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (items_tx, items) = flume::unbounded();
        self.inner.subscriptions.lock().insert(subscription_id, items_tx);
        let reply = self.expect_ok(ClientRequest::SessionSubscribe { id: id.to_string(), from_seq, subscription_id });
        if let Err(e) = reply {
            self.inner.subscriptions.lock().remove(&subscription_id);
            return Err(e);
        }
        Ok(Subscription { id: subscription_id, items, connection: self.inner.clone() })
    }

    /// Follows changes to repositories, workspaces and sessions. See [`ClientRequest::FeedSubscribe`].
    pub fn feed_subscribe(&self) -> Result<Subscription> {
        let subscription_id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (items_tx, items) = flume::unbounded();
        self.inner.subscriptions.lock().insert(subscription_id, items_tx);
        if let Err(e) = self.expect_ok(ClientRequest::FeedSubscribe { subscription_id }) {
            self.inner.subscriptions.lock().remove(&subscription_id);
            return Err(e);
        }
        Ok(Subscription { id: subscription_id, items, connection: self.inner.clone() })
    }

    /// Returns the prompt's `seq`, and whether it had already been recorded.
    pub fn session_prompt(&self, id: &str, text: &str, client_msg_id: Option<String>) -> Result<(Seq, bool)> {
        let request = ClientRequest::SessionPrompt { id: id.to_string(), text: text.to_string(), client_msg_id };
        let reply = self.call(request)?;
        let ClientResponse::PromptRecorded { seq, duplicate } = reply else {
            return unexpected(&reply);
        };
        Ok((seq, duplicate))
    }

    pub fn session_approve(&self, id: &str, approval_id: &str, decision: ApprovalDecision) -> Result<()> {
        self.expect_ok(ClientRequest::SessionApprove {
            id: id.to_string(),
            approval_id: approval_id.to_string(),
            decision,
        })
    }

    pub fn session_configure(
        &self,
        id: &str,
        model: Option<String>,
        permission_mode: Option<String>,
    ) -> Result<Session> {
        self.expect_session(ClientRequest::SessionConfigure { id: id.to_string(), model, permission_mode })
    }

    pub fn session_close(&self, id: &str) -> Result<Session> {
        self.expect_session(ClientRequest::SessionClose { id: id.to_string() })
    }

    pub fn session_reopen(&self, id: &str) -> Result<Session> {
        self.expect_session(ClientRequest::SessionReopen { id: id.to_string() })
    }

    pub fn session_cancel(&self, id: &str) -> Result<()> {
        self.expect_ok(ClientRequest::SessionCancel { id: id.to_string() })
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

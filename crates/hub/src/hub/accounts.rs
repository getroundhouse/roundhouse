//! Whose account a session's agent runs under, and what agents have heard of each account's limits.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::model::{
    AccountId, AgentAccount, AgentKind, SessionAccount, SessionId, UsageLimit, WorkspaceId, WorkspaceStatus,
};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tracing::error;

use super::{Hub, not_ready, unexpected};

impl Hub {
    /// The account an agent of this kind runs under in the workspace, as the workspace's worker reads it, since the
    /// login is on the worker's machine. It's the one way the hub learns which account that is, so what an agent
    /// hears of its limits is kept under the same account they're then read by.
    ///
    /// Only for a workspace that's ready, as every read from a worker is, so asking never starts a worker for one
    /// that's archived or still being made.
    pub(crate) async fn account_of(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
        agent: AgentKind,
    ) -> Result<AgentAccount, ApiError> {
        let workspace = self.workspace(workspace_id).await?;
        if workspace.status != WorkspaceStatus::Ready {
            return Err(not_ready(&workspace));
        }
        let link = self.ensure_worker(workspace_id).await?;
        let reply = link.call(WorkerRequest::AccountFor { agent }).await?;
        let WorkerResponse::Account(account) = reply else { return Err(unexpected(&reply)) };
        Ok(account)
    }

    /// The account the session's agent runs under, and what's been heard of that account's limits, by any agent
    /// under it.
    pub(crate) async fn session_account(self: &Arc<Self>, id: SessionId) -> Result<SessionAccount, ApiError> {
        let session = self.session_get(id).await?;
        let account = self.account_of(&session.workspace_id, session.agent).await?;
        let limits = match account.id.clone() {
            Some(id) => self.blocking(move |store| store.account_limits(&id)).await?,
            // A login that can't be told from another has no limits kept for it.
            None => Vec::new(),
        };
        Ok(SessionAccount { account, limits })
    }

    /// Keeps what an agent under `account` heard of its limits at `heard_at_ms`.
    pub(crate) async fn hear_limits(&self, account: AccountId, limits: Vec<UsageLimit>, heard_at_ms: i64) {
        let kept = account.clone();
        if let Err(e) = self.blocking(move |store| store.hear_limits(&kept, &limits, heard_at_ms)).await {
            error!(%account, "could not keep what an agent heard of its account's limits: {e}");
        }
    }
}

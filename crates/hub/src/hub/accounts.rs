//! Whose account a session's agent runs under, and what agents have heard of each account's limits.

use std::sync::Arc;

use protocol::client::{ApiError, HubNotice};
use protocol::model::{
    Account, AccountId, AgentAccount, AgentKind, SessionAccount, SessionId, UsageLimit, WorkspaceId,
};
use protocol::worker::AccountFor;
use tracing::error;

use super::Hub;

impl Hub {
    /// The account an agent of this kind runs under in the workspace, as the workspace's worker reads it, when the
    /// login is on the worker's machine. It's how the hub learns which account that is for a launch it gave no login,
    /// so what an agent hears of its limits is kept under the same account they're then read by.
    ///
    /// Only for a workspace that's ready, as every read from a worker is, so asking never starts a worker for one
    /// that's archived or still being made.
    pub(crate) async fn account_of(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
        agent: AgentKind,
    ) -> Result<AgentAccount, ApiError> {
        let worktree = self.ready_worktree(workspace_id).await?;
        worktree.link().call(AccountFor { agent, login: None }).await
    }

    /// The account the session's agent runs under, and what's been heard of that account's limits, by any agent
    /// under it. For clients from before accounts were records, which ask for it. An account whose login the hub gave
    /// the agent is answered from the hub's own record, since the worker isn't told whose a token is.
    pub(crate) async fn session_account(self: &Arc<Self>, id: SessionId) -> Result<SessionAccount, ApiError> {
        let session = self.session_get(id).await?;
        if let Some(held) = self.held_login_account(session.account).await? {
            return Ok(held);
        }
        let account = self.account_of(&session.workspace_id, session.agent).await?;
        let limits = match account.logged_in_as().cloned() {
            Some(id) => self.blocking(move |store| store.account_limits(&id)).await?,
            // A login that can't be told from another has no limits kept for it.
            None => Vec::new(),
        };
        Ok(SessionAccount { account, limits })
    }

    /// The account `id` as a worker would say it, when it's one the hub chose the ID of for a login it was given,
    /// with what's been heard of its limits.
    async fn held_login_account(&self, id: Option<AccountId>) -> Result<Option<SessionAccount>, ApiError> {
        let Some(id) = id else { return Ok(None) };
        let Some(record) = self.blocking(move |store| store.account(&id)).await? else { return Ok(None) };
        if record.login_agent.is_none() {
            return Ok(None);
        }
        let Account { id, model_provider, plan, login, email, limits, .. } = record;
        let account = AgentAccount { id: Some(id), logged_in: true, model_provider, plan, login, email };
        Ok(Some(SessionAccount { account, limits }))
    }

    /// Keeps what an agent under `account` heard of its limits at `heard_at_ms`, which holds every model's own week the
    /// account has when `every_models_week` says so, and tells every client when it changed the account's record,
    /// since every session under the account shows it.
    pub(crate) async fn hear_limits(
        &self,
        account: AccountId,
        limits: Vec<UsageLimit>,
        every_models_week: bool,
        heard_at_ms: i64,
    ) {
        let kept = account.clone();
        match self.blocking(move |store| store.hear_limits(&kept, &limits, every_models_week, heard_at_ms)).await {
            Ok(Some(record)) => self.notify(HubNotice::Account(record)),
            Ok(None) => {}
            Err(e) => error!(%account, "could not keep what an agent heard of its account's limits: {e}"),
        }
    }

    /// Files that `session`'s agent launched under `under`, as its worker read it, and tells every client what that
    /// changed of the account's record. Whether the session's record changed is returned, for the session to announce
    /// itself, since only it knows how.
    pub(crate) async fn launched_under(&self, session: &SessionId, under: AgentAccount) -> Result<bool, ApiError> {
        let id = session.clone();
        let launched = self.blocking(move |store| store.launched_under(&id, &under)).await?;
        if let Some(record) = launched.account {
            self.notify(HubNotice::Account(record));
        }
        Ok(launched.session)
    }
}

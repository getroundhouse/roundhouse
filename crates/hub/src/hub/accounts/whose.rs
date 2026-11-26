//! Whose account a session's agent runs under, and what agents have heard of each account's limits.

use std::sync::Arc;

use protocol::client::{ApiError, HubNotice};
use protocol::model::{AccountId, AgentAccount, AgentKind, SessionId, UsageLimit, WorkspaceId};
use protocol::worker::AccountFor;
use tracing::error;

use super::limits_heard::merged;
use crate::hub::Hub;
use crate::hub::observations::{AccountIdentity, AccountLimits};
use crate::store::Launched;

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

    /// Keeps what an agent under `account` heard of its limits at `heard_at_ms`, which holds every model's own week the
    /// account has when `every_models_week` says so, merged with what's kept, as [`merged`] says, as an observation,
    /// and tells every client, since every session under the account shows it. An account nobody has launched an agent
    /// under since the hub's database was made gets a record, which owns what's heard of it.
    pub(crate) async fn hear_limits(
        &self,
        account: AccountId,
        limits: Vec<UsageLimit>,
        every_models_week: bool,
        heard_at_ms: i64,
    ) {
        let kept = account.clone();
        match self.blocking(move |store| store.account_made(&kept)).await {
            Ok(Some(made)) => self.notify(HubNotice::Account(made)),
            Ok(None) => {}
            Err(e) => {
                return error!(
                    name: "account.limits_unkept",
                    %account,
                    error = %e,
                    "Couldn't keep what an agent heard of its account's limits"
                );
            }
        }
        let merge =
            |held: Option<&Vec<UsageLimit>>| merged(held.map(Vec::as_slice), &limits, every_models_week, heard_at_ms);
        self.observation_updated::<AccountLimits>(account, heard_at_ms, merge).await;
    }

    /// Files that `session`'s agent launched under `under`, as its worker read it at `read_at_ms`, and tells every
    /// client what that changed of the account's record, and then of who the account is, which is an observation.
    /// What it changed of the session's record is returned, for the session to follow and announce itself, since only
    /// it knows how.
    pub(crate) async fn launched_under(
        &self,
        session: &SessionId,
        under: AgentAccount,
        read_at_ms: i64,
    ) -> Result<Launched, ApiError> {
        let id = session.clone();
        let filed = under.clone();
        let mut launched = self.blocking(move |store| store.launched_under(&id, &filed)).await?;
        if let Some(record) = launched.account.take() {
            self.notify(HubNotice::Account(record));
        }
        if let Some(account) = under.logged_in_as().cloned() {
            let AgentAccount { model_provider, plan, login, email, .. } = under;
            let identity = protocol::model::AccountIdentity { model_provider, plan, login, email };
            self.observation_read::<AccountIdentity>(account, identity, read_at_ms).await.result()?;
        }
        Ok(launched)
    }
}

//! Selection is durable hub state, never inferred from the worker's machine or renewal order.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AccountId, AgentKind, AgentLogin, HubSettings};

use super::{Handed, Logins};
use crate::error::internal;
use crate::store::Held;

impl Logins {
    pub(crate) fn set_default(
        &self,
        agent: AgentKind,
        account: &AccountId,
        now_ms: i64,
    ) -> Result<HubSettings, ApiError> {
        let _one_at_a_time = self.one_at_a_time.lock();
        self.renewable(account, agent)?;
        self.available(agent, account, now_ms)?;
        Ok(self.store.set_account_default(agent, account)?)
    }

    /// A broken selection fails closed. Only a kind with no selection can use machine credentials.
    pub(crate) fn to_hand(&self, agent: AgentKind, now_ms: i64) -> Result<Option<Handed>, ApiError> {
        let _one_at_a_time = self.one_at_a_time.lock();
        let settings = self.store.hub_settings()?;
        let selected = settings.account_defaults.unwrap_or_default().into_iter().find(|choice| choice.agent == agent);
        let Some(selected) = selected else { return Ok(None) };
        self.available(agent, &selected.account, now_ms).map(Some)
    }

    fn available(&self, agent: AgentKind, account: &AccountId, now_ms: i64) -> Result<Handed, ApiError> {
        let record = self.record(account)?;
        let called = record.name.as_deref().unwrap_or(account.as_str());
        let unavailable = || {
            ApiError::new(
                ErrorCode::Conflict,
                format!(
                    "the default account {called} needs a login; renew or reconnect it in Accounts, or choose another default"
                ),
            )
        };
        let held = self.store.held_logins(agent)?.into_iter().find(|held| held.account == *account);
        let Some(Held { file, login, .. }) = held else { return Err(unavailable()) };
        if login.ends_at_ms.is_some_and(|ends| ends <= now_ms) {
            return Err(unavailable());
        }
        let token = self
            .files
            .read(&file)
            .map_err(|e| internal(format!("couldn't read {called}'s login: {e}")))?
            .ok_or_else(unavailable)?;
        Ok(Handed { account: account.clone(), login: AgentLogin { token } })
    }
}

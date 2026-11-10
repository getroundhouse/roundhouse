//! Filing what the session's agent hears of its account's limits under that account. The line an agent hears them
//! in doesn't name the account, so the session finds out which it is, once for each launch of the agent.

use std::sync::Arc;

use protocol::model::{AccountId, RunId, UsageLimit};
use tracing::debug;

use super::LiveSession;

/// Which account the agent going runs under, as far as the session knows.
pub(super) enum Hearing {
    /// The worker is being asked. What the agent hears meanwhile waits here, only the newest, with when it was
    /// heard.
    Asking(Option<(Vec<UsageLimit>, i64)>),
    /// `None` for a login that can't be told from another, whose limits aren't kept.
    Known(Option<AccountId>),
}

impl LiveSession {
    /// The agent of `run` heard of its account's limits, at `heard_at_ms`. Only the agent going speaks for the
    /// session. The first time one does, the worker is asked which account it runs under, off this task, since the
    /// answer takes a call to the agent's CLI and the agent's events mustn't wait on it.
    pub(super) fn heard_limits(self: &Arc<Self>, run: RunId, limits: Vec<UsageLimit>, heard_at_ms: i64) {
        let account = {
            let mut state = self.state.lock();
            if state.agent.is_none_or(|agent| agent.run != run) {
                return;
            }
            match &mut state.hearing {
                Some((of, Hearing::Known(account))) if *of == run => account.clone(),
                Some((of, Hearing::Asking(waiting))) if *of == run => {
                    *waiting = Some((limits, heard_at_ms));
                    return;
                }
                // Nothing is known of this launch's account, whatever was of the last one's.
                Some(_) | None => {
                    state.hearing = Some((run, Hearing::Asking(Some((limits, heard_at_ms)))));
                    tokio::spawn(self.clone().learn_account(run));
                    return;
                }
            }
        };
        let (Some(account), Ok(hub)) = (account, self.hub()) else { return };
        tokio::spawn(async move { hub.hear_limits(account, limits, heard_at_ms).await });
    }

    /// Asks which account the agent of `run` runs under, and files what it heard meanwhile. If the worker can't
    /// say, nothing is known, and it's asked again when the agent next hears of a limit.
    async fn learn_account(self: Arc<Self>, run: RunId) {
        let Ok(hub) = self.hub() else { return };
        let account = hub.account_of(&self.workspace_id, self.agent).await;
        let (account, waiting) = {
            let mut state = self.state.lock();
            let waiting = match &mut state.hearing {
                Some((of, Hearing::Asking(waiting))) if *of == run => waiting.take(),
                // Another launch has taken its place meanwhile.
                Some(_) | None => return,
            };
            match account {
                Ok(account) => {
                    state.hearing = Some((run, Hearing::Known(account.id.clone())));
                    (account.id, waiting)
                }
                Err(e) => {
                    debug!(session = %self.id, "could not learn which account the agent runs under: {e}");
                    state.hearing = None;
                    return;
                }
            }
        };
        if let (Some(account), Some((limits, heard_at_ms))) = (account, waiting) {
            hub.hear_limits(account, limits, heard_at_ms).await;
        }
    }
}

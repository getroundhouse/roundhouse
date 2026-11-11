//! Which account the session's agent runs under, and filing what it hears of that account's limits under it. The line
//! an agent hears them in doesn't name the account, so the session asks the worker which it is as each launch of the
//! agent starts, and its record names it from then on.

use std::sync::Arc;
use std::time::Duration;

use protocol::model::{AccountId, AgentAccount, RunId, UsageLimit};
use tracing::{debug, error};

use super::LiveSession;
use crate::hub::Hub;

/// How long the session waits before asking the worker again, each time it couldn't say, as when the CLI's call timed
/// out. Until it can, the session goes on naming the account the last launch ran under, which is nearly always the
/// same one.
const RETRIES: [Duration; 4] =
    [Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(4), Duration::from_secs(8)];

/// Which account the agent going runs under, as far as the session knows.
pub(super) enum Hearing {
    /// The worker is being asked. What the agent hears meanwhile waits here, only the newest, with when it was
    /// heard.
    Asking(Option<(Vec<UsageLimit>, i64)>),
    /// `None` for a login that can't be told from another, whose limits aren't kept.
    Known(Option<AccountId>),
}

impl LiveSession {
    /// The agent of `run` has launched. Which account it runs under is learnt now, off this task, so the session's
    /// record says whose account it is before the agent has heard of any limit, and it's known by the time it has.
    pub(super) fn launched(self: &Arc<Self>, run: RunId) {
        {
            let mut state = self.state.lock();
            let this_launch = state.hearing.as_ref().is_some_and(|(of, _)| *of == run);
            if state.agent.is_none_or(|agent| agent.run != run) || this_launch {
                return;
            }
            state.hearing = Some((run, Hearing::Asking(None)));
        }
        tokio::spawn(self.clone().learn_account(run));
    }

    /// The agent of `run` heard of its account's limits, at `heard_at_ms`. Only the agent going speaks for the
    /// session. Once the account is known they're filed under it. While it's being learnt they wait, and if the
    /// worker couldn't say, it's asked again now, off this task, since the answer takes a call to the agent's CLI and
    /// the agent's events mustn't wait on it.
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

    /// Asks which account the agent of `run` runs under, files it on the account's record and the session's, and
    /// then what the agent heard meanwhile. A worker that can't say is asked again a few times. If it still can't,
    /// nothing is known, and it's asked again when the agent next hears of a limit.
    async fn learn_account(self: Arc<Self>, run: RunId) {
        let Ok(hub) = self.hub() else { return };
        let mut retries = RETRIES.iter();
        let account = loop {
            let e = match hub.account_of(&self.workspace_id, self.agent).await {
                Ok(account) => break account,
                Err(e) => e,
            };
            debug!(session = %self.id, "could not learn which account the agent runs under: {e}");
            let Some(wait) = retries.next() else {
                let mut state = self.state.lock();
                if asking_for(state.hearing.as_ref(), run) {
                    state.hearing = None;
                }
                return;
            };
            tokio::time::sleep(*wait).await;
            if !asking_for(self.state.lock().hearing.as_ref(), run) {
                return;
            }
        };
        let id = account.logged_in_as().cloned();
        let waiting = {
            // Held as events are recorded, from making sure this is still the launch going until the session's record
            // says so and has been announced. So a launch that replaced it can't file its own account in between and
            // be overwritten, and the feed hears each copy of the record in the order they were made.
            let _in_order = self.recording.lock().await;
            let waiting = {
                let mut state = self.state.lock();
                let waiting = match &mut state.hearing {
                    Some((of, Hearing::Asking(waiting))) if *of == run => waiting.take(),
                    // Another launch has taken its place meanwhile.
                    Some(_) | None => return,
                };
                state.hearing = Some((run, Hearing::Known(id.clone())));
                waiting
            };
            self.file_account(&hub, account).await;
            waiting
        };
        if let (Some(id), Some((limits, heard_at_ms))) = (id, waiting) {
            hub.hear_limits(id, limits, heard_at_ms).await;
        }
    }

    /// Files that the agent going launched under `account`, on the account's record and the session's, and announces
    /// the session's if that changed it.
    async fn file_account(&self, hub: &Hub, account: AgentAccount) {
        match hub.launched_under(&self.id, account).await {
            Ok(true) => {
                if let Err(e) = self.announce(hub).await {
                    error!(session = %self.id, "could not announce which account the agent runs under: {e}");
                }
            }
            Ok(false) => {}
            Err(e) => error!(session = %self.id, "could not record which account the agent runs under: {e}"),
        }
    }
}

/// Whether the session is still asking which account `run` runs under, rather than another launch having taken its
/// place.
fn asking_for(hearing: Option<&(RunId, Hearing)>, run: RunId) -> bool {
    matches!(hearing, Some((of, Hearing::Asking(_))) if *of == run)
}

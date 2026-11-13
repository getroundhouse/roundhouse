//! Which account the session's agent runs under, and filing what it hears of that account's limits under it. The line
//! an agent hears them in doesn't name the account, so the session learns which it is as each launch of the agent
//! starts, and its record names it from then on. A launch the hub gave a login it holds is under that login's account,
//! as the hub's own record says. The session asks the worker about any other.

use std::sync::Arc;
use std::time::Duration;

use protocol::model::{AccountId, AgentAccount, RunId, UsageLimit};
use tracing::{debug, error};

use super::LiveSession;
use crate::hub::Hub;

/// How long the session waits before asking the worker again, each time it couldn't say, as when the CLI's call timed
/// out. Until it can, the session goes on naming the account the last launch ran under, which is nearly always the
/// same one, unless the hub gave the last launch a login and this one none, and then it names none.
const RETRIES: [Duration; 4] =
    [Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(4), Duration::from_secs(8)];

/// What the agent heard of its limits while its account was being learnt: only the newest, with when it was heard.
type Waiting = Option<(Vec<UsageLimit>, i64)>;

/// Which account the agent going runs under, as far as the session knows.
pub(super) enum Hearing {
    /// It's being learnt, from the hub's record of the login it gave the run, or from the worker. What the agent hears
    /// meanwhile waits here.
    Asking(Waiting),
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

    /// Learns which account the agent of `run` runs under, files it on the account's record and the session's, and
    /// then what the agent heard meanwhile. A worker that can't say is asked again a few times. If it still can't,
    /// nothing is known, and it's asked again when the agent next hears of a limit.
    async fn learn_account(self: Arc<Self>, run: RunId) {
        let Ok(hub) = self.hub() else { return };
        let Filing::AskTheWorker = self.file_given_login(&hub, run).await else { return };
        let mut retries = RETRIES.iter();
        let account = loop {
            let e = match hub.account_of(&self.workspace_id, self.agent).await {
                Ok(account) => break account,
                Err(e) => e,
            };
            debug!(session = %self.id, "could not learn which account the agent runs under: {e}");
            let Some(wait) = retries.next() else {
                self.stop_asking(run);
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
            let Some(waiting) = self.known(run, id.clone()) else { return };
            self.file_account(&hub, account).await;
            waiting
        };
        if let (Some(id), Some((limits, heard_at_ms))) = (id, waiting) {
            hub.hear_limits(id, limits, heard_at_ms).await;
        }
    }

    /// Files the account of a run the hub gave a login it holds, from the hub's own record of that, since the worker
    /// isn't told whose a token is, and its machine's own login, if it has one, is another account. A run it gave none
    /// has its worker asked, once the session's record no longer names the account an earlier launch was given the
    /// login of. Under the lock events are recorded under, as filing what the worker says is.
    async fn file_given_login(&self, hub: &Hub, run: RunId) -> Filing {
        let (account, waiting) = {
            let _in_order = self.recording.lock().await;
            if !asking_for(self.state.lock().hearing.as_ref(), run) {
                return Filing::Done;
            }
            let id = self.id.clone();
            let filed = match hub.blocking(move |store| store.file_run_login(&id, run)).await {
                Ok(filed) => filed,
                Err(e) => {
                    error!(session = %self.id, "could not read which login the agent was launched under: {}", e.message);
                    self.stop_asking(run);
                    return Filing::Done;
                }
            };
            if filed.session_changed
                && let Err(e) = self.announce(hub).await
            {
                error!(session = %self.id, "could not announce which account the agent runs under: {e}");
            }
            let Some(account) = filed.account else { return Filing::AskTheWorker };
            let Some(waiting) = self.known(run, Some(account.clone())) else { return Filing::Done };
            (account, waiting)
        };
        if let Some((limits, heard_at_ms)) = waiting {
            hub.hear_limits(account, limits, heard_at_ms).await;
        }
        Filing::Done
    }

    /// Has the session know that `run` is under `account`, and gives back what the agent heard meanwhile. `None` once
    /// another launch has taken its place.
    fn known(&self, run: RunId, account: Option<AccountId>) -> Option<Waiting> {
        let mut state = self.state.lock();
        let waiting = match &mut state.hearing {
            Some((of, Hearing::Asking(waiting))) if *of == run => waiting.take(),
            Some(_) | None => return None,
        };
        state.hearing = Some((run, Hearing::Known(account)));
        Some(waiting)
    }

    /// Stops learning which account `run` is under, when nothing could say, so it's learnt again when the agent next
    /// hears of a limit, unless another launch has taken its place.
    fn stop_asking(&self, run: RunId) {
        let mut state = self.state.lock();
        if asking_for(state.hearing.as_ref(), run) {
            state.hearing = None;
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

/// Whether learning a run's account from the hub's own record is all there is to do.
enum Filing {
    /// It's filed, or another launch has taken its place, or the record couldn't be read, and then it's learnt again
    /// when the agent next hears of a limit.
    Done,
    /// The hub gave the run no login, so its worker is asked.
    AskTheWorker,
}

/// Whether the session is still asking which account `run` runs under, rather than another launch having taken its
/// place.
fn asking_for(hearing: Option<&(RunId, Hearing)>, run: RunId) -> bool {
    matches!(hearing, Some((of, Hearing::Asking(_))) if *of == run)
}

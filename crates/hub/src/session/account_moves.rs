//! Moving a chat to another account, as `SessionAssignAccount` says. A running agent can't change its login, so one
//! under another account is stopped, and the chat's next launch takes the account it was moved to.

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{AccountId, RunId, Session};
use tokio::sync::broadcast;

use super::{Live, LiveSession};
use crate::hub::Hub;
use crate::store::SessionLogin;

/// What a move does to the agent running, once it has decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stopping {
    /// None is running, or the one running is under the account the chat is moved to, so it's left alone.
    Nothing,
    /// One under another account, with nothing to cut short. The session lets go of it before it's stopped, so no
    /// prompt reaches it and its exit changes nothing.
    LetGo,
    /// One under another account that counts as running, as the run `run`, which the person said to stop anyway. Its
    /// exit cuts short what it was doing, as a stop does.
    CutShort { run: RunId },
}

impl LiveSession {
    /// Moves the chat to `account`, as `SessionAssignAccount` says, and gives back its record.
    ///
    /// All under the lock a launch holds while it starts the agent, so a launch either starts first, and is stopped
    /// here, or after, under `account`. The move is saved only once the agent under another account has stopped, so a
    /// hub that goes in between leaves the chat as it was, rather than moved with its old agent carried on.
    pub(crate) async fn assign_account(&self, account: AccountId, force: bool) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        hub.can_move_to(self.agent, account.clone()).await?;
        let _starts = self.configuring.lock().await;
        let stopping = self.decide_stopping(&hub, &account, force).await?;
        // Before the stop, so the agent's exit, however soon it's recorded, is heard.
        let live = matches!(stopping, Stopping::CutShort { .. }).then(|| self.subscribe());
        // Not under the lock events are recorded under, since the stop's reply comes on the stream that also brings the
        // agent's exit to be recorded.
        let stopped = if stopping == Stopping::Nothing { Ok(()) } else { self.handle_stop().await };
        if let Stopping::CutShort { run } = stopping {
            if let (Some(live), Ok(())) = (live, &stopped) {
                self.until_gone(run, live).await;
            }
            self.stop_hearing().await;
        }
        let (id, kept) = (self.id.clone(), account.clone());
        let moved = hub.blocking(move |store| store.move_session(&id, &kept)).await?;
        self.state.lock().login = SessionLogin::Hub(account);
        if moved.forgot_cache {
            self.forget_cache().await;
        }
        if let Some(record) = moved.account {
            hub.notify(HubNotice::Account(record));
        }
        // An agent that only waits is started again at once under the account the chat was moved to, as one that
        // carried on would: its CLI lists the wait again and asks after it, so it hears however soon the wait ends.
        // One whose queue is held for the person, as after a turn that failed, isn't, as a wait's end doesn't start
        // one, and hears how its wait ended once they send something.
        if stopping == Stopping::LetGo && self.state.lock().progress.status.is_active() {
            self.wake_agent();
        }
        let session = self.announce(&hub).await?;
        // As closing does, the record is announced whether or not the stop was answered, since a stop fails only with
        // the worker's connection, which takes the agent with it.
        stopped.map(|()| session)
    }

    /// Decides what the move does to the agent running, as [`Stopping`] says, under the lock events are recorded under.
    /// So a prompt is either recorded first, and finds the agent at work, or after. One recorded after an agent the move
    /// lets go of finds no agent it can reach, so its launch waits for this move and runs under `target`. One recorded
    /// after an agent the move cuts short is sent to that agent, which is still the session's until it exits, and is
    /// cut short with its turn, as a prompt sent just before a stop is. Refused with `NeedsForce`, changing nothing,
    /// when the agent counts as running under another account and `force` doesn't say to stop it anyway.
    async fn decide_stopping(&self, hub: &Hub, target: &AccountId, force: bool) -> Result<Stopping, ApiError> {
        let _in_order = self.recording.lock().await;
        let Some(agent) = self.state.lock().agent else {
            // What's being learnt of an agent that has gone isn't filed over the account the move names either.
            self.state.lock().hearing = None;
            return Ok(Stopping::Nothing);
        };
        let (id, run) = (self.id.clone(), agent.run);
        let given = hub.blocking(move |store| store.run_login(&id, run)).await?;
        if given.as_ref() == Some(target) {
            return Ok(Stopping::Nothing);
        }
        if self.counts_as_running() {
            if !force {
                let message = "moving the chat to another account stops its agent, which is at work";
                return Err(ApiError::new(ErrorCode::NeedsForce, message));
            }
            return Ok(Stopping::CutShort { run });
        }
        let mut state = self.state.lock();
        state.agent = None;
        // Which account the agent going is under is no longer the session's to learn, so what's learnt of it isn't
        // filed over the account the move names.
        state.hearing = None;
        Ok(Stopping::LetGo)
    }

    /// Waits until the run `run` is no longer the session's agent, once its exit is recorded, or its worker has gone,
    /// which `live` hears of, so everything it said before is recorded too.
    async fn until_gone(&self, run: RunId, mut live: broadcast::Receiver<Live>) {
        while self.state.lock().agent.is_some_and(|agent| agent.run == run) {
            if let Err(broadcast::error::RecvError::Closed) = live.recv().await {
                break;
            }
        }
    }

    /// Which account an agent the move cut short was under is no longer the session's to learn, whether or not its
    /// stop was answered. Under the lock a lookup of its account files what it learnt under, so one already filing is
    /// done before the move names the account the chat was moved to, rather than filed over it.
    async fn stop_hearing(&self) {
        let _in_order = self.recording.lock().await;
        self.state.lock().hearing = None;
    }
}

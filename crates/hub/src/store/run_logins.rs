//! The account whose login the hub gave each run of a session's agent it launched under one, which is how the hub
//! knows which account such a run is under, since a token doesn't say whose it is and the worker isn't told.

use protocol::model::{AccountId, RunId, SessionId};
use rusqlite::{OptionalExtension as _, params};

use super::accounts::name_account;
use super::{Store, StoreError, lock};

#[cfg(test)]
mod tests;

/// What the hub's record of the login it gave a run says, once it's filed on the session's record.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RunLogin {
    /// The account whose login the hub gave the run, if it gave it one.
    pub(crate) account: Option<AccountId>,
    /// Whether the session's record changed: it names that account now, or no longer names an earlier launch's.
    pub(crate) session_changed: bool,
}

impl Store {
    /// Records that `session`'s run `run` launches under the login the hub holds for `account`. Made before the launch
    /// is sent, and on disk before this returns, so no agent runs under a login the hub gave it without the hub's record
    /// of whose it is, whatever stops the hub or the machine.
    pub(crate) fn record_run_login(
        &self,
        session: &SessionId,
        run: RunId,
        account: &AccountId,
    ) -> Result<(), StoreError> {
        self.lasting(|tx| {
            tx.execute(
                "INSERT INTO runs_under_logins (session_id, run, account_id) VALUES (?, ?, ?)",
                params![session.as_str(), run.get().cast_signed(), account.as_str()],
            )
            .map(drop)
        })
    }

    /// Files which account `session`'s run `run` is under, from the hub's record of the login it gave the run, so the
    /// session's record names it. A run with no such record wasn't given one, so the session stops naming an account an
    /// earlier launch was given the login of, and its worker is asked. In one transaction, so the session's record never
    /// names the account of a login its agent wasn't given.
    pub(crate) fn file_run_login(&self, session: &SessionId, run: RunId) -> Result<RunLogin, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let account: Option<AccountId> = tx
            .query_row(
                "SELECT account_id FROM runs_under_logins WHERE session_id = ? AND run = ?",
                params![session.as_str(), run.get().cast_signed()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(AccountId::from);
        let session_changed = match &account {
            Some(account) => name_account(&tx, session, Some(account))?,
            None => {
                tx.execute(
                    "UPDATE sessions SET account_id = NULL
                         WHERE id = ? AND account_id IN (SELECT id FROM accounts WHERE login_agent IS NOT NULL)",
                    [session.as_str()],
                )? > 0
            }
        };
        tx.commit()?;
        Ok(RunLogin { account, session_changed })
    }

    /// Forgets the login the hub gave each run of `session` but `run`, once a worker has said `run` is the session's
    /// agent, since a session has one agent, and only the one going is asked which account it's under.
    pub(crate) fn forget_other_run_logins(&self, session: &SessionId, run: RunId) -> Result<(), StoreError> {
        lock(&self.writer).execute(
            "DELETE FROM runs_under_logins WHERE session_id = ? AND run != ?",
            params![session.as_str(), run.get().cast_signed()],
        )?;
        Ok(())
    }
}

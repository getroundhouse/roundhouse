use super::super::accounts::{name_account, named};
use super::super::{Store, StoreError, lock};
use protocol::model::{Account, AccountId, RunId, SessionId};
use rusqlite::{Connection, OptionalExtension as _, params};

/// What the hub's record of the login it gave a run says, once it's filed on the session's record.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RunLogin {
    /// The account whose login the hub gave the run, if it gave it one.
    pub(crate) account: Option<AccountId>,
    /// Whether the session's record changed: it names that account now, or no longer names an earlier launch's.
    pub(crate) session_changed: bool,
    /// The account's record, when the session's came to name it and that brought when it was last used up to the
    /// session's, which is a change to the account's record to announce.
    pub(crate) account_changed: Option<Account>,
}

/// What recording the login a launch is given found, and left.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Recorded {
    /// The account the session's record named, until the launch is filed, which is the one the API's cache holds its
    /// conversation under.
    pub(crate) before: Option<AccountId>,
    /// The account the session is on now: the one it was on, or, for one that wasn't on any, the launch's.
    pub(crate) on: Option<AccountId>,
}

impl Store {
    /// Records that `session`'s run `run` launches under the login the hub holds for `account`, and puts the session
    /// on `account` if it isn't on one yet, as its first launch under a login the hub holds does. Made before the launch
    /// is sent, and on disk before this returns, so no agent runs under a login the hub gave it without the hub's
    /// record of whose it is, whatever stops the hub or the machine.
    pub(crate) fn record_run_login(
        &self,
        session: &SessionId,
        run: RunId,
        account: &AccountId,
    ) -> Result<Recorded, StoreError> {
        self.lasting(|tx| {
            tx.execute(
                "INSERT INTO runs_under_logins (session_id, run, account_id) VALUES (?, ?, ?)",
                params![session.as_str(), run.get().cast_signed(), account.as_str()],
            )?;
            // Only where it's on none, since the revision moves on whatever sets it, even to what it was.
            tx.execute(
                "UPDATE sessions SET assigned_account_id = ? WHERE id = ? AND assigned_account_id IS NULL",
                params![account.as_str(), session.as_str()],
            )?;
            let on = tx
                .query_row("SELECT assigned_account_id FROM sessions WHERE id = ?", [session.as_str()], |row| {
                    row.get::<_, Option<String>>(0)
                })
                .optional()?
                .flatten()
                .map(AccountId::from);
            Ok(Recorded { before: named(tx, session)?, on })
        })
    }

    /// The account whose login the hub gave `session`'s run `run`, or `None` if it gave it none, so the run is under
    /// its machine's own login, which is never one the hub holds.
    pub(crate) fn run_login(&self, session: &SessionId, run: RunId) -> Result<Option<AccountId>, StoreError> {
        Ok(given(&lock(&self.reader), session, run)?)
    }

    /// Files which account `session`'s run `run` is under, from the hub's record of the login it gave the run, so the
    /// session's record names it. A run with no such record wasn't given one, so the session stops naming an account
    /// an earlier launch was given the login of, and its worker is asked. In one transaction, so the session's record
    /// never names the account of a login its agent wasn't given.
    pub(crate) fn file_run_login(&self, session: &SessionId, run: RunId) -> Result<RunLogin, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let account = given(&tx, session, run)?;
        let (session_changed, account_changed) = if let Some(account) = &account {
            let named = name_account(&tx, session, Some(account))?;
            (named.session, named.account)
        } else {
            let changed = tx.execute(
                "UPDATE sessions SET account_id = NULL
                     WHERE id = ? AND account_id IN (SELECT id FROM accounts WHERE login_agent IS NOT NULL)",
                [session.as_str()],
            )? > 0;
            (changed, None)
        };
        tx.commit()?;
        Ok(RunLogin { account, session_changed, account_changed })
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

/// The account whose login the hub gave `session`'s run `run`, if it gave it one.
fn given(conn: &Connection, session: &SessionId, run: RunId) -> rusqlite::Result<Option<AccountId>> {
    conn.query_row(
        "SELECT account_id FROM runs_under_logins WHERE session_id = ? AND run = ?",
        params![session.as_str(), run.get().cast_signed()],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map(|account| account.map(AccountId::from))
}

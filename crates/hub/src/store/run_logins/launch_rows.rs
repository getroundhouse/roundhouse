use super::super::accounts::name_account;
use super::super::sessions::{forget_cache_unless_under, on_the_hubs, session_login_of};
use super::super::{SessionLogin, Store, StoreError, lock};
use protocol::model::{Account, AccountId, RunId, SessionId};
use rusqlite::{Connection, OptionalExtension as _, params};

/// What the hub's record of the login it gave a run says, once it's filed on the session's record.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RunLogin {
    /// The account whose login the hub gave the run, if it gave it one.
    pub(crate) account: Option<AccountId>,
    /// Whether the session's record changed: it no longer names an earlier launch's account.
    pub(crate) session_changed: bool,
}

/// What put a session on none yet on the account a launch is given the login of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Placed {
    /// Its kind's default account, or none: it's on an account already.
    ByDefault,
    /// Automatic, which its record says from here.
    Automatically,
}

/// What recording the login a launch is given found, and left.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Recorded {
    /// Which login the session is on now: the one it was on, or, for one on none yet, the hub's for the launch's
    /// account.
    pub(crate) on: SessionLogin,
    /// Whether that changed the session's record, which is then for every client to hear.
    pub(crate) session_changed: bool,
    /// Whether putting it on the account forgot what was cached of its conversation, as
    /// [`forget_cache_unless_under`] says.
    pub(crate) forgot_cache: bool,
    /// The account's record, when the session's came to name it and that brought when it was last used up to the
    /// session's, which is a change to the account's record to announce.
    pub(crate) account_changed: Option<Account>,
}

impl Store {
    /// Records that `session`'s run `run` launches under the login the hub holds for `account`, and puts the session on
    /// the hub's login for `account` if it's on none yet, as its first launch does, so its record names `account` from
    /// here, and says whether Automatic `placed` it there, and another account's cache is forgotten, as a move forgets
    /// it. Made before the launch is sent, and on
    /// disk before this returns, so no agent runs under a login the hub gave it without the hub's record of whose it
    /// is, whatever stops the hub or the machine. A launch that then fails leaves the session on `account`, without
    /// another's cache, as one that starts does.
    pub(crate) fn record_run_login(
        &self,
        session: &SessionId,
        run: RunId,
        account: &AccountId,
        placed: Placed,
    ) -> Result<Recorded, StoreError> {
        self.lasting(|tx| {
            tx.execute(
                "INSERT INTO runs_under_logins (session_id, run, account_id) VALUES (?, ?, ?)",
                params![session.as_str(), run.get().cast_signed(), account.as_str()],
            )?;
            let (session_changed, forgot_cache, account_changed) =
                if session_login_of(tx, session)? == Some(SessionLogin::NoneYet) {
                    // Only where it changes, since the revision moves on whatever sets it, even to what it was.
                    let put = tx.execute(
                        "UPDATE sessions SET account_source = 'hub' WHERE id = ? AND account_source IS NOT 'hub'",
                        [session.as_str()],
                    )? > 0;
                    let automatically = tx.execute(
                        "UPDATE sessions SET placed_automatically = ?2 WHERE id = ?1 AND placed_automatically != ?2",
                        params![session.as_str(), placed == Placed::Automatically],
                    )? > 0;
                    let named = name_account(tx, session, Some(account))?;
                    let forgot_cache = forget_cache_unless_under(tx, session, named.before.as_ref(), account)?;
                    (put || automatically || named.session || forgot_cache, forgot_cache, named.account)
                } else {
                    (false, false, None)
                };
            // The run's row, just inserted, names the session's, which it can't outlive.
            let on = session_login_of(tx, session)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            Ok(Recorded { on, session_changed, forgot_cache, account_changed })
        })
    }

    /// The account whose login the hub gave `session`'s run `run`, or `None` if it gave it none, so the run is under
    /// its machine's own login, which is never one the hub holds.
    pub(crate) fn run_login(&self, session: &SessionId, run: RunId) -> Result<Option<AccountId>, StoreError> {
        Ok(given(&lock(&self.reader), session, run)?)
    }

    /// Which account `session`'s run `run` is under, from the hub's record of the login it gave the run. Recording that
    /// launch put a session on none on the hub's login for the account, which its record names from then, so a run
    /// given a login changes nothing more. A run with no such record wasn't given one, so a session that isn't on the
    /// hub's login stops naming an account an earlier launch was given the login of, and its worker is asked. One on
    /// the hub's login goes on naming the account it's on, as [`on_the_hubs`] says. In one transaction, so the
    /// session's record never names the account of a login its agent wasn't given, but for the one it's on.
    pub(crate) fn file_run_login(&self, session: &SessionId, run: RunId) -> Result<RunLogin, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let account = given(&tx, session, run)?;
        let session_changed = account.is_none()
            && !on_the_hubs(&tx, session)?
            && tx.execute(
                "UPDATE sessions SET account_id = NULL
                     WHERE id = ? AND account_id IN (SELECT id FROM accounts WHERE login_agent IS NOT NULL)",
                [session.as_str()],
            )? > 0;
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

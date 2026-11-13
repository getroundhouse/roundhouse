//! The accounts agents run under, each a record: who it is, as the worker last read it, and what agents under it have
//! heard of its limits. A limit is the account's rather than a session's, so it's kept by account, whichever session's
//! agent heard it.

use protocol::model::{Account, AccountId, AgentAccount, LimitWindow, Rev, SessionId, UsageLimit};
use rusqlite::{Connection, OptionalExtension as _, params};

use super::logins::{IdFrom, LOGIN_COLUMNS, login_at};
use super::{Store, StoreError, from_text, lock, rev_of, text_of};

#[cfg(test)]
mod tests;

/// What filing a launch's account changed, so the hub announces only what did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Launched {
    /// The account's record, when it's new or says someone else now.
    pub account: Option<Account>,
    /// The session's record names another account than it did.
    pub session: bool,
}

/// Every account, oldest first, for a list of every record.
pub(super) fn accounts(conn: &Connection) -> rusqlite::Result<Vec<Account>> {
    let ids: Vec<AccountId> = conn
        .prepare_cached("SELECT id FROM accounts ORDER BY rowid")?
        .query_map([], |row| Ok(AccountId::from(row.get::<_, String>(0)?)))?
        .collect::<rusqlite::Result<_>>()?;
    ids.iter().filter_map(|id| account(conn, id).transpose()).collect()
}

/// The record of `id`, with what's left of its limits. `None` for an account with none.
pub(super) fn account(conn: &Connection, id: &AccountId) -> rusqlite::Result<Option<Account>> {
    let Some(mut account) = conn
        .query_row(
            &format!(
                "SELECT id, model_provider, plan, login, email, rev, name, {LOGIN_COLUMNS} FROM accounts WHERE id = ?"
            ),
            [id.as_str()],
            |row| {
                let (id_from, held) = login_at(row, 7)?;
                Ok(Account {
                    id: row.get::<_, String>(0)?.into(),
                    model_provider: row.get(1)?,
                    plan: row.get(2)?,
                    login: row.get(3)?,
                    email: row.get(4)?,
                    name: row.get(6)?,
                    login_agent: match id_from {
                        IdFrom::Hub(agent) => Some(agent),
                        IdFrom::Adapter => None,
                    },
                    limits: Vec::new(),
                    held_login: held.map(|(_, held)| held),
                    rev: rev_of(row, 5)?,
                })
            },
        )
        .optional()?
    else {
        return Ok(None);
    };
    // When each was heard isn't on the record, since hearing the same again isn't a change to it.
    account.limits = limits_of(conn, id)?.into_iter().map(|limit| UsageLimit { heard_at_ms: None, ..limit }).collect();
    Ok(Some(account))
}

/// What's been heard of `account`'s limits, shortest window first, each with when it was heard.
fn limits_of(conn: &Connection, account: &AccountId) -> rusqlite::Result<Vec<UsageLimit>> {
    let mut statement = conn.prepare_cached(
        "SELECT window, used_percent, resets_at_ms, heard_at_ms FROM account_limits WHERE account_id = ?",
    )?;
    let rows = statement.query_map([account.as_str()], |row| {
        Ok(UsageLimit {
            window: from_text(row.get(0)?)?,
            used_percent: row.get(1)?,
            resets_at_ms: row.get(2)?,
            heard_at_ms: Some(row.get(3)?),
        })
    })?;
    let mut limits = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    limits.sort_by_key(|limit| limit.window);
    Ok(limits)
}

impl Store {
    /// Keeps what an agent under `account` heard at `heard_at_ms`, and returns the account's record when that changed
    /// it, with a newer revision, in the same transaction. Each window is kept by itself, so hearing of one leaves
    /// what's known of another, and something heard earlier never replaces something heard later. Hearing the same
    /// again changes only when it was heard, which isn't on the record. A window this build has no name for isn't
    /// kept, since there'd be nothing to tell it from another by. An account nobody has launched an agent under since
    /// this build gets a record of its limits alone.
    pub fn hear_limits(
        &self,
        account: &AccountId,
        limits: &[UsageLimit],
        heard_at_ms: i64,
    ) -> Result<Option<Account>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let before: Option<Rev> = tx
            .query_row("SELECT rev FROM accounts WHERE id = ?", [account.as_str()], |row| rev_of(row, 0))
            .optional()?;
        tx.execute("INSERT INTO accounts (id) VALUES (?) ON CONFLICT DO NOTHING", [account.as_str()])?;
        for limit in limits.iter().filter(|limit| limit.window != LimitWindow::Unknown) {
            tx.execute(
                "INSERT INTO account_limits (account_id, window, used_percent, resets_at_ms, heard_at_ms)
                     VALUES (?, ?, ?, ?, ?)
                     ON CONFLICT (account_id, window) DO UPDATE
                     SET used_percent = excluded.used_percent, resets_at_ms = excluded.resets_at_ms,
                         heard_at_ms = excluded.heard_at_ms
                     WHERE excluded.heard_at_ms >= heard_at_ms",
                params![account.as_str(), text_of(&limit.window), limit.used_percent, limit.resets_at_ms, heard_at_ms],
            )?;
        }
        let record = account_or_missing(&tx, account)?;
        tx.commit()?;
        Ok((before != Some(record.rev)).then_some(record))
    }

    /// Files that `session`'s agent launched under `under`, as its worker read it: who the account is, and that the
    /// session's agent runs under it. A login that can't be told from another, or nobody logged in, leaves the session
    /// naming none, since there's no record to name.
    pub fn launched_under(&self, session: &SessionId, under: &AgentAccount) -> Result<Launched, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let id = under.logged_in_as();
        let account = match id {
            Some(id) => {
                let changed = tx.execute(
                    "INSERT INTO accounts (id, model_provider, plan, login, email) VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT (id) DO UPDATE
                         SET model_provider = excluded.model_provider, plan = excluded.plan, login = excluded.login,
                             email = excluded.email
                         WHERE model_provider IS NOT excluded.model_provider OR plan IS NOT excluded.plan
                             OR login IS NOT excluded.login OR email IS NOT excluded.email",
                    params![id.as_str(), under.model_provider, under.plan, under.login, under.email],
                )?;
                (changed > 0).then(|| account_or_missing(&tx, id)).transpose()?
            }
            None => None,
        };
        let session_changed = tx.execute(
            "UPDATE sessions SET account_id = ?1 WHERE id = ?2 AND account_id IS NOT ?1",
            params![id.map(AccountId::as_str), session.as_str()],
        )? > 0;
        tx.commit()?;
        Ok(Launched { account, session: session_changed })
    }

    /// What's been heard of `account`'s limits, shortest window first, each with when it was heard.
    pub fn account_limits(&self, account: &AccountId) -> Result<Vec<UsageLimit>, StoreError> {
        Ok(limits_of(&lock(&self.reader), account)?)
    }

    /// The record of `id`. `None` for an account with none.
    pub fn account(&self, id: &AccountId) -> Result<Option<Account>, StoreError> {
        Ok(account(&lock(&self.reader), id)?)
    }
}

/// The record of an account that was just written in the same transaction, so it's there.
pub(super) fn account_or_missing(conn: &Connection, id: &AccountId) -> rusqlite::Result<Account> {
    account(conn, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

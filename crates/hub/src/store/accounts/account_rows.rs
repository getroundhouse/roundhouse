use super::super::logins::{IdFrom, LoginColumns, login_at};
use super::super::sessions::on_the_hubs;
use super::super::{ByColumn, Columns, OneValue, Store, StoreError, columns, lock, rev_of};
use protocol::model::{Account, AccountId, AgentAccount, Reserve, SessionId};
use rusqlite::{Connection, OptionalExtension as _, params};
use std::collections::HashSet;

/// What filing a launch's account changed, so the hub announces only what did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Launched {
    /// The account's record, when it's new, or when the session's record came to name it and that brought when it was
    /// last used up to the session's.
    pub account: Option<Account>,
    /// The session's record changed: it names another account than it did, or it's on the ambient login now.
    pub session: bool,
    /// The session was on none, and is on the ambient login now, since the worker said whose login its agent runs
    /// under.
    pub ambient: bool,
}

/// Every account, oldest first, for a list of every record.
pub(in super::super) fn accounts(conn: &Connection) -> rusqlite::Result<Vec<Account>> {
    let ids: Vec<AccountId> = conn
        .prepare_cached("SELECT id FROM accounts ORDER BY rowid")?
        .query_map([], OneValue::read::<String>)?
        .map(|id| id.map(AccountId::from))
        .collect::<rusqlite::Result<_>>()?;
    ids.iter().filter_map(|id| account(conn, id).transpose()).collect()
}

columns! {
    /// What an account's record reads of its row, before its [`LoginColumns`].
    struct AccountColumns { id, rev, name, used_at_ms, reserve_percent }
}

/// The record of `id`, with none of who it is or what's left of its limits, which are observations. `None` for an
/// account with none.
pub(in super::super) fn account(conn: &Connection, id: &AccountId) -> rusqlite::Result<Option<Account>> {
    conn.query_row(
        &format!(
            "SELECT {}, {}
                     FROM accounts
                     WHERE id = ?",
            AccountColumns::LIST,
            LoginColumns::LIST
        ),
        [id.as_str()],
        |row| {
            let c = AccountColumns::at(0);
            let (id_from, held) = login_at(row, LoginColumns::at(AccountColumns::COUNT))?;
            Ok(Account {
                id: row.at::<String>(c.id)?.into(),
                name: row.at(c.name)?,
                login_agent: match id_from {
                    IdFrom::Hub(agent) => Some(agent),
                    IdFrom::Adapter => None,
                },
                held_login: held.map(|(_, held)| held),
                used_at_ms: row.at(c.used_at_ms)?,
                // Only a reserve the hub took is kept, so one it can't read is as good as none.
                reserve: row
                    .at::<Option<i64>>(c.reserve_percent)?
                    .and_then(|percent| u8::try_from(percent).ok())
                    .and_then(Reserve::new),
                rev: rev_of(row, c.rev)?,
            })
        },
    )
    .optional()
}

impl Store {
    /// Makes a record of `account`, if it has none, as hearing of its limits does for one nobody has launched an agent
    /// under since the database was made, and returns the record it made.
    pub fn account_made(&self, id: &AccountId) -> Result<Option<Account>, StoreError> {
        let conn = lock(&self.writer);
        let made = conn.execute("INSERT INTO accounts (id) VALUES (?) ON CONFLICT DO NOTHING", [id.as_str()])? > 0;
        Ok(if made { account(&conn, id)? } else { None })
    }

    /// Files that `session`'s agent launched under `under`, as its worker read it: that the account is there, and that
    /// the session's agent runs under it. Who the account is, the hub keeps as an observation. A login that can't be told from another, or nobody logged in, leaves the session
    /// naming none, since there's no record to name. A session on the hub's login goes on naming the account it's on,
    /// as `on_the_hubs` says, whatever an agent under another login says.
    ///
    /// A session on none is put on the ambient login once the worker says whose it is, since the hub gave its agent
    /// none, as a session's first launch while its kind has no default does. One whose login nobody can say, as with
    /// nobody logged in or a login by API key, stays on none, so a launch once a login is added puts it on the default,
    /// rather than leave it on a login with no account to show, and so nothing to move it off from. On disk before this
    /// returns, as a held login's launch is, since a power cut that took it back would leave the session on none.
    pub fn launched_under(&self, session: &SessionId, under: &AgentAccount) -> Result<Launched, StoreError> {
        self.lasting(|tx| launched_under(tx, session, under))
    }

    /// Every account's ID.
    pub(crate) fn account_ids(&self) -> Result<HashSet<AccountId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare("SELECT id FROM accounts")?;
        let ids = statement.query_map([], OneValue::read::<String>)?;
        Ok(ids.map(|id| id.map(AccountId::from)).collect::<rusqlite::Result<_>>()?)
    }

    /// The record of `id`. `None` for an account with none.
    pub fn account(&self, id: &AccountId) -> Result<Option<Account>, StoreError> {
        Ok(account(&lock(&self.reader), id)?)
    }
}

/// What [`Store::launched_under`] does, in its transaction.
fn launched_under(tx: &Connection, session: &SessionId, under: &AgentAccount) -> rusqlite::Result<Launched> {
    let id = under.logged_in_as();
    let made = match id {
        Some(id) => tx.execute("INSERT INTO accounts (id) VALUES (?) ON CONFLICT (id) DO NOTHING", [id.as_str()])? > 0,
        None => false,
    };
    let on_the_hubs = on_the_hubs(tx, session)?;
    let named = if on_the_hubs { None } else { Some(name_account(tx, session, id)?) };
    let (named_another, named) = named.map_or((false, None), |named| (named.session, named.account));
    let ambient = !on_the_hubs
        && id.is_some()
        && tx.execute(
            "UPDATE sessions SET account_source = 'ambient' WHERE id = ? AND account_source IS NULL",
            [session.as_str()],
        )? > 0;
    let account = match (named, id) {
        (Some(record), _) => Some(record),
        (None, Some(id)) if made => Some(account_or_missing(tx, id)?),
        (None, _) => None,
    };
    Ok(Launched { account, session: named_another || ambient, ambient })
}

/// What naming a session's account found and changed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Named {
    /// The account the session's record named before, if any.
    pub(crate) before: Option<AccountId>,
    /// The session's record names another account than it did, or none.
    pub(crate) session: bool,
    /// The record of the account it names now, when naming it brought when the account was last used up to the
    /// session's last send, as [`used_by_session`] says.
    pub(crate) account: Option<Account>,
}

/// Has `session`'s record name `account` as the one its agent runs under, or none, and says what that changed.
///
/// A session that named no account comes to name one once its first launch is filed, and what the person sent before
/// then is that account's use, since it was recorded before anything said whose account the launch was under. A
/// session that named one already had what it was sent counted for that one, so the account it moves to isn't given
/// it too, however the move came: a send whose launch ran under another login, or a launch the hub started by itself.
/// Each send counts for one account.
pub(in super::super) fn name_account(
    conn: &Connection,
    session: &SessionId,
    account: Option<&AccountId>,
) -> rusqlite::Result<Named> {
    let before = named(conn, session)?;
    let changed = conn.execute(
        "UPDATE sessions SET account_id = ?1 WHERE id = ?2 AND account_id IS NOT ?1",
        params![account.map(AccountId::as_str), session.as_str()],
    )? > 0;
    let account = if changed && before.is_none() && account.is_some() { used_by_session(conn, session)? } else { None };
    Ok(Named { before, session: changed, account })
}

/// The account `session`'s record names as the one its agent runs under.
fn named(conn: &Connection, session: &SessionId) -> rusqlite::Result<Option<AccountId>> {
    conn.query_row("SELECT account_id FROM sessions WHERE id = ?", [session.as_str()], OneValue::read::<Option<String>>)
        .optional()
        .map(|account| account.flatten().map(AccountId::from))
}

/// Brings when the account `session`'s record names was last used up to when the person last sent the session's agent
/// something, and gives back the account's record if that changed it. An account used later, in another session, keeps
/// that. So what the person sends counts for the account the session names as they send it, which is the one its
/// newest launch ran under, even when no agent is running and the next launch turns out to run under another.
pub(in super::super) fn used_by_session(conn: &Connection, session: &SessionId) -> rusqlite::Result<Option<Account>> {
    let changed = conn.execute(
        "UPDATE accounts SET used_at_ms = sessions.user_active_at_ms FROM sessions
             WHERE sessions.id = ?1 AND accounts.id = sessions.account_id
                 AND sessions.user_active_at_ms > coalesce(accounts.used_at_ms, -1)",
        [session.as_str()],
    )?;
    if changed == 0 {
        return Ok(None);
    }
    let id: String =
        conn.query_row("SELECT account_id FROM sessions WHERE id = ?", [session.as_str()], OneValue::read)?;
    account(conn, &id.into())
}

/// The record of an account that was just written in the same transaction, so it's there.
pub(in super::super) fn account_or_missing(conn: &Connection, id: &AccountId) -> rusqlite::Result<Account> {
    account(conn, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

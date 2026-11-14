use super::super::accounts::account_or_missing;
use super::super::{Store, StoreError, from_text, lock, text_of};
use crate::login_files::LoginFile;
use protocol::model::{Account, AccountId, AgentKind, HeldLogin, HubSettings};
use rusqlite::{OptionalExtension as _, Row, params};

/// Who chose an account's ID, which says whether a login connected to it can be its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IdFrom {
    /// Its agents' adapter, from the login on a worker's machine.
    Adapter,
    /// The hub, for a login it was given for agents of this kind.
    Hub(AgentKind),
}

/// What an account's row says of a login: whose ID the account has, and the file of the login the hub holds for it, if
/// it holds one.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LoginRow {
    pub(crate) id_from: IdFrom,
    pub(crate) file: Option<LoginFile>,
}

/// A login the hub holds, with its account, what the person called it, and the file its token is in.
pub(crate) struct Held {
    pub(crate) account: AccountId,
    pub(crate) file: LoginFile,
    pub(crate) login: HeldLogin,
}

/// The account and any first default committed together, plus the token file to retire.
pub(crate) struct Connected {
    pub(crate) account: Account,
    pub(crate) replaced: Option<LoginFile>,
    pub(crate) defaults: Option<HubSettings>,
}

/// A login being connected to an account, which is made if it has no record.
#[derive(Clone, Copy)]
pub(crate) struct Connecting<'a> {
    pub(crate) account: &'a AccountId,
    pub(crate) agent: AgentKind,
    pub(crate) name: &'a str,
    pub(crate) file: &'a LoginFile,
    pub(crate) connected_at_ms: i64,
    pub(crate) ends_at_ms: Option<i64>,
}

/// The columns every read of a row's login takes, in this order, from `LOGIN_COLUMNS`.
pub(in super::super) const LOGIN_COLUMNS: &str = "login_agent, login_file, login_connected_at_ms, login_ends_at_ms";

/// A row's login, as its [`LOGIN_COLUMNS`] say from `first` on: whose ID it has, and the login the hub holds for it
/// with its file. It holds one only when the file is one the hub names and it says when it was connected, so every
/// read takes the same to be held. One that names another file is cleared as the hub starts.
pub(in super::super) fn login_at(
    row: &Row<'_>,
    first: usize,
) -> rusqlite::Result<(IdFrom, Option<(LoginFile, HeldLogin)>)> {
    let id_from = match row.get::<_, Option<String>>(first)? {
        Some(agent) => IdFrom::Hub(from_text(agent)?),
        None => IdFrom::Adapter,
    };
    let file = row.get::<_, Option<String>>(first + 1)?.as_deref().and_then(LoginFile::parse);
    let connected_at_ms: Option<i64> = row.get(first + 2)?;
    let ends_at_ms: Option<i64> = row.get(first + 3)?;
    let held =
        file.zip(connected_at_ms).map(|(file, connected_at_ms)| (file, HeldLogin { connected_at_ms, ends_at_ms }));
    Ok((id_from, held))
}

impl Store {
    /// Every login the hub holds for agents of `agent`'s kind, the one connected most recently first.
    pub(crate) fn held_logins(&self, agent: AgentKind) -> Result<Vec<Held>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT id, {LOGIN_COLUMNS} FROM accounts WHERE login_agent = ? AND login_file IS NOT NULL
                 ORDER BY login_connected_at_ms DESC, id"
        ))?;
        let rows = statement.query_map([text_of(&agent)], |row| Ok((row.get::<_, String>(0)?, login_at(row, 1)?.1)))?;
        let mut held = Vec::new();
        for row in rows {
            if let (account, Some((file, login))) = row? {
                held.push(Held { account: account.into(), file, login });
            }
        }
        Ok(held)
    }

    /// What `id`'s row says of a login.
    pub(crate) fn login_row(&self, id: &AccountId) -> Result<LoginRow, StoreError> {
        let conn = lock(&self.reader);
        let row = conn
            .query_row(&format!("SELECT {LOGIN_COLUMNS} FROM accounts WHERE id = ?"), [id.as_str()], |row| {
                login_at(row, 0)
            })
            .optional()?;
        let (id_from, held) = row.ok_or_else(|| no_account(id))?;
        Ok(LoginRow { id_from, file: held.map(|(file, _)| file) })
    }

    /// Has `connecting.account` hold the login whose token is in `connecting.file`, with its name, and makes the
    /// account's record if it has none. Returns the record, and the file of the login it replaced, for the hub to let
    /// go of. Its commit is on disk before this returns, so a login the hub has said it holds isn't lost to a power
    /// cut, and the hub lets go of the replaced login's file only after.
    pub(crate) fn connect_login(&self, connecting: &Connecting<'_>) -> Result<Connected, StoreError> {
        let Connecting { account: id, agent, name, file, connected_at_ms, ends_at_ms } = *connecting;
        self.lasting(|tx| {
            let replaced: Option<String> = tx
                .query_row("SELECT login_file FROM accounts WHERE id = ?", [id.as_str()], |row| row.get(0))
                .optional()?
                .flatten();
            tx.execute(
                "INSERT INTO accounts (id, name, login_agent, login_file, login_connected_at_ms, login_ends_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT (id) DO UPDATE
                     SET name = excluded.name, login_agent = excluded.login_agent, login_file = excluded.login_file,
                         login_connected_at_ms = excluded.login_connected_at_ms,
                         login_ends_at_ms = excluded.login_ends_at_ms",
                params![id.as_str(), name, text_of(&agent), file.as_str(), connected_at_ms, ends_at_ms],
            )?;
            let record = account_or_missing(tx, id)?;
            let first = tx.execute(
                "INSERT INTO account_defaults (agent, account_id) VALUES (?, ?)
                ON CONFLICT (agent) DO NOTHING",
                params![text_of(&agent), id.as_str()],
            )?;
            let defaults = if first > 0 { Some(super::super::hub_settings::hub_settings(tx)?) } else { None };
            Ok(Connected { account: record, replaced: replaced.as_deref().and_then(LoginFile::parse), defaults })
        })
    }

    /// Gives the account `id` the name `name`, and returns its record when that changed it.
    pub(crate) fn rename_account(&self, id: &AccountId, name: &str) -> Result<Option<Account>, StoreError> {
        let conn = lock(&self.writer);
        let changed =
            conn.execute("UPDATE accounts SET name = ?1 WHERE id = ?2 AND name IS NOT ?1", params![name, id.as_str()])?;
        Ok(if changed > 0 { Some(account_or_missing(&conn, id)?) } else { None })
    }

    /// Has the account `id` hold no login, keeping the rest of its record, and returns the record when that changed it.
    /// The hub lets go of the login's file first, so a power cut that takes this back leaves a row naming a file that's
    /// gone, which is cleared as the hub next starts, and never a token that was disconnected.
    pub(crate) fn clear_login(&self, id: &AccountId) -> Result<Option<Account>, StoreError> {
        let conn = lock(&self.writer);
        let changed = conn.execute(
            "UPDATE accounts SET login_file = NULL, login_connected_at_ms = NULL, login_ends_at_ms = NULL
                 WHERE id = ? AND login_file IS NOT NULL",
            [id.as_str()],
        )?;
        Ok(if changed > 0 { Some(account_or_missing(&conn, id)?) } else { None })
    }

    /// The file of every login a row names, by account, for the hub to make the files and the rows agree as it starts.
    /// `None` for a row that names one but doesn't hold it, as [`login_at`] says, which tidying clears.
    pub(crate) fn login_files(&self) -> Result<Vec<(AccountId, Option<LoginFile>)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement =
            conn.prepare_cached(&format!("SELECT id, {LOGIN_COLUMNS} FROM accounts WHERE login_file IS NOT NULL"))?;
        let rows = statement.query_map([], |row| {
            Ok((AccountId::from(row.get::<_, String>(0)?), login_at(row, 1)?.1.map(|(file, _)| file)))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

fn no_account(id: &AccountId) -> StoreError {
    StoreError::NotFound { kind: "account", id: id.to_string() }
}

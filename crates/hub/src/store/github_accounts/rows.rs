use super::super::hub_settings::hub_settings;
use super::super::{
    ByColumn, Columns, OneValue, Store, StoreError, columns, current_rev, lock, rev_of, text_from, text_of,
};
use crate::login_files::LoginFile;
use protocol::model::{
    GithubAccount, GithubAccountId, GithubAppId, GithubInstallation, GithubInstallationId, GithubLoginKind, GithubUser,
    GithubUserId, HeldGithubLogin, HubSettings, Rev,
};
use rusqlite::{Connection, OptionalExtension as _, Row, Transaction, params};
use std::collections::HashSet;

columns! {
    /// The columns every read of a GitHub account's row takes, as [`account_at`] reads them.
    pub(super) struct GithubAccountColumns {
        id, user_id, login_kind, login_file, login_connected_at_ms, login_ends_at_ms, rev,
        login_needs_sign_in, app_id, installation_id, installed_on_id,
    }
}

/// A login being connected to a GitHub account, which is made if the hub has no record of it.
#[derive(Clone, Copy)]
pub(crate) struct GithubConnecting<'a> {
    pub(crate) account: &'a GithubAccountId,
    pub(crate) user: &'a GithubUser,
    pub(crate) how: GithubLoginKind,
    pub(crate) file: &'a LoginFile,
    pub(crate) connected_at_ms: i64,
    pub(crate) ends_at_ms: Option<i64>,
    /// Where it's installed, for an app's installation, and `None` for any other login.
    pub(crate) installation: Option<&'a GithubInstallation>,
}

/// The account as connecting left it, the file of the login it replaced, for the hub to let go of, and the settings,
/// when connecting made it the default.
pub(crate) struct GithubConnected {
    pub(crate) account: GithubAccount,
    pub(crate) replaced: Option<LoginFile>,
    pub(crate) settings: Option<HubSettings>,
}

/// A row's record, and the file of the login it holds. It holds one only when the file is one the hub names, and it
/// says how and when it was connected, so every read takes the same to be held. One that names another file is cleared
/// as the hub starts. What GitHub calls its user isn't kept here, since the hub only observes it, as
/// `crate::hub::observations::GithubUsername` says, so the record names the user by its number, as `#<number>`, which
/// the hub fills with the username it holds as the record leaves it, as `crate::hub::observations::OldForm` says.
pub(super) fn account_at(row: &Row<'_>) -> rusqlite::Result<(GithubAccount, Option<LoginFile>)> {
    let c = GithubAccountColumns::at(0);
    let file = row.at::<Option<String>>(c.login_file)?.as_deref().and_then(LoginFile::parse);
    let held = match (file, row.at::<Option<String>>(c.login_kind)?, row.at::<Option<i64>>(c.login_connected_at_ms)?) {
        (Some(file), Some(how), Some(connected_at_ms)) => {
            let held = HeldGithubLogin {
                how: text_from(c.login_kind, how)?,
                connected_at_ms,
                ends_at_ms: row.at(c.login_ends_at_ms)?,
                needs_sign_in: row.at(c.login_needs_sign_in)?,
            };
            Some((file, held))
        }
        _ => None,
    };
    let account = GithubAccount {
        id: row.at::<String>(c.id)?.into(),
        user: unnamed(GithubUserId::new(row.at(c.user_id)?)),
        held_login: held.as_ref().map(|(_, held)| *held),
        installation: installation_at(row)?,
        rev: rev_of(row, c.rev)?,
    };
    Ok((account, held.map(|(file, _)| file)))
}

/// The GitHub user `id`, named by its number, `#<number>`, as a record names it while what GitHub calls it isn't known.
/// *Unconfirmed:* a client of a version from before the username was an observation is sent that.
fn unnamed(id: GithubUserId) -> GithubUser {
    GithubUser { id, name: format!("#{id}").into() }
}

/// Where a row's account is installed, when it's an app's installation, as all three of its columns say: its app, the
/// installation, and the user or organization it's on, by number, which links the account to GitHub's installation.
/// What GitHub calls that user or organization the hub only observes, as `crate::hub::observations::InstalledOn` says,
/// so it's named by its number here, as a user is.
fn installation_at(row: &Row<'_>) -> rusqlite::Result<Option<GithubInstallation>> {
    let c = GithubAccountColumns::at(0);
    let columns = (row.at::<Option<u64>>(c.app_id)?, row.at(c.installation_id)?, row.at(c.installed_on_id)?);
    let (Some(app), Some(id), Some(on_id)) = columns else { return Ok(None) };
    Ok(Some(GithubInstallation {
        app: GithubAppId::new(app),
        id: GithubInstallationId::new(id),
        on: unnamed(GithubUserId::new(on_id)),
    }))
}

/// Every GitHub account, oldest first, for a list of every record.
pub(in super::super) fn github_accounts(conn: &Connection) -> rusqlite::Result<Vec<GithubAccount>> {
    let mut statement =
        conn.prepare_cached(&format!("SELECT {} FROM github_accounts ORDER BY rowid", GithubAccountColumns::LIST))?;
    statement.query_map([], |row| account_at(row).map(|(account, _)| account))?.collect()
}

/// The row of `id`, as [`account_at`] reads it.
pub(super) fn row_of(
    conn: &Connection,
    id: &GithubAccountId,
) -> rusqlite::Result<Option<(GithubAccount, Option<LoginFile>)>> {
    conn.query_row(
        &format!("SELECT {} FROM github_accounts WHERE id = ?", GithubAccountColumns::LIST),
        [id.as_str()],
        account_at,
    )
    .optional()
}

impl Store {
    /// Whether the hub has any GitHub account's record.
    pub(crate) fn has_github_accounts(&self) -> Result<bool, StoreError> {
        let conn = lock(&self.reader);
        Ok(conn.query_row("SELECT EXISTS (SELECT 1 FROM github_accounts)", [], OneValue::read)?)
    }

    /// Every GitHub account's ID.
    pub(crate) fn github_account_ids(&self) -> Result<HashSet<GithubAccountId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare("SELECT id FROM github_accounts")?;
        let ids = statement.query_map([], OneValue::read::<String>)?;
        Ok(ids.map(|id| id.map(GithubAccountId::from)).collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn github_account(&self, id: &GithubAccountId) -> Result<Option<GithubAccount>, StoreError> {
        Ok(row_of(&lock(&self.reader), id)?.map(|(account, _)| account))
    }

    /// The file of the login the hub holds for `id`, or `None` while it holds none. Refused when there's no such
    /// account.
    pub(crate) fn github_login_file(&self, id: &GithubAccountId) -> Result<Option<LoginFile>, StoreError> {
        let row = row_of(&lock(&self.reader), id)?;
        let (_, file) = row.ok_or_else(|| StoreError::NotFound { kind: "GitHub account", id: id.to_string() })?;
        Ok(file)
    }

    /// Every GitHub account, oldest first, with the file of the login it holds, if it holds one, for the hub's daily
    /// read of who each is on GitHub.
    pub(crate) fn github_accounts_and_files(&self) -> Result<Vec<(GithubAccount, Option<LoginFile>)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement =
            conn.prepare_cached(&format!("SELECT {} FROM github_accounts ORDER BY rowid", GithubAccountColumns::LIST))?;
        Ok(statement.query_map([], account_at)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Every login the hub holds for a GitHub account, by account, with its file, for finding the one that holds a
    /// token.
    pub(crate) fn held_github_logins(&self) -> Result<Vec<(GithubAccountId, LoginFile)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {} FROM github_accounts WHERE login_file IS NOT NULL",
            GithubAccountColumns::LIST
        ))?;
        let rows = statement.query_map([], account_at)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter_map(|(account, file)| Some((account.id, file?))).collect())
    }

    /// The file of every login a row names, by account, for the hub to make the files and the rows agree as it starts.
    /// `None` for a row that names one but doesn't hold it, as [`account_at`] says, which tidying clears.
    pub(crate) fn github_login_files(&self) -> Result<Vec<(GithubAccountId, Option<LoginFile>)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {} FROM github_accounts WHERE login_file IS NOT NULL",
            GithubAccountColumns::LIST
        ))?;
        let rows = statement.query_map([], |row| account_at(row).map(|(account, file)| (account.id, file)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Has `connecting.account` hold the login whose token is in `connecting.file`, as `connecting.user`, and makes the
    /// account's record if it has none, as [`connect_login`] says. Its commit is on disk before this returns, so a
    /// login the hub has said it holds isn't lost to a power cut, and the hub lets go of the replaced login's file only
    /// after.
    pub(crate) fn connect_github_login(
        &self,
        connecting: &GithubConnecting<'_>,
    ) -> Result<GithubConnected, StoreError> {
        self.lasting(|tx| connect_login(tx, connecting))
    }

    /// Every login of kind `how` the hub holds as an account's, oldest first, with the account's record and the login's
    /// file: sign-ins with GitHub, for renewing each before its token ends, and `gh`'s, for reading its token.
    pub(crate) fn held_github_logins_of(
        &self,
        how: GithubLoginKind,
    ) -> Result<Vec<(GithubAccount, LoginFile)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {} FROM github_accounts WHERE login_kind = ? AND login_file IS NOT NULL ORDER BY rowid",
            GithubAccountColumns::LIST
        ))?;
        let rows = statement.query_map([text_of(&how)], account_at)?;
        let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter_map(|(account, file)| Some((account, file?))).collect())
    }

    /// Says the GitHub account `id`'s login, whose file the hub has just renewed in place, now ends at `ends_at_ms`, and
    /// returns the record when that changed it. When it was connected stays as it was, since the person did nothing.
    /// Not synced, since the file is what lasts: a row a power cut takes back says the login ends when it did, so it's
    /// renewed again, with what the file holds.
    pub(crate) fn renew_github_sign_in(
        &self,
        id: &GithubAccountId,
        ends_at_ms: Option<i64>,
    ) -> Result<Option<GithubAccount>, StoreError> {
        let conn = lock(&self.writer);
        let changed = conn.execute(
            "UPDATE github_accounts SET login_ends_at_ms = ? WHERE id = ? AND login_file IS NOT NULL",
            params![ends_at_ms, id.as_str()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        Ok(row_of(&conn, id)?.map(|(account, _)| account))
    }

    /// Moves the revision of `id`'s record, since what GitHub calls its user changed, which the record holds the old
    /// form of, and returns the record. `None` for an account with none.
    pub(in super::super) fn github_account_moved(
        &self,
        id: &GithubAccountId,
    ) -> Result<Option<GithubAccount>, StoreError> {
        let conn = lock(&self.writer);
        // Moves the revision, as `github_accounts_rev_on_update` does for any update that doesn't.
        conn.execute("UPDATE github_accounts SET id = id WHERE id = ?", [id.as_str()])?;
        Ok(row_of(&conn, id)?.map(|(account, _)| account))
    }

    /// Says the login the GitHub account `id` holds needs signing in again, as when GitHub has refused what renews a
    /// sign-in's token, or nothing can, and returns the record when that changed it.
    pub(crate) fn github_login_needs_sign_in(&self, id: &GithubAccountId) -> Result<Option<GithubAccount>, StoreError> {
        let conn = lock(&self.writer);
        let changed = conn.execute(
            "UPDATE github_accounts SET login_needs_sign_in = 1
                 WHERE id = ? AND login_file IS NOT NULL AND login_needs_sign_in = 0",
            [id.as_str()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        Ok(row_of(&conn, id)?.map(|(account, _)| account))
    }

    /// Has the GitHub account `id` hold no login, keeping the rest of its record, and returns the record when that
    /// changed it. The hub lets go of the login's file first, so a power cut that takes this back leaves a row naming a
    /// file that's gone, which is cleared as the hub next starts, and never a token that was disconnected.
    pub(crate) fn clear_github_login(&self, id: &GithubAccountId) -> Result<Option<GithubAccount>, StoreError> {
        let conn = lock(&self.writer);
        let changed = conn.execute(
            "UPDATE github_accounts
                 SET login_kind = NULL, login_file = NULL, login_connected_at_ms = NULL, login_ends_at_ms = NULL,
                     login_needs_sign_in = 0
                 WHERE id = ? AND login_file IS NOT NULL",
            [id.as_str()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        Ok(row_of(&conn, id)?.map(|(account, _)| account))
    }

    /// Forgets the GitHub account `id`, as disconnecting any but an app's installation does: its record, and the hub's
    /// settings' naming it as the default, in one transaction. Returns the
    /// record as it last was, the revision its removal moved the records to, and the settings when it was the default,
    /// or `None` when there's no such account. The hub lets go of the login's file first, so a power cut that takes
    /// this back leaves a row naming a file that's gone, which is removed as the hub next starts.
    pub(crate) fn remove_github_account(&self, id: &GithubAccountId) -> Result<Option<GithubRemoved>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let Some((account, _)) = row_of(&tx, id)? else { return Ok(None) };
        let was_default =
            tx.execute("UPDATE hub_settings SET github_account_id = NULL WHERE github_account_id = ?", [id.as_str()])?;
        tx.execute("DELETE FROM github_accounts WHERE id = ?", [id.as_str()])?;
        let rev = current_rev(&tx)?;
        let settings = if was_default > 0 { Some(hub_settings(&tx)?) } else { None };
        tx.commit()?;
        Ok(Some(GithubRemoved { account, rev, settings }))
    }
}

/// A GitHub account the hub has forgotten: its record as it last was, the revision its removal moved the records to,
/// and the hub's settings when it was the default, which then name none.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GithubRemoved {
    pub(crate) account: GithubAccount,
    pub(crate) rev: Rev,
    pub(crate) settings: Option<HubSettings>,
}

/// Has `connecting.account` hold the login whose token is in `connecting.file`, as `connecting.user`, which needs no
/// signing in again, in `tx`, and makes the account's record if it has none. What GitHub calls the user, and what its
/// profile says, the hub keeps as observations. The first GitHub account the hub has a record of becomes the default, in the same
/// transaction.
pub(in super::super) fn connect_login(
    tx: &Transaction<'_>,
    connecting: &GithubConnecting<'_>,
) -> rusqlite::Result<GithubConnected> {
    let GithubConnecting { account: id, user, how, file, connected_at_ms, ends_at_ms, installation } = *connecting;
    let first = !tx.query_row("SELECT EXISTS (SELECT 1 FROM github_accounts)", [], OneValue::read::<bool>)?;
    let replaced = row_of(tx, id)?.and_then(|(_, file)| file);
    tx.execute(
        "INSERT INTO github_accounts
             (id, user_id, login_kind, login_file, login_connected_at_ms, login_ends_at_ms, login_needs_sign_in,
              app_id, installation_id, installed_on_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9)
             ON CONFLICT (id) DO UPDATE
             SET user_id = excluded.user_id, login_kind = excluded.login_kind,
                 login_file = excluded.login_file, login_connected_at_ms = excluded.login_connected_at_ms,
                 login_ends_at_ms = excluded.login_ends_at_ms, login_needs_sign_in = 0, app_id = excluded.app_id,
                 installation_id = excluded.installation_id, installed_on_id = excluded.installed_on_id",
        params![
            id.as_str(),
            user.id.get(),
            text_of(&how),
            file.as_str(),
            connected_at_ms,
            ends_at_ms,
            installation.map(|installation| installation.app.get()),
            installation.map(|installation| installation.id.get()),
            installation.map(|installation| installation.on.id.get()),
        ],
    )?;
    if first {
        tx.execute("UPDATE hub_settings SET github_account_id = ?", [id.as_str()])?;
    }
    let settings = if first { Some(hub_settings(tx)?) } else { None };
    let (account, _) = row_of(tx, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    Ok(GithubConnected { account, replaced, settings })
}

/// The account the hub holds a sign-in with GitHub for as the GitHub user `user`, the oldest if there were several, in
/// `tx`, whose login a sign-in as that user renews.
pub(in super::super) fn signed_in_as(
    tx: &Transaction<'_>,
    user: &GithubUser,
) -> rusqlite::Result<Option<GithubAccountId>> {
    let found = tx.query_row(
        "SELECT id FROM github_accounts WHERE user_id = ? AND login_kind = ? AND login_file IS NOT NULL
             ORDER BY rowid LIMIT 1",
        params![user.id.get(), text_of(&GithubLoginKind::SignIn)],
        OneValue::read::<String>,
    );
    Ok(found.optional()?.map(GithubAccountId::from))
}

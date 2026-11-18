use super::super::hub_settings::hub_settings;
use super::super::{Store, StoreError, from_text, lock, rev_of, text_of};
use super::profiles::keep_profile;
use crate::login_files::LoginFile;
use github::GithubProfile;
use protocol::model::{
    GithubAccount, GithubAccountId, GithubAppId, GithubInstallation, GithubInstallationId, GithubLoginKind, GithubUser,
    GithubUserId, HeldGithubLogin, HubSettings,
};
use rusqlite::{Connection, OptionalExtension as _, Row, Transaction, params};

/// The columns every read of a GitHub account's row takes, in this order, as [`account_at`] reads them.
pub(super) const COLUMNS: &str = "id, user_id, username, login_kind, login_file, login_connected_at_ms, \
                                  login_ends_at_ms, rev, login_needs_sign_in, app_id, installation_id, \
                                  installed_on_id, installed_on FROM github_accounts";

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
    /// What GitHub's profile of `user` said as the hub asked whose the login is, at `connected_at_ms`, and `None` for
    /// an app's installation, whose bot has none.
    pub(crate) profile: Option<&'a GithubProfile>,
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
/// as the hub starts.
pub(super) fn account_at(row: &Row<'_>) -> rusqlite::Result<(GithubAccount, Option<LoginFile>)> {
    let file = row.get::<_, Option<String>>(4)?.as_deref().and_then(LoginFile::parse);
    let held = match (file, row.get::<_, Option<String>>(3)?, row.get::<_, Option<i64>>(5)?) {
        (Some(file), Some(how), Some(connected_at_ms)) => {
            let held = HeldGithubLogin {
                how: from_text(how)?,
                connected_at_ms,
                ends_at_ms: row.get(6)?,
                needs_sign_in: row.get(8)?,
            };
            Some((file, held))
        }
        _ => None,
    };
    let account = GithubAccount {
        id: row.get::<_, String>(0)?.into(),
        user: GithubUser { id: GithubUserId::new(row.get(1)?), name: row.get::<_, String>(2)?.into() },
        held_login: held.as_ref().map(|(_, held)| *held),
        installation: installation_at(row)?,
        rev: rev_of(row, 7)?,
    };
    Ok((account, held.map(|(file, _)| file)))
}

/// Where a row's account is installed, when it's an app's installation, as all four of its columns say.
fn installation_at(row: &Row<'_>) -> rusqlite::Result<Option<GithubInstallation>> {
    let columns = (row.get::<_, Option<u64>>(9)?, row.get(10)?, row.get(11)?, row.get::<_, Option<String>>(12)?);
    let (Some(app), Some(id), Some(on_id), Some(on)) = columns else { return Ok(None) };
    Ok(Some(GithubInstallation {
        app: GithubAppId::new(app),
        id: GithubInstallationId::new(id),
        on: GithubUser { id: GithubUserId::new(on_id), name: on.into() },
    }))
}

/// Every GitHub account, oldest first, for a list of every record.
pub(in super::super) fn github_accounts(conn: &Connection) -> rusqlite::Result<Vec<GithubAccount>> {
    let mut statement = conn.prepare_cached(&format!("SELECT {COLUMNS} ORDER BY rowid"))?;
    statement.query_map([], |row| account_at(row).map(|(account, _)| account))?.collect()
}

/// The row of `id`, as [`account_at`] reads it.
pub(super) fn row_of(
    conn: &Connection,
    id: &GithubAccountId,
) -> rusqlite::Result<Option<(GithubAccount, Option<LoginFile>)>> {
    conn.query_row(&format!("SELECT {COLUMNS} WHERE id = ?"), [id.as_str()], account_at).optional()
}

impl Store {
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

    /// Every login the hub holds for a GitHub account, by account, with its file, for finding the one that holds a
    /// token.
    pub(crate) fn held_github_logins(&self) -> Result<Vec<(GithubAccountId, LoginFile)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!("SELECT {COLUMNS} WHERE login_file IS NOT NULL"))?;
        let rows = statement.query_map([], account_at)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter_map(|(account, file)| Some((account.id, file?))).collect())
    }

    /// The file of every login a row names, by account, for the hub to make the files and the rows agree as it starts.
    /// `None` for a row that names one but doesn't hold it, as [`account_at`] says, which tidying clears.
    pub(crate) fn github_login_files(&self) -> Result<Vec<(GithubAccountId, Option<LoginFile>)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!("SELECT {COLUMNS} WHERE login_file IS NOT NULL"))?;
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

    /// Every sign-in with GitHub the hub holds as an account's login, oldest first, with the account's record and the
    /// login's file, for renewing each before its token ends.
    pub(crate) fn held_github_sign_ins(&self) -> Result<Vec<(GithubAccount, LoginFile)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {COLUMNS} WHERE login_kind = ? AND login_file IS NOT NULL ORDER BY rowid"
        ))?;
        let rows = statement.query_map([text_of(&GithubLoginKind::SignIn)], account_at)?;
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
}

/// Has `connecting.account` hold the login whose token is in `connecting.file`, as `connecting.user`, which needs no
/// signing in again, in `tx`, and makes the account's record if it has none, and keeps what GitHub's profile of the
/// user said, if it was asked. The first GitHub account the hub has a record of becomes the default, in the same
/// transaction.
pub(in super::super) fn connect_login(
    tx: &Transaction<'_>,
    connecting: &GithubConnecting<'_>,
) -> rusqlite::Result<GithubConnected> {
    let GithubConnecting { account: id, user, how, file, connected_at_ms, ends_at_ms, installation, profile } =
        *connecting;
    let first = !tx.query_row("SELECT EXISTS (SELECT 1 FROM github_accounts)", [], |row| row.get::<_, bool>(0))?;
    let replaced = row_of(tx, id)?.and_then(|(_, file)| file);
    tx.execute(
        "INSERT INTO github_accounts
             (id, user_id, username, login_kind, login_file, login_connected_at_ms, login_ends_at_ms,
              login_needs_sign_in, app_id, installation_id, installed_on_id, installed_on)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10, ?11)
             ON CONFLICT (id) DO UPDATE
             SET user_id = excluded.user_id, username = excluded.username, login_kind = excluded.login_kind,
                 login_file = excluded.login_file, login_connected_at_ms = excluded.login_connected_at_ms,
                 login_ends_at_ms = excluded.login_ends_at_ms, login_needs_sign_in = 0, app_id = excluded.app_id,
                 installation_id = excluded.installation_id, installed_on_id = excluded.installed_on_id,
                 installed_on = excluded.installed_on",
        params![
            id.as_str(),
            user.id.get(),
            user.name.as_str(),
            text_of(&how),
            file.as_str(),
            connected_at_ms,
            ends_at_ms,
            installation.map(|installation| installation.app.get()),
            installation.map(|installation| installation.id.get()),
            installation.map(|installation| installation.on.id.get()),
            installation.map(|installation| installation.on.name.as_str()),
        ],
    )?;
    if let Some(profile) = profile {
        keep_profile(tx, id, profile, connected_at_ms)?;
    }
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
        |row| row.get::<_, String>(0),
    );
    Ok(found.optional()?.map(GithubAccountId::from))
}

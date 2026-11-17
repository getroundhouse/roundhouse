use protocol::model::{GithubAccount, GithubAccountId, GithubAppId, GithubInstallation, GithubLoginKind, GithubUserId};
use rusqlite::{OptionalExtension as _, params};

use super::super::{GithubConnected, GithubConnecting, Store, StoreError, lock, text_of};
use super::rows::{COLUMNS, account_at, connect_login};
use crate::login_files::LoginFile;

impl Store {
    /// Every GitHub account that's an installation of `app`, oldest first, with the file of the login it holds, if it
    /// holds one.
    pub(crate) fn installation_accounts(
        &self,
        app: GithubAppId,
    ) -> Result<Vec<(GithubAccount, Option<LoginFile>)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!("SELECT {COLUMNS} WHERE app_id = ? ORDER BY rowid"))?;
        Ok(statement.query_map([app.get()], account_at)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Every installation of an app the hub holds a login for, oldest first, with the account's record and the
    /// login's file, for making each a token before its last ends.
    pub(crate) fn held_github_installations(&self) -> Result<Vec<(GithubAccount, LoginFile)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {COLUMNS} WHERE login_kind = ? AND login_file IS NOT NULL AND app_id IS NOT NULL ORDER BY rowid"
        ))?;
        let rows = statement.query_map([text_of(&GithubLoginKind::App)], account_at)?;
        let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter_map(|(account, file)| Some((account, file?))).collect())
    }

    /// Has the account of `installation`'s app on the same user or organization hold the login in `connecting.file`,
    /// as `installation`, or else `connecting.account`, a new account, as [`connect_login`] says, so an app installed
    /// again where it was removed is the same account again. Its commit is on disk before this returns, as connecting
    /// a token's is.
    pub(crate) fn connect_github_installation(
        &self,
        connecting: &GithubConnecting<'_>,
        installation: &GithubInstallation,
    ) -> Result<GithubConnected, StoreError> {
        self.lasting(|tx| {
            let found = tx.query_row(
                "SELECT id FROM github_accounts WHERE app_id = ? AND installed_on_id = ? ORDER BY rowid LIMIT 1",
                params![installation.app.get(), installation.on.id.get()],
                |row| row.get::<_, String>(0),
            );
            let account = found.optional()?.map_or_else(|| connecting.account.clone(), GithubAccountId::from);
            connect_login(tx, &GithubConnecting { account: &account, installation: Some(installation), ..*connecting })
        })
    }

    /// The account that's an installation of `app` on the user or organization `on`, if there is one, as
    /// [`Store::connect_github_installation`] would keep a login for it as.
    pub(crate) fn installed_on(&self, app: GithubAppId, on: GithubUserId) -> Result<Option<GithubAccount>, StoreError> {
        let conn = lock(&self.reader);
        let found = conn.query_row(
            &format!("SELECT {COLUMNS} WHERE app_id = ? AND installed_on_id = ? ORDER BY rowid LIMIT 1"),
            params![app.get(), on.get()],
            account_at,
        );
        Ok(found.optional()?.map(|(account, _)| account))
    }
}

use super::super::{Store, StoreError, from_text, json_at, lock, rev_of, text_of};
use protocol::model::{
    AccountDefault, AccountId, AgentKind, GithubAccountId, GithubSettings, HubId, HubSettings, HubSettingsChange,
    SentCredential,
};
use rusqlite::{Connection, params};

pub(in super::super) fn hub_settings(conn: &Connection) -> rusqlite::Result<HubSettings> {
    let columns = "keep_awake, allow_asked_in_full_access, rev, github_account_id, github_sent";
    conn.query_row(&format!("SELECT {columns} FROM hub_settings"), [], |row| {
        // The outer SELECT remains active while the defaults are read, keeping one SQLite snapshot.
        let mut defaults =
            conn.prepare_cached("SELECT agent, account_id, automatic FROM account_defaults ORDER BY agent")?;
        let account_defaults = defaults
            .query_map([], |row| {
                Ok(AccountDefault {
                    agent: from_text(row.get(0)?)?,
                    account: row.get::<_, String>(1)?.into(),
                    automatic: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(HubSettings {
            account_defaults: Some(account_defaults),
            keep_awake: row.get(0)?,
            allow_asked_in_full_access: Some(row.get(1)?),
            github: Some(GithubSettings {
                account: row.get::<_, Option<String>>(3)?.map(GithubAccountId::from),
                sent: json_at(row, 4)?,
            }),
            rev: rev_of(row, 2)?,
        })
    })
}

impl Store {
    /// Called with the logins lock held, after checking the account and its token. It ends Automatic for the kind.
    pub(crate) fn set_account_default(&self, agent: AgentKind, account: &AccountId) -> Result<HubSettings, StoreError> {
        self.lasting(|tx| {
            tx.execute(
                "INSERT INTO account_defaults (agent, account_id) VALUES (?1, ?2)
                ON CONFLICT (agent) DO UPDATE SET account_id = excluded.account_id, automatic = 0
                WHERE account_id != excluded.account_id OR automatic",
                params![text_of(&agent), account.as_str()],
            )?;
            hub_settings(tx)
        })
    }

    /// Puts `agent`'s chats on accounts automatically, keeping the account chosen for it, and answers with the settings,
    /// or `None` when no account has been chosen for it, as before any login for it was added. Called with the logins
    /// lock held.
    pub(crate) fn set_automatic(&self, agent: AgentKind) -> Result<Option<HubSettings>, StoreError> {
        self.lasting(|tx| {
            let chosen = tx.execute(
                "UPDATE account_defaults SET automatic = 1 WHERE agent = ? AND NOT automatic",
                [text_of(&agent)],
            )? > 0
                || tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM account_defaults WHERE agent = ?)",
                    [text_of(&agent)],
                    |row| row.get::<_, bool>(0),
                )?;
            chosen.then(|| hub_settings(tx)).transpose()
        })
    }

    /// Makes `account` the default GitHub account, or each machine's own login with `None`. Called with the GitHub
    /// logins' lock held, after checking the account and its login. Only a default that ends up different gives the
    /// settings a newer revision.
    pub(crate) fn set_github_default(&self, account: Option<&GithubAccountId>) -> Result<HubSettings, StoreError> {
        self.lasting(|tx| {
            tx.execute(
                "UPDATE hub_settings SET github_account_id = ?1 WHERE github_account_id IS NOT ?1",
                [account.map(GithubAccountId::as_str)],
            )?;
            hub_settings(tx)
        })
    }

    /// Records `sent` as what the hub sends every worker for the default GitHub account, and answers with the settings
    /// when that changed them, which moves their revision, so a renewal that changes only the token changes nothing.
    /// Called with the GitHub logins' lock held, as what's sent is worked out. Not synced, since the hub works it out
    /// and records it again as it starts, before any client reads it.
    pub(crate) fn record_github_sent(&self, sent: &SentCredential) -> Result<Option<HubSettings>, StoreError> {
        let sent = serde_json::to_string(sent)?;
        let conn = lock(&self.writer);
        let changed = conn.execute("UPDATE hub_settings SET github_sent = ?1 WHERE github_sent IS NOT ?1", [&sent])?;
        Ok(if changed == 0 { None } else { Some(hub_settings(&conn)?) })
    }

    /// This hub's ID, made when its database was.
    pub fn hub_id(&self) -> Result<HubId, StoreError> {
        let id: String = lock(&self.reader).query_row("SELECT hub_id FROM hub_identity", [], |row| row.get(0))?;
        Ok(id.as_str().into())
    }

    pub fn hub_settings(&self) -> Result<HubSettings, StoreError> {
        Ok(hub_settings(&lock(&self.reader))?)
    }

    /// Changes the settings `change` names. Only a setting that ends up different gives the record a newer revision,
    /// so setting the same again says nothing new.
    pub fn change_hub_settings(&self, change: &HubSettingsChange) -> Result<HubSettings, StoreError> {
        let conn = lock(&self.writer);
        if let Some(keep_awake) = change.keep_awake {
            conn.execute("UPDATE hub_settings SET keep_awake = ?1 WHERE keep_awake != ?1", params![keep_awake])?;
        }
        if let Some(allow) = change.allow_asked_in_full_access {
            let sql = "UPDATE hub_settings SET allow_asked_in_full_access = ?1 WHERE allow_asked_in_full_access != ?1";
            conn.execute(sql, params![allow])?;
        }
        Ok(hub_settings(&conn)?)
    }
}

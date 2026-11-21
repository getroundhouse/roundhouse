use super::super::{ByColumn, Columns, OneValue, Store, StoreError, columns, json_at, lock, rev_of, text_at, text_of};
use protocol::model::{
    AccountDefault, AccountId, AgentKind, GithubAccountId, GithubSettings, HubId, HubSettings, HubSettingsChange,
    ProviderName, SentCredential,
};
use rusqlite::{Connection, params};

columns! {
    /// What the hub's settings read of their row.
    struct SettingsColumns {
        keep_awake, allow_asked_in_full_access, rev, github_account_id, github_sent, github_left_to_machine,
        github_set_up,
    }
}

columns! {
    /// What the hub's settings read of each kind's default account.
    struct DefaultColumns { agent, account_id, automatic }
}

pub(in super::super) fn hub_settings(conn: &Connection) -> rusqlite::Result<HubSettings> {
    let c = SettingsColumns::at(0);
    conn.query_row(&format!("SELECT {} FROM hub_settings", SettingsColumns::LIST), [], |row| {
        // The outer SELECT remains active while the defaults are read, keeping one SQLite snapshot.
        let d = DefaultColumns::at(0);
        let mut defaults =
            conn.prepare_cached(&format!("SELECT {} FROM account_defaults ORDER BY agent", DefaultColumns::LIST))?;
        let account_defaults = defaults
            .query_map([], |row| {
                Ok(AccountDefault {
                    agent: text_at(row, d.agent)?,
                    account: row.at::<String>(d.account_id)?.into(),
                    automatic: row.at(d.automatic)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(HubSettings {
            account_defaults: Some(account_defaults),
            keep_awake: row.at(c.keep_awake)?,
            allow_asked_in_full_access: Some(row.at(c.allow_asked_in_full_access)?),
            github: Some(GithubSettings {
                account: row.at::<Option<String>>(c.github_account_id)?.map(GithubAccountId::from),
                sent: json_at(row, c.github_sent)?,
                left_to_machine: json_at(row, c.github_left_to_machine)?.unwrap_or_default(),
                set_up: json_at(row, c.github_set_up)?.unwrap_or_default(),
            }),
            rev: rev_of(row, c.rev)?,
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
                    OneValue::read,
                )?;
            chosen.then(|| hub_settings(tx)).transpose()
        })
    }

    /// Makes `account` the default GitHub account, or none with `None`. Called with the GitHub
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

    /// Records `left` as the providers whose machines Roundhouse leaves GitHub to, in order of name, and `set_up` as
    /// the ones it sets up GitHub on, in the order given, on the hub's settings, and returns the settings when that
    /// changed them.
    pub(crate) fn record_github_providers(
        &self,
        left: &[ProviderName],
        set_up: &[ProviderName],
    ) -> Result<Option<HubSettings>, StoreError> {
        let mut left = left.to_vec();
        left.sort();
        let (left, set_up) = (serde_json::to_string(&left)?, serde_json::to_string(set_up)?);
        let conn = lock(&self.writer);
        let sql = "UPDATE hub_settings SET github_left_to_machine = ?1, github_set_up = ?2
                   WHERE github_left_to_machine IS NOT ?1 OR github_set_up IS NOT ?2";
        let changed = conn.execute(sql, [&left, &set_up])?;
        Ok(if changed == 0 { None } else { Some(hub_settings(&conn)?) })
    }

    /// This hub's ID, made when its database was.
    pub fn hub_id(&self) -> Result<HubId, StoreError> {
        let id: String = lock(&self.reader).query_row("SELECT hub_id FROM hub_identity", [], OneValue::read)?;
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

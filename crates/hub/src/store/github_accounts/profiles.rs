//! What GitHub's profile of each GitHub account's user said as the hub last asked, which the account's commit identity
//! is made from. Only the hub reads it, for what it sends workers, so it's on no record, and keeping it moves no
//! revision but for a new username, which is on the account's.

use protocol::model::{GithubAccount, GithubAccountId, GithubLoginKind, GithubUser};
use rusqlite::{Connection, OptionalExtension as _, params};

use super::super::{Store, StoreError, lock, text_of};
use super::rows::{COLUMNS, account_at, row_of};
use crate::login_files::LoginFile;

/// What GitHub's profile of a user says that its commit identity is made from, as `GET /user` gives it: the name the
/// user goes by and the email it shows everyone, either of which GitHub gives as none, as for a user who hasn't set a
/// name or keeps every email private.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GithubProfile {
    pub(crate) name: Option<String>,
    pub(crate) email: Option<String>,
}

/// Keeps `profile` as what GitHub said of `account`'s user when the hub asked at `asked_at_ms`, in `conn`, unless what's
/// kept was asked after, as when a check's answer comes once the account has been connected again. Answers whether it
/// kept it.
pub(super) fn keep_profile(
    conn: &Connection,
    account: &GithubAccountId,
    profile: &GithubProfile,
    asked_at_ms: i64,
) -> rusqlite::Result<bool> {
    let kept = conn.execute(
        "INSERT INTO github_profiles (account_id, name, email, read_at_ms) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (account_id) DO UPDATE
             SET name = excluded.name, email = excluded.email, read_at_ms = excluded.read_at_ms
             WHERE github_profiles.read_at_ms <= excluded.read_at_ms",
        params![account.as_str(), profile.name, profile.email, asked_at_ms],
    )?;
    Ok(kept > 0)
}

impl Store {
    /// What GitHub's profile of `id`'s user said as the hub last asked, or `None` when it hasn't asked, as for an
    /// account from before it did.
    pub(crate) fn github_profile(&self, id: &GithubAccountId) -> Result<Option<GithubProfile>, StoreError> {
        let found = lock(&self.reader).query_row(
            "SELECT name, email FROM github_profiles WHERE account_id = ?",
            [id.as_str()],
            |row| Ok(GithubProfile { name: row.get(0)?, email: row.get(1)? }),
        );
        Ok(found.optional()?)
    }

    /// Every GitHub account the hub holds a token's or a sign-in's login for whose user's profile it last asked GitHub
    /// for at `last_asked_by_ms` or before, or never, oldest first, with the login's file. An app's installation has
    /// none, since its bot's commit identity is made from its username alone.
    pub(crate) fn github_profiles_due(
        &self,
        last_asked_by_ms: i64,
    ) -> Result<Vec<(GithubAccount, LoginFile)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {COLUMNS} WHERE login_file IS NOT NULL AND login_kind IN (?1, ?2) AND NOT EXISTS (
                 SELECT 1 FROM github_profiles WHERE account_id = github_accounts.id AND read_at_ms > ?3
             ) ORDER BY rowid"
        ))?;
        let (token, sign_in) = (text_of(&GithubLoginKind::Token), text_of(&GithubLoginKind::SignIn));
        let rows = statement.query_map(params![token, sign_in, last_asked_by_ms], account_at)?;
        let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter_map(|(account, file)| Some((account, file?))).collect())
    }

    /// Keeps what GitHub said of `id`'s user when the hub asked at `asked_at_ms`, its `profile` and its username, as
    /// `user` has it, unless what's kept was asked after. Answers with the account's record when that changed it, as a
    /// username its user has changed on GitHub does. Not synced, since a profile a power cut takes back is read again
    /// at the next check.
    pub(crate) fn read_github_profile(
        &self,
        id: &GithubAccountId,
        user: &GithubUser,
        profile: &GithubProfile,
        asked_at_ms: i64,
    ) -> Result<Option<GithubAccount>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        if row_of(&tx, id)?.is_none() || !keep_profile(&tx, id, profile, asked_at_ms)? {
            return Ok(None);
        }
        let renamed = tx.execute(
            "UPDATE github_accounts SET username = ?1 WHERE id = ?2 AND username <> ?1",
            params![user.name.as_str(), id.as_str()],
        )?;
        let changed = if renamed > 0 { row_of(&tx, id)?.map(|(account, _)| account) } else { None };
        tx.commit()?;
        Ok(changed)
    }

    /// Says the hub asked GitHub for `id`'s user's profile at `asked_at_ms`, and GitHub didn't take the account's
    /// token, as one revoked, so it's asked again only once the next read is due, rather than at every check. What
    /// GitHub said before is kept, if it said anything.
    pub(crate) fn github_profile_refused(&self, id: &GithubAccountId, asked_at_ms: i64) -> Result<(), StoreError> {
        lock(&self.writer).execute(
            "INSERT INTO github_profiles (account_id, read_at_ms) SELECT id, ?2 FROM github_accounts WHERE id = ?1
                 ON CONFLICT (account_id) DO UPDATE SET read_at_ms = excluded.read_at_ms
                 WHERE github_profiles.read_at_ms <= excluded.read_at_ms",
            params![id.as_str(), asked_at_ms],
        )?;
        Ok(())
    }
}

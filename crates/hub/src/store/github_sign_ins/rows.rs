use super::super::github_accounts::{connect_login, signed_in_as};
use super::super::{GithubConnected, GithubConnecting, Store, StoreError, current_rev, json_at, lock, rev_of};
use crate::login_files::LoginFile;
use protocol::model::{
    GithubAccountId, GithubLoginKind, GithubSignIn, GithubSignInId, GithubSignInStatus, GithubUser, Rev,
};
use rusqlite::{Connection, OptionalExtension as _, Row, params};

/// The columns every read of a sign-in's row takes, in this order, as [`sign_in_at`] reads them.
const COLUMNS: &str = "id, status, rev FROM github_sign_ins";

/// What starting a sign-in did.
#[derive(Debug)]
pub(crate) struct StartedSignIn {
    /// Its record, as starting it left it, or as it was when one of its ID was started already.
    pub(crate) sign_in: GithubSignIn,
    /// Whether this started it, rather than finding one of its ID, as when a client sends its start again meanwhile.
    pub(crate) made: bool,
    /// Each sign-in that had ended, which starting it removed, with the revision of its removal.
    pub(crate) removed: Vec<(GithubSignInId, Rev)>,
}

/// A sign-in the person approved, whose login is to be kept as an account's.
#[derive(Clone, Copy)]
pub(crate) struct GithubApproving<'a> {
    pub(crate) sign_in: &'a GithubSignInId,
    /// Whose GitHub says its token is.
    pub(crate) user: &'a GithubUser,
    pub(crate) file: &'a LoginFile,
    /// The ID of the account it makes, when the hub holds no sign-in as `user`.
    pub(crate) new_account: &'a GithubAccountId,
    pub(crate) signed_in_at_ms: i64,
    /// When its token ends, by the hub's clock.
    pub(crate) ends_at_ms: Option<i64>,
}

/// The account a sign-in made or renewed, as connecting a token's says, and the sign-in's record, approved.
pub(crate) struct GithubApproved {
    pub(crate) connected: GithubConnected,
    pub(crate) sign_in: GithubSignIn,
}

/// A row's record. A status this build can't read, which only a newer build's row could hold, is `Unknown`.
fn sign_in_at(row: &Row<'_>) -> rusqlite::Result<GithubSignIn> {
    Ok(GithubSignIn {
        id: row.get::<_, String>(0)?.into(),
        status: json_at(row, 1)?.unwrap_or(GithubSignInStatus::Unknown),
        rev: rev_of(row, 2)?,
    })
}

/// Every sign-in, oldest first, for a list of every record.
pub(in super::super) fn github_sign_ins(conn: &Connection) -> rusqlite::Result<Vec<GithubSignIn>> {
    let mut statement = conn.prepare_cached(&format!("SELECT {COLUMNS} ORDER BY rowid"))?;
    statement.query_map([], sign_in_at)?.collect()
}

fn row_of(conn: &Connection, id: &GithubSignInId) -> rusqlite::Result<Option<GithubSignIn>> {
    conn.query_row(&format!("SELECT {COLUMNS} WHERE id = ?"), [id.as_str()], sign_in_at).optional()
}

fn set_status(conn: &Connection, id: &GithubSignInId, status: &str) -> rusqlite::Result<GithubSignIn> {
    conn.execute("UPDATE github_sign_ins SET status = ? WHERE id = ?", params![status, id.as_str()])?;
    row_of(conn, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

impl Store {
    pub(crate) fn github_sign_in(&self, id: &GithubSignInId) -> Result<Option<GithubSignIn>, StoreError> {
        Ok(row_of(&lock(&self.reader), id)?)
    }

    /// Records the sign-in `id` as `waiting`, and removes every sign-in that has ended, in one transaction, unless one
    /// of its ID is recorded already, which this leaves, with the rest, as they were.
    pub(crate) fn start_github_sign_in(
        &self,
        id: &GithubSignInId,
        waiting: &GithubSignInStatus,
    ) -> Result<StartedSignIn, StoreError> {
        let status = serde_json::to_string(waiting)?;
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        if let Some(sign_in) = row_of(&tx, id)? {
            return Ok(StartedSignIn { sign_in, made: false, removed: Vec::new() });
        }
        let mut removed = Vec::new();
        for ended in github_sign_ins(&tx)?.into_iter().filter(|sign_in| !sign_in.status.is_waiting()) {
            tx.execute("DELETE FROM github_sign_ins WHERE id = ?", [ended.id.as_str()])?;
            removed.push((ended.id, current_rev(&tx)?));
        }
        tx.execute("INSERT INTO github_sign_ins (id, status) VALUES (?, ?)", params![id.as_str(), status])?;
        let sign_in = row_of(&tx, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        tx.commit()?;
        Ok(StartedSignIn { sign_in, made: true, removed })
    }

    /// Records where the sign-in `id` has got to. Refused when there's no such sign-in.
    pub(crate) fn set_github_sign_in(
        &self,
        id: &GithubSignInId,
        status: &GithubSignInStatus,
    ) -> Result<GithubSignIn, StoreError> {
        let status = serde_json::to_string(status)?;
        let conn = lock(&self.writer);
        match set_status(&conn, id, &status) {
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(StoreError::NotFound { kind: "sign-in with GitHub", id: id.to_string() })
            }
            set => Ok(set?),
        }
    }

    /// Has the account the hub holds a sign-in for as `approving.user` hold the login in `approving.file` in place of
    /// the one it held, or else a new account hold it, as connecting a token does, and records the sign-in as approved
    /// as that account, in one transaction. Its commit is on disk before this returns, as connecting a token's is.
    pub(crate) fn approve_github_sign_in(&self, approving: &GithubApproving<'_>) -> Result<GithubApproved, StoreError> {
        let GithubApproving { sign_in, user, file, new_account, signed_in_at_ms, ends_at_ms } = *approving;
        self.lasting(|tx| {
            let account = signed_in_as(tx, user)?.unwrap_or_else(|| new_account.clone());
            let how = GithubLoginKind::SignIn;
            let connected_at_ms = signed_in_at_ms;
            let connecting = GithubConnecting {
                account: &account,
                user,
                how,
                file,
                connected_at_ms,
                ends_at_ms,
                installation: None,
            };
            let connected = connect_login(tx, &connecting)?;
            let approved = GithubSignInStatus::Approved { account };
            let status =
                serde_json::to_string(&approved).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            let sign_in = set_status(tx, sign_in, &status)?;
            Ok(GithubApproved { connected, sign_in })
        })
    }

    /// Records every sign-in the last hub left waiting as ended, as the hub starts, since none can be asked after: what
    /// GitHub is asked with was only in that hub's memory. One that had ended stays as it was.
    pub(crate) fn end_waiting_github_sign_ins(&self) -> Result<(), StoreError> {
        let ended = serde_json::to_string(&GithubSignInStatus::Ended)?;
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        for waiting in github_sign_ins(&tx)?.into_iter().filter(|sign_in| sign_in.status.is_waiting()) {
            set_status(&tx, &waiting.id, &ended)?;
        }
        tx.commit()?;
        Ok(())
    }
}

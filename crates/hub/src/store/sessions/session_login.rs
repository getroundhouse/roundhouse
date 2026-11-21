//! Which login a chat's agent launches under, as its record's account source and account say together.

use protocol::model::{AccountId, AccountSource, Session, SessionId};
use rusqlite::{Connection, OptionalExtension as _};

use super::super::{ByColumn, Columns, OneValue, columns, maybe_text_at};

/// Which login a chat's launches, resumes and title calls take, as [`Session::account_source`] and the account its
/// record names say together. The record keeps the two apart, as the protocol does, and this is where the hub reads
/// them as one, as [`Session::hub_account`] does for a client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SessionLogin {
    /// None yet, so the chat's next launch takes its kind's default: the hub's login for an added default, or the
    /// ambient one while its kind has none. A record that says the hub's but names no account is this too, so that
    /// launch fills it in.
    NoneYet,
    /// The ambient login, whatever the workspace's machine has, so no launch is handed one, whatever the default is.
    Ambient,
    /// The login the hub holds for this account, which every launch is handed, or can't launch without.
    Hub(AccountId),
}

impl SessionLogin {
    /// What a record's account source and account say together.
    pub(crate) fn of(source: Option<AccountSource>, account: Option<&AccountId>) -> SessionLogin {
        match (source, account) {
            (Some(AccountSource::Ambient), _) => SessionLogin::Ambient,
            (Some(AccountSource::Hub), Some(account)) => SessionLogin::Hub(account.clone()),
            // Only a newer hub could write a source this one doesn't know, and this one refuses a newer hub's database,
            // so it never reads one.
            (None | Some(AccountSource::Hub | AccountSource::Unknown), _) => SessionLogin::NoneYet,
        }
    }

    /// What `session`'s record says.
    pub(crate) fn of_record(session: &Session) -> SessionLogin {
        SessionLogin::of(session.account_source, session.account.as_ref())
    }
}

/// Whether `session`'s row says its login is the hub's, whether it names the account yet or not. What such a record
/// names is the account the session is on, which only a move changes, so filing what a run under another login is
/// under, such as one still running from before the move, leaves it. `false` without a row.
pub(in super::super) fn on_the_hubs(conn: &Connection, session: &SessionId) -> rusqlite::Result<bool> {
    let says =
        conn.query_row("SELECT account_source IS 'hub' FROM sessions WHERE id = ?", [session.as_str()], OneValue::read);
    Ok(says.optional()?.unwrap_or(false))
}

/// What `session`'s row says, or `None` without one.
pub(in super::super) fn session_login_of(
    conn: &Connection,
    session: &SessionId,
) -> rusqlite::Result<Option<SessionLogin>> {
    columns! {
        struct LoginColumns { account_source, account_id }
    }
    let c = LoginColumns::at(0);
    conn.query_row(&format!("SELECT {} FROM sessions WHERE id = ?", LoginColumns::LIST), [session.as_str()], |row| {
        let source = maybe_text_at(row, c.account_source)?;
        let account = row.at::<Option<String>>(c.account_id)?.map(AccountId::from);
        Ok(SessionLogin::of(source, account.as_ref()))
    })
    .optional()
}

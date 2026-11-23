//! What the app calls what it names in more than one place, so each is called the same everywhere: who an account is,
//! on a chat's card of its limits, on the context's card when there are no limits, in the menu that moves a chat to
//! another, and in Agents in Settings, and what this machine is, where a workspace's card says where its worker runs
//! and where Agents says which machines the hub hands a login to.

use client_core::records::Snapshot;
use protocol::model::Account;

/// What this machine is, after "this": a Mac, or a computer on any other system.
pub(crate) const MACHINE: &str = if cfg!(target_os = "macos") { "Mac" } else { "computer" };

/// What this machine is called as a heading or picker choice: "This Mac" on a Mac, and "This computer" elsewhere.
pub(crate) const THIS_MACHINE: &str = if cfg!(target_os = "macos") { "This Mac" } else { "This computer" };

/// Who `account` is, as `records`, the copy it's from, reads it: the name the person gave it, for one whose login they
/// gave the hub, since a token doesn't say whose it is, and otherwise its email. Either goes by the other when it's all
/// that's known, and `None` when neither is.
pub(crate) fn account(records: &Snapshot, account: &Account) -> Option<String> {
    let email = records.account_identity(account).email;
    let (first, then) = match account.login_agent {
        Some(_) => (account.name.clone(), email),
        None => (email, account.name.clone()),
    };
    first.or(then)
}

/// What `account` is called where it has to be called something, as on its row in Agents and in the menu that moves a
/// chat to it: who it is, as [`account`] says, and otherwise how it's logged in, or its ID.
pub(crate) fn called(records: &Snapshot, account: &Account) -> String {
    let login = || records.account_identity(account).login;
    self::account(records, account).or_else(login).unwrap_or_else(|| account.id.to_string())
}

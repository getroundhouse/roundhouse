//! What the app calls what it names in more than one place, so each is called the same everywhere: who an account is,
//! on a chat's card of its limits, on the context's card when there are no limits, and in Accounts in Settings, and
//! what this machine is, where a workspace's card says where its worker runs and where Accounts says which machines
//! the hub hands a login to.

use protocol::model::Account;

/// What this machine is, after "this": a Mac, or a computer on any other system.
pub(crate) const MACHINE: &str = if cfg!(target_os = "macos") { "Mac" } else { "computer" };

/// Who `account` is: the name the person gave it, for one whose login they gave the hub, since a token doesn't say
/// whose it is, and otherwise its email. Either goes by the other when it's all the record says, and `None` when it
/// says neither.
pub(crate) fn account(account: &Account) -> Option<String> {
    let (first, then) = match account.login_agent {
        Some(_) => (&account.name, &account.email),
        None => (&account.email, &account.name),
    };
    first.clone().or_else(|| then.clone())
}

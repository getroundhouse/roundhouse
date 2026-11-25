//! Where the hub keeps the secret of each login it holds, such as a token: a file for each connection, named for it, in
//! a directory of the logins of one kind, which no other kind's are in, and whose names the rows of the tables of that
//! kind keep: `accounts` for agents' logins, both `github_accounts` and `github_apps` for GitHub's, an account's login
//! and an app's key, and `tool_servers` for what each tool server of the hub's own knows the person by. Only this user
//! can open them, as the database. A file's name is one the hub made, and one read back from the database is checked as
//! it becomes a path, so a row can't name a file anywhere else.
//!
//! The writes of a file and of the row that names it are in an order chosen so that whatever stops the hub, or the
//! machine, leaves either a file no row names or a row naming a file that's gone. As the hub starts, [`Locked::tidy`]
//! mends both. So a login the hub has said it holds is never lost, and a secret it was told to forget never comes
//! back. Only this module writes or removes a file, so no caller can get the order wrong.

#[cfg(test)]
mod tests;

mod locked;
mod login_file;
mod secret;

pub(crate) use locked::{Locked, LoginFiles};
pub(crate) use login_file::LoginFile;
pub(crate) use secret::{LoginSecret, Stored};

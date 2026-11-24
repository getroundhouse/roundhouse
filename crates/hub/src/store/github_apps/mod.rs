//! The apps of your own the hub holds the key of, as each app's row has them: its number, which file its key is in, and
//! what GitHub made it from. What GitHub calls it, its owner and its bot, the hub only observes, as
//! `crate::hub::observations::GithubAppSaid` says. The key is never in the database, as `crate::github_logins` says.
//! Which GitHub accounts are its installations is on theirs.

#[cfg(test)]
mod tests;

mod rows;
pub(crate) use rows::GithubAppMaking;
pub(super) use rows::github_apps;

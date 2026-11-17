//! The apps of your own the hub holds the key of, as each app's row has them: what GitHub calls it, who made it, its
//! bot, and which file its key is in. The key is never in the database, as `crate::github_logins` says. Which GitHub
//! accounts are its installations is on theirs.

#[cfg(test)]
mod tests;

mod rows;
pub(crate) use rows::GithubAppMaking;
pub(super) use rows::github_apps;

//! The sign-ins with GitHub the hub runs, as each one's row has them: where it has got to. What GitHub is asked with is
//! never here, since the hub keeps it in memory while each waits, and approving one keeps its login as a GitHub
//! account's, in the same transaction, as `crate::github_logins` says.

#[cfg(test)]
use crate::login_files::LoginFile;
#[cfg(test)]
use protocol::model::{GithubSignInId, GithubSignInStatus, GithubUser, GithubUserId};

#[cfg(test)]
mod tests;

mod rows;
pub(super) use rows::github_sign_ins;
pub(crate) use rows::{GithubApproved, GithubApproving, StartedSignIn};

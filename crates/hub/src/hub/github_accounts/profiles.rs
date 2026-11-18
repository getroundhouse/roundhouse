//! Asking GitHub again who an account the check found due is, for its commit identity. What GitHub answers is
//! the `github` crate's to read, and what's kept of it is `crate::github_logins`'s.

use std::sync::Arc;

use tracing::error;

use super::super::{Hub, off_thread};
use crate::github_logins::ProfileDue;
use crate::store::now_ms;

impl Hub {
    /// Asks GitHub who `due`'s account is, within its time limit, with the token its file held as the check found it,
    /// then has `GithubLogins::profile_read` keep what GitHub said, and tells every client when that changed the
    /// account's record, as a username changed on GitHub does.
    pub(super) async fn read_profile(&self, due: ProfileDue) {
        let asked_at_ms = now_ms();
        let read = self.github.token_owner(&due.token).await;
        let logins = Arc::clone(&self.github_logins);
        match off_thread(move || logins.profile_read(&due, read, asked_at_ms)).await {
            Ok(Some(change)) => {
                self.announce_github_account(change);
            }
            Ok(None) => {}
            Err(e) => error!("couldn't keep who GitHub says an account is: {}", e.message),
        }
    }
}

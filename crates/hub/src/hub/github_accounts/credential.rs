//! What the hub's workers give their processes for github.com, as `crate::github_logins` works it out, for each
//! worker's link to follow, and the check by the wall clock that finds the default's token has ended.

use std::sync::Arc;
use std::time::Duration;

use protocol::worker::GithubCredential;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;
use tracing::error;

use super::super::Hub;
use crate::github_logins::GithubLogins;
use crate::store::now_ms;

/// How often the hub checks, by the wall clock, whether the default GitHub account's token has ended, and as it
/// starts. Tokio's timers don't count the time a Mac sleeps, so waiting until a token's end would wait as long again
/// as the Mac slept, where checking every minute it's awake finds it within a minute of waking. Until then, a push
/// fails at GitHub, which says the token is bad, rather than with the block, which says what to do.
const TOKEN_ENDS_CHECKED_EVERY: Duration = Duration::from_secs(60);

impl Hub {
    /// What workers are to give their processes for github.com, from now on, as it changes, as
    /// [`GithubLogins::git_credential`] says.
    pub(crate) fn git_credential(&self) -> watch::Receiver<GithubCredential> {
        self.github_logins.git_credential()
    }

    /// Checks, as the hub starts and every [`TOKEN_ENDS_CHECKED_EVERY`] after, by the wall clock, whether the default's
    /// token has ended, which then has workers sent a block. Until the hub stops.
    pub(crate) async fn watch_github_token_ends(self: Arc<Self>) {
        check_token_ends(Arc::clone(&self.github_logins), TOKEN_ENDS_CHECKED_EVERY, now_ms).await;
    }
}

/// Checks `logins` at once, and every `every` after, at the time `wall_clock` says.
async fn check_token_ends(logins: Arc<GithubLogins>, every: Duration, wall_clock: impl Fn() -> i64) {
    let mut checks = tokio::time::interval(every);
    // One that comes late, as on a hub that's busy, isn't followed by others to catch up.
    checks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        checks.tick().await;
        let (logins, now_ms) = (Arc::clone(&logins), wall_clock());
        if let Err(e) = tokio::task::spawn_blocking(move || logins.check(now_ms)).await {
            error!("checking whether the default GitHub account's token has ended failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use protocol::client::github_accounts::GithubAccountConnect;
    use protocol::model::{GithubToken, GithubUser, GithubUserId};

    use super::*;
    use crate::github::TokenOwner;
    use crate::github_logins::Connect;
    use crate::store::tests::store_with_session;

    const ENDS_AT_MS: i64 = 1_000_000_000;

    /// A Mac that sleeps for the eight hours its default's token has left, by the wall clock, while tokio's clock,
    /// which doesn't count the sleep, moves only to the next check.
    #[tokio::test(start_paused = true)]
    async fn a_defaults_token_that_ends_while_the_hub_runs_blocks_at_the_next_check_by_the_wall_clock() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(store_with_session(dir.path()));
        let logins = Arc::new(GithubLogins::new(store, dir.path(), 0));
        let connect = Connect::new(GithubAccountConnect { token: GithubToken::new("a-token"), id: None }, 0).unwrap();
        let user = GithubUser { id: GithubUserId::new(7), name: "ada".into() };
        logins.connect(&connect, &TokenOwner { user, ends_at_ms: Some(ENDS_AT_MS) }).unwrap();
        let mut sent = logins.git_credential();
        let wall_clock = Arc::new(AtomicI64::new(ENDS_AT_MS - 8 * 60 * 60 * 1000));
        let every = TOKEN_ENDS_CHECKED_EVERY;
        tokio::spawn(check_token_ends(Arc::clone(&logins), every, {
            let wall_clock = Arc::clone(&wall_clock);
            move || wall_clock.load(Ordering::SeqCst)
        }));
        // Once the first check has been, and the next is waited for, which is when tokio's clock moves.
        let first = tokio::time::timeout(every / 2, sent.changed()).await;

        wall_clock.store(ENDS_AT_MS, Ordering::SeqCst);
        let before_the_next =
            tokio::time::timeout((every / 2).saturating_sub(Duration::from_millis(1)), sent.changed()).await;
        let at_the_next = tokio::time::timeout(Duration::from_millis(2), sent.changed()).await;

        assert!(first.is_err(), "the first check, as the hub starts, found it hadn't ended");
        assert!(before_the_next.is_err(), "nothing changed before the next check: {:?}", *sent.borrow());
        assert!(at_the_next.is_ok(), "the next check found it had ended");
        assert!(matches!(&*sent.borrow(), GithubCredential::Blocked { reason } if reason.contains("has ended")));
    }
}

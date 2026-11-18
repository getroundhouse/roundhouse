//! What the hub's workers give their processes for github.com, as `crate::github_logins` works it out, for each
//! worker's link to follow, and as it records it on the hub's settings, for every client to hear of, and the check by
//! the wall clock that finds the default's token has ended, and which sign-ins to renew before theirs does, until the
//! hub stops.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::HubNotice;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tracing::error;

use super::super::Hub;
use crate::github_logins::DefaultCredential;
use crate::store::now_ms;

impl Hub {
    /// What workers are to give their processes for github.com, from now on, as it changes, as
    /// [`GithubLogins::git_credential`](crate::github_logins::GithubLogins::git_credential) says.
    pub(crate) fn git_credential(&self) -> watch::Receiver<DefaultCredential> {
        self.github_logins.git_credential()
    }

    /// Tells every client of the hub's settings each time what workers are sent is recorded anew on them, as
    /// [`GithubLogins::recorded_settings`](crate::github_logins::GithubLogins::recorded_settings) says, from when this
    /// is called, so the hub calls it before it serves any client, here or paired, and no client misses a change made
    /// after the records it lists. Each change to the logins has the logins record it, whatever made it, which is why
    /// it's followed here rather than told of by each, and a client drops a copy no newer than one it has, so the
    /// settings told of by what changed the default can come before or after. Until the task it runs on is ended, as
    /// the hub stops.
    pub(crate) fn tell_of_what_workers_are_sent(self: &Arc<Self>) -> impl Future<Output = ()> + Send + 'static {
        let (hub, mut recorded) = (Arc::clone(self), self.github_logins.recorded_settings());
        async move {
            while recorded.changed().await.is_ok() {
                let settings = recorded.borrow_and_update().clone();
                if let Some(settings) = settings {
                    hub.notify(HubNotice::HubSettings(settings));
                }
            }
        }
    }

    /// Checks the GitHub logins as the hub starts and every
    /// [`check_github_logins_every`](crate::Timers::check_github_logins_every) after, by the wall clock, as
    /// [`Hub::check_github_logins`] says, each once the last has finished, until
    /// [`Hub::stop_checking_github_logins`].
    pub(crate) async fn watch_github_logins(self: Arc<Self>) {
        let stopped = stopped(self.github_checks_stopped.subscribe());
        check_by_the_wall_clock(self.timers.check_github_logins_every, now_ms, stopped, |now_ms| {
            let hub = Arc::clone(&self);
            async move { hub.check_github_logins(now_ms).await }
        })
        .await;
    }

    /// Stops checking the GitHub logins, as the hub stops, and waits for `watching`, the task
    /// [`Hub::watch_github_logins`] runs on, to end, before the hub lets go of its lock. It ends at once between checks.
    /// A check under way that hasn't started its renewals starts none, and one that has ends once they have, each
    /// within [`ASK_WITHIN`](github::ASK_WITHIN), 15 s, and then its write, which waits on nothing outside the
    /// hub. So the wait is at most the check's own work, a renewal's call and its write. GitHub revokes what a renewal
    /// renews as it gives the next, so a stop that dropped what GitHub gave would have the account need signing in
    /// again, where waiting makes a stop at most about 15 s longer. Making an installation's token, and looking for
    /// where an app of your own is installed, are dropped where they are, since GitHub revokes nothing as it makes a
    /// token, so dropping one costs only another call, at the next hub's first check.
    pub(crate) async fn stop_checking_github_logins(&self, watching: JoinHandle<()>) {
        self.github_checks_stopped.send_replace(true);
        if let Err(e) = watching.await {
            error!("checking the GitHub logins failed as the hub stopped: {e}");
        }
    }
}

/// Once `checks_stopped` says the hub has stopped checking, or the hub has gone.
pub(super) async fn stopped(mut checks_stopped: watch::Receiver<bool>) {
    // An error says the hub has gone, which stops the checks too.
    let _gone = checks_stopped.wait_for(|stopped| *stopped).await;
}

/// Has `check` check at once, and every `every` after, at the time `wall_clock` says then, each once the last has
/// finished, so no two run at once, until `stopped`, which a check under way runs to its end past. One that comes late,
/// as on a hub that's busy or after a check that took long, isn't followed by others to catch up.
pub(super) async fn check_by_the_wall_clock<F>(
    every: Duration,
    wall_clock: impl Fn() -> i64,
    stopped: impl Future<Output = ()>,
    mut check: impl FnMut(i64) -> F,
) where
    F: Future<Output = ()>,
{
    let mut checks = tokio::time::interval(every);
    checks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut stopped = std::pin::pin!(stopped);
    loop {
        tokio::select! {
            biased;
            () = &mut stopped => return,
            _ = checks.tick() => {}
        }
        check(wall_clock()).await;
    }
}

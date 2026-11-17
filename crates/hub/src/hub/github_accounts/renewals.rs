//! Checking the GitHub logins by the wall clock, renewing each sign-in the check finds due, making each installation of
//! an app of your own the check finds due a new token, looking for where each app is installed, and asking GitHub who
//! each account the check finds due is again. GitHub is asked between two turns at the logins' lock, never under it.
//! What it answers is `crate::github`'s to read, and what's kept of it, and in what order, is `crate::github_logins`'s.

use std::sync::Arc;

use tokio::task::JoinSet;
use tracing::{error, warn};

use super::super::{Hub, off_thread};
use super::credential::stopped;
use crate::github_logins::{Checked, Due, GithubAccountChange};
use crate::store::now_ms;

impl Hub {
    /// Checks the GitHub logins at `now_ms` by the wall clock, as `GithubLogins::check` says, tells every client of
    /// each account the check changed, and renews each sign-in it found due, makes each installation it found due a
    /// token, looks for where each app is installed, and asks GitHub who each account it found due is, all at once, and
    /// waits for them, each call within GitHub's time limit. Only [`Hub::watch_github_logins`] checks, once the last
    /// check has finished, so no check finds a sign-in due while it's being renewed, and none is renewed twice at once,
    /// which would send GitHub what renews its token after GitHub had revoked it. Each is a task of its own, so one
    /// that panics fails only itself. Once the hub has stopped checking, as [`Hub::stop_checking_github_logins`] says,
    /// a check starts none, and the stop waits for the renewals started before, and drops the rest where they are.
    pub(in super::super) async fn check_github_logins(self: &Arc<Self>, now_ms: i64) {
        let logins = Arc::clone(&self.github_logins);
        let Checked { due, changed, minting, apps, profiles } = match off_thread(move || logins.check(now_ms)).await {
            Ok(checked) => checked,
            Err(e) => {
                error!("couldn't check the GitHub logins: {}", e.message);
                return;
            }
        };
        for account in changed {
            self.announce_github_account(GithubAccountChange { account, changed: true, settings: None });
        }
        let (mut renewing, mut unwaited) = (JoinSet::new(), JoinSet::new());
        {
            // Held while the renewals start, so a stop either comes first, and none starts, or waits for them all.
            let stopped = self.github_checks_stopped.borrow();
            if *stopped {
                return;
            }
            for due in due {
                let hub = Arc::clone(self);
                renewing.spawn(async move { hub.renew_sign_in(due).await });
            }
            for due in minting {
                let hub = Arc::clone(self);
                unwaited.spawn(async move { hub.mint(due).await });
            }
            for app in apps {
                let hub = Arc::clone(self);
                unwaited.spawn(async move { hub.look_by_itself(app).await });
            }
            for due in profiles {
                let hub = Arc::clone(self);
                unwaited.spawn(async move { hub.read_profile(due).await });
            }
        }
        let stopping = stopped(self.github_checks_stopped.subscribe());
        tokio::join!(joined(&mut renewing, "renewing a sign-in with GitHub"), async {
            // A stop doesn't wait for these, since GitHub revokes nothing as it makes an installation's token or says
            // who a token is, so what it gave and the hub dropped costs only another call, at the next hub's first
            // check.
            let what = "making an installation's token, looking for an app's, or asking GitHub who an account is";
            tokio::select! {
                () = joined(&mut unwaited, what) => {}
                () = stopping => {}
            }
        });
    }

    /// Renews the sign-in `due`: asks GitHub, within its time limit, then has `GithubLogins::renewed` keep what GitHub
    /// gave, or that it refused what renews the token, and tells every client what that changed. One GitHub doesn't
    /// answer, or answers as may pass by the next try, is said in the log and left as it is, for the next check to
    /// renew, so a default whose token ends meanwhile blocks, saying it's being renewed.
    async fn renew_sign_in(&self, due: Due) {
        let renewal = match self.github.renew(&due.refresh).await {
            Ok(renewal) => renewal,
            Err(e) => {
                let why = e.message;
                warn!(account = %due.account, "couldn't renew a sign-in with GitHub, so the next check will: {why}");
                return;
            }
        };
        let (logins, answered_at_ms) = (Arc::clone(&self.github_logins), now_ms());
        match off_thread(move || logins.renewed(&due, renewal, answered_at_ms)).await {
            Ok(Some(change)) => {
                self.announce_github_account(change);
            }
            Ok(None) => {}
            Err(e) => error!("couldn't keep what renewing a sign-in with GitHub came to: {}", e.message),
        }
    }
}

/// Once every task in `tasks` has ended, saying in the log each that failed at `what`.
async fn joined(tasks: &mut JoinSet<()>, what: &str) {
    while let Some(ended) = tasks.join_next().await {
        if let Err(e) = ended {
            error!("{what} failed: {e}");
        }
    }
}

use std::sync::Arc;
use std::time::Duration;

use protocol::model::GithubSignInId;
use tracing::error;

use super::super::{Hub, off_thread};
use crate::github::{ASK_WITHIN, Approval, DeviceCode, Granted};
use crate::github_logins::{Approved, SignInEnd, SignedIn};
use crate::jobs::{Ended, JobKind, Limits, NotStarted, step, uncut};
use crate::store::now_ms;

/// How much longer than its code lasts a sign-in's job may run: an ask after its token, whose answer can come just as
/// the code ends, and then asking GitHub whose the token is, each within [`ASK_WITHIN`], and keeping it, which waits
/// on nothing outside the hub and runs uncut.
const BEYOND_ITS_CODE: Duration = ASK_WITHIN.saturating_mul(3);

/// How long a sign-in's job has to record how it ended, once its work was cut short, which only writes its row.
const RECORDING_WITHIN: Duration = Duration::from_secs(15);

impl Hub {
    /// Starts the job that asks GitHub after the sign-in `id`, whose code is `code`, until GitHub approves or refuses
    /// it or its code ends, and then keeps what GitHub gave, or records how it ended, and tells every client. The job
    /// ends with the sign-in, so how many may run at once is how many sign-ins can wait, and it holds `code`, which is
    /// what GitHub is asked with, so nothing else does. One called off, as cancelling does, stops asking where it is.
    /// One cut short before it kept what GitHub gave is recorded as having failed, unless it has ended meanwhile.
    /// Refused as [`Jobs::start`](crate::jobs::Jobs::start) refuses it.
    pub(super) fn wait_for_sign_in(self: &Arc<Self>, id: GithubSignInId, code: DeviceCode) -> Result<(), NotStarted> {
        let limits = Limits { work: code.lasts.saturating_add(BEYOND_ITS_CODE), after: RECORDING_WITHIN };
        let (key, ending) = (id.clone(), id.clone());
        let hub = Arc::clone(self);
        let work = async move {
            step("asking GitHub after the sign-in, until the person approves or refuses it or its code ends");
            let approval = hub.github.wait_for_approval(&code, hub.timers.sign_in_pace).await;
            let answered_at_ms = now_ms();
            drop(code);
            match approval {
                Approval::Granted(granted) => hub.keep_sign_in(id, granted, answered_at_ms).await,
                Approval::Refused => hub.end_sign_in_uncut(id, SignInEnd::Refused).await,
                Approval::Ended => hub.end_sign_in_uncut(id, SignInEnd::Ended).await,
                Approval::Failed(message) => hub.end_sign_in_uncut(id, SignInEnd::Failed(message)).await,
            }
        };
        let hub = Arc::clone(self);
        let after = move |ended: Ended| async move {
            if !ended.finished_or_ran_uncut() {
                let message = "the hub stopped waiting for GitHub before the sign-in ended".to_string();
                hub.end_sign_in_alone(ending, SignInEnd::Failed(message)).await;
            }
        };
        self.sign_ins.start(JobKind::WaitForGithubSignIn, key, limits, work, after)
    }

    /// Keeps what GitHub `granted` the sign-in `id` at `granted_at_ms`, as whoever GitHub says the token is, and tells
    /// every client, or records that it failed, as when GitHub won't say whose the token is. GitHub is asked first,
    /// and then the rest runs uncut, so a cancel meanwhile is decided under the logins' lock, as
    /// `GithubLogins::approve_sign_in` says.
    async fn keep_sign_in(self: &Arc<Self>, id: GithubSignInId, granted: Granted, granted_at_ms: i64) {
        step("asking GitHub whose the sign-in's token is");
        let owner = match self.github.token_owner(&granted.token).await {
            Ok(owner) => owner,
            Err(e) => {
                let message = format!("GitHub didn't say whose the sign-in is: {}", e.message);
                return self.end_sign_in_uncut(id, SignInEnd::Failed(message)).await;
            }
        };
        let approved = Approved::new(id.clone(), SignedIn::given(granted, granted_at_ms), granted_at_ms);
        let hub = Arc::clone(self);
        uncut(async move {
            let logins = Arc::clone(&hub.github_logins);
            match off_thread(move || logins.approve_sign_in(&approved, &owner)).await {
                Ok(change) => {
                    hub.announce_sign_in(change);
                }
                Err(e) => {
                    error!(sign_in = %id, "couldn't keep what GitHub gave a sign-in: {}", e.message);
                    let message = format!("the hub couldn't keep what GitHub gave: {}", e.message);
                    hub.end_sign_in_alone(id, SignInEnd::Failed(message)).await;
                }
            }
        })
        .await;
    }

    async fn end_sign_in_uncut(self: &Arc<Self>, id: GithubSignInId, end: SignInEnd) {
        let hub = Arc::clone(self);
        uncut(async move { hub.end_sign_in_alone(id, end).await }).await;
    }
}

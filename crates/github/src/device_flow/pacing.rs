use std::time::Duration;

use protocol::client::ApiError;
use tokio::time::Instant;
use tracing::{info, warn};

use super::super::client::within;
use super::approval::Approval;

/// How much longer GitHub asks the hub to wait between asks after `slow_down` that says no interval, as its docs give.
const SLOWER_BY: Duration = Duration::from_secs(5);

/// What one ask after a code's token came to.
#[derive(Debug)]
pub(super) enum Asked {
    /// Not yet, so ask again.
    Again,
    /// Not yet, and ask less often: between asks as long as `interval` says, or, when GitHub said none, [`SLOWER_BY`]
    /// longer than before.
    SlowDown {
        interval: Option<Duration>,
    },
    Done(Approval),
}

/// Asks with `ask` after a code's token, first and between each ask as long as `interval`, longer after GitHub says
/// `slow_down`, and never sooner than `least`, until it's done or the code `ends`, as far as tokio's clock says. Each
/// ask is within [`ASK_WITHIN`](super::super::ASK_WITHIN), and one that fails, as when GitHub can't be reached or
/// doesn't answer in time, is asked again. No ask starts once the code has ended, but one started just before can wait
/// out its limit, so it ends at the latest [`ASK_WITHIN`](super::super::ASK_WITHIN) after the code does. Dropping it
/// stops asking.
#[expect(
    clippy::disallowed_methods,
    reason = "the interval GitHub has the hub leave between asks after a sign-in's token"
)]
pub(super) async fn paced<F>(interval: Duration, least: Duration, ends: Instant, mut ask: impl FnMut() -> F) -> Approval
where
    F: Future<Output = Result<Asked, ApiError>>,
{
    let mut interval = interval.max(least);
    loop {
        let next = Instant::now() + interval;
        if next >= ends {
            return Approval::Ended;
        }
        tokio::time::sleep_until(next).await;
        match within(ask()).await {
            Ok(Asked::Done(approval)) => return approval,
            Ok(Asked::Again) => {}
            Ok(Asked::SlowDown { interval: said }) => {
                interval = said.unwrap_or(interval + SLOWER_BY).max(least);
                info!(
                    name: "github.sign_in_slowed",
                    seconds = interval.as_secs(),
                    "GitHub asked the hub to ask after a sign-in less often"
                );
            }
            Err(e) => {
                warn!(
                    name: "github.sign_in_unasked",
                    error = e.message,
                    "Couldn't ask GitHub after a sign-in's token, so it's asked again"
                );
            }
        }
    }
}

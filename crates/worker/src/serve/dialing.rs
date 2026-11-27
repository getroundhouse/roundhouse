//! A worker that dials its hub, rather than being reached by it, and dials again whenever the connection drops, so it
//! outlives a hub that restarts, as `docs/decisions/workers-dial-the-hub.md` says. How it dials, over iroh with its
//! ticket, is its caller's, so this crate needs no transport of its own. Only on Unix, as for a worker that listens,
//! since what claims an instance is there only.

use std::io;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::Instant;
use tracing::{info, warn};

use super::connection::{self, Ended};
use super::listen::without_hub;
use super::{Config, Worker};

/// The first pause before dialing again after a dial that failed, which doubles with each that fails after it, up to
/// [`LONGEST_PAUSE`].
const FIRST_PAUSE: Duration = Duration::from_millis(100);
/// Short enough that a worker whose hub has restarted dials the new one well within the 5 s a starting hub waits for
/// the workers an earlier hub left.
const LONGEST_PAUSE: Duration = Duration::from_secs(1);

/// What a dial that failed means.
pub enum DialFailed {
    /// The hub can't be reached now, which may pass, so the worker dials again.
    Unreachable(String),
    /// The hub refused this worker, such as for a ticket that's been used, so dialing again won't help.
    Refused(String),
}

/// Serves the hub on each connection `dial` opens, one at a time, until the hub asks the worker to stop, the hub
/// refuses it, it has had no hub for `idle_limit`, or `stops_with` has returned while it has none. Then it stops every agent and terminal, after sending the last
/// hub, if it's still there, what they said as they stopped. Each connection that drops without the hub asking it to
/// stop is dialed again. The caller holds the instance's claim.
#[expect(clippy::disallowed_methods, reason = "the pause between dials that failed, which grows")]
pub async fn serve_dialing<D, F, R, W>(
    mut dial: D,
    config: Config,
    idle_limit: Duration,
    stops_with: impl Future<Output = ()>,
) -> io::Result<()>
where
    D: FnMut() -> F,
    F: Future<Output = Result<(R, W), DialFailed>>,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let mut stops_with = std::pin::pin!(stops_with);
    let worker = Worker::new(config);
    let (mut alone_since, mut pause) = (Instant::now(), FIRST_PAUSE);
    let (last, refused) = loop {
        let remaining = idle_limit.saturating_sub(alone_since.elapsed());
        if remaining.is_zero() {
            info!(name: "hub.none_reached", idle_limit = ?idle_limit, "No hub has been reached in a while, so agents are stopped");
            break (None, None);
        }
        let dialed = tokio::select! {
            dialed = tokio::time::timeout(remaining, dial()) => dialed,
            // A hub that's connected stops with the same, and stops the worker as it does.
            () = &mut stops_with => {
                info!(name: "worker.stops_with_gone", "What this worker stops with has gone, so agents are stopped");
                break (None, None);
            }
        };
        match dialed {
            Ok(Ok((reader, writer))) => {
                pause = FIRST_PAUSE;
                let closing = connection::serve(&worker, reader, writer, None).await;
                if closing.ended == Ended::Stop {
                    break (Some(closing), None);
                }
                info!(name: "hub.dropped", "The connection to the hub dropped, so the worker dials it again");
                without_hub(&worker).await;
                closing.abandon();
                alone_since = Instant::now();
            }
            Ok(Err(DialFailed::Refused(why))) => break (None, Some(why)),
            Ok(Err(DialFailed::Unreachable(why))) => {
                warn!(name: "hub.unreachable", error = why, "Couldn't reach the hub, so the worker dials it again");
                tokio::time::sleep(pause).await;
                pause = (pause * 2).min(LONGEST_PAUSE);
            }
            Err(_) => {}
        }
    };
    worker.stop(last).await;
    match refused {
        Some(why) => {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("the hub refused this worker: {why}")))
        }
        None => Ok(()),
    }
}

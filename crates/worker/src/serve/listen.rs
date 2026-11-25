//! A worker that can outlive its hub. It serves one hub at a time on a Unix socket, and carries on between them when
//! the hub asked it to.

use std::io;
use std::path::Path;
use std::time::Duration;

use tokio::net::{UnixListener, UnixStream};
use tokio::task::{JoinError, JoinHandle};
use tokio::time::Instant;
use tracing::{info, warn};

use super::connection::{self, Closing, Ended};
use super::{Config, Worker};

/// Serves hubs on a socket at `socket`, one at a time, until a hub asks the worker to stop, or a hub goes that didn't
/// ask it to keep running without one, or it has had no hub for `idle_limit`, or `stops_with` has returned while it has
/// none. Then it stops every agent and terminal, after sending the last hub, if it's still there, what they said as they
/// stopped. The caller holds the instance's claim, so a socket already there is one a worker left behind.
///
/// A hub that connects while another is connected waits for that one to go, since one that crashed can be noticed to
/// have gone only after the next has connected. It's refused if that takes longer than
/// [`Timers::hub_goes_within`](crate::Timers::hub_goes_within), so a hub that asks twice for its connection learns
/// there's one already, and if another connects meanwhile, since the last to connect is the likeliest to still be
/// there. If the worker stops first, its connection closes with no reply.
#[expect(
    clippy::disallowed_methods,
    reason = "the pause after a connection that couldn't be accepted, and the time a worker with no hub waits for one"
)]
pub async fn listen(
    socket: &Path,
    config: Config,
    idle_limit: Duration,
    stops_with: impl Future<Output = ()>,
) -> io::Result<()> {
    let listener = bind(socket)?;
    let mut stops_with = std::pin::pin!(stops_with);
    let goes_within = config.timers.hub_goes_within;
    let worker = Worker::new(config);
    let mut hubs = Hubs { worker: &worker, goes_within, serving: None, next: None };
    let mut alone_since = Instant::now();
    let last = loop {
        tokio::select! {
            // A hub that has gone first, so one whose wait for that is over just then isn't refused.
            biased;
            closing = ending(&mut hubs.serving) => {
                match closing {
                    Ok(closing) if closing.ended == Ended::Stop => break Some(closing),
                    Ok(closing) => {
                        info!(name: "hub.went", "The hub went, and asked this worker to keep running without it");
                        without_hub(&worker).await;
                        closing.abandon();
                    }
                    Err(e) => warn!(name: "hub.serving_failed", error = %e, "Serving a hub failed"),
                }
                alone_since = Instant::now();
                // Only now that what the last hub asked for has stopped, so nothing of this one's stops with it.
                hubs.serve_next();
            }
            () = refusal_due(hubs.next.as_ref()) => hubs.refuse_next(),
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => hubs.connected(stream),
                Err(e) => {
                    // Such as for running out of file descriptors, which may pass.
                    warn!(name: "hub.connection_failed", error = %e, "A hub's connection couldn't be accepted");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
            () = tokio::time::sleep_until(alone_since + idle_limit), if hubs.serving.is_none() => {
                info!(
                    name: "hub.none_connected",
                    idle_limit = ?idle_limit,
                    "No hub has connected in a while, so agents are stopped"
                );
                break None;
            }
            // A hub that's connected stops with the same, and stops the worker as it does.
            () = &mut stops_with, if hubs.serving.is_none() => {
                info!(name: "worker.stops_with_gone", "What this worker stops with has gone, so agents are stopped");
                break None;
            }
        }
    };
    // Before anything stops, so a hub can't connect to a worker that's stopping, and starts another.
    let _ = std::fs::remove_file(socket);
    drop(listener);
    hubs.next = None;
    worker.stop(last).await;
    Ok(())
}

/// Carries `worker` on without the hub that was connected, which asked it to keep running without it: nothing more
/// goes to that hub, and no worktree's changes are watched, since each was watched for that hub, and the next hub says
/// again what it wants.
pub(super) async fn without_hub(worker: &Worker) {
    worker.state.outbox.detach().await;
    worker.state.watches.lock().stop_all_changes();
}

/// Only this user can connect.
fn bind(socket: &Path) -> io::Result<UnixListener> {
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket)?;
    private_files::restrict(socket)?;
    Ok(listener)
}

/// The hub being served, and the one waiting for it to go.
struct Hubs<'a> {
    worker: &'a Worker,
    /// How long the one waiting waits.
    goes_within: Duration,
    serving: Option<JoinHandle<Closing>>,
    /// Only while another is being served.
    next: Option<Next>,
}

/// A hub that connected while another was connected, and hasn't been read from yet.
struct Next {
    stream: UnixStream,
    /// When it's refused, if the other hasn't gone.
    refused_at: Instant,
}

impl Hubs<'_> {
    /// Serves the hub on `stream`, or has it wait for the one being served to go, in place of any already waiting,
    /// which is refused.
    fn connected(&mut self, stream: UnixStream) {
        if self.serving.is_none() {
            self.serve(stream);
            return;
        }
        self.refuse_next();
        self.next = Some(Next { stream, refused_at: Instant::now() + self.goes_within });
    }

    /// Once the one being served has gone, serves the one waiting, if there is one.
    fn serve_next(&mut self) {
        self.serving = None;
        if let Some(next) = self.next.take() {
            self.serve(next.stream);
        }
    }

    fn serve(&mut self, stream: UnixStream) {
        let (worker, (reader, writer)) = (self.worker.clone(), stream.into_split());
        self.serving = Some(tokio::spawn(async move { connection::serve(&worker, reader, writer, None).await }));
    }

    fn refuse_next(&mut self) {
        let Some(Next { stream, .. }) = self.next.take() else { return };
        let (worker, (reader, writer)) = (self.worker.clone(), stream.into_split());
        let refused = Some("another hub is connected to this worker".to_string());
        tokio::spawn(async move { connection::serve(&worker, reader, writer, refused).await.finish().await });
    }
}

/// Resolves once `next` is to be refused, and never while there's none.
#[expect(clippy::disallowed_methods, reason = "how long a hub waits for the one connected to go")]
async fn refusal_due(next: Option<&Next>) {
    match next {
        Some(next) => tokio::time::sleep_until(next.refused_at).await,
        None => std::future::pending().await,
    }
}

/// Resolves once the hub connected has stopped sending, and never while none is.
async fn ending(serving: &mut Option<JoinHandle<Closing>>) -> Result<Closing, JoinError> {
    match serving {
        Some(serving) => serving.await,
        None => std::future::pending().await,
    }
}

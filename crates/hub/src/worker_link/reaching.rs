//! Reaching a worker: starting its instance, if it's to be started, and then taking the connection its worker dials
//! the hub on, or, for one of the local provider's workers that doesn't dial, running the command it gives.

use std::time::Duration;

use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::InstanceId;
use providers::{Provider, ProviderError, WorkerLiveness};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, warn};

use super::carrier::Carrier;
use crate::child_process::{self, Spawned, Stderr};
use crate::error::provider_failed;
use crate::hub::Hub;
use crate::worker_dial::{Dialed, Endpoint};

/// The worker a link is opened to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opening {
    /// One started on the instance if it's not running already. What it sends goes to its sessions from the start.
    Started,
    /// One an earlier hub left running, which is looked for as the hub starts, for
    /// [`Timers::before_serving`](crate::Timers::before_serving) to dial the hub again, or its provider to say how to
    /// reach it, and then again and again while its workspace is disconnected. What it sends is held until
    /// [`WorkerLink::release`], so the hub can say first which agent each session has, and what the worker sends again
    /// of what the earlier hub missed is taken for that agent's.
    LeftRunning {
        /// How long its workspace has been disconnected, if it is, which paces how often its provider is asked whether
        /// it's gone: often at first, since a worker that has just died can read as running for a moment, then seldom.
        disconnected_for: Option<Duration>,
    },
}

/// A worker's stream, and what carries it.
pub(super) struct Reached {
    pub(super) carrier: Carrier,
    pub(super) reader: Box<dyn AsyncRead + Send + Unpin>,
    pub(super) writer: Box<dyn AsyncWrite + Send + Unpin>,
}

/// Reaches the instance's worker, starting the instance first if it's to be started, with the hub's worker builds for
/// the provider to put there, and a ticket for the worker to dial the hub with, at the endpoint the provider's
/// [`WorkerReach`](providers::WorkerReach) says. On failure nothing is left running that the hub started. A worker
/// [`Opening::LeftRunning`] has the hub's `before_serving` to dial, or its provider to say how to reach it, as for one
/// an earlier build left, which doesn't dial, and fails with `Unavailable` if it doesn't, which leaves the workspace
/// disconnected, or `Worker` if its provider says it's gone.
pub(super) async fn reach(
    hub: &Hub,
    provider: &dyn Provider,
    instance_id: &InstanceId,
    log: &std::path::Path,
    opening: Opening,
) -> Result<Reached, ApiError> {
    // Not one left running, which goes on writing to the log it has.
    if opening == Opening::Started {
        logging::move_aside_if_long(log);
    }
    let reached = match dialing(hub, provider, instance_id, opening).await? {
        // Boxed, since every future that can start a worker holds it, and binding an endpoint is a large one.
        Some(endpoint) => Box::pin(dialed(hub, provider, instance_id, endpoint, opening)).await,
        None => by_command(hub, provider, instance_id, log, opening).await,
    };
    if reached.is_err()
        && opening == Opening::Started
        && let Err(e) = provider.kill_worker(instance_id).await
    {
        warn!(
            name: "worker.unopened_unended",
            %instance_id,
            error = error_chain(&e),
            "Couldn't end a worker whose stream couldn't be opened"
        );
    }
    reached
}

/// The endpoint the instance's worker dials, or `None` if it doesn't: one whose provider's workers don't, or one an
/// earlier build left running, which has no ticket, nor a key the hub admitted it by, since it listens.
async fn dialing(
    hub: &Hub,
    provider: &dyn Provider,
    instance_id: &InstanceId,
    opening: Opening,
) -> Result<Option<Endpoint>, ApiError> {
    let endpoint = Endpoint::dialed_by(provider.workers_reach());
    if opening == Opening::Started || endpoint.is_none() {
        return Ok(endpoint);
    }
    let id = instance_id.clone();
    let dials = hub.blocking(move |store| store.instance_dials(&id)).await?;
    Ok(endpoint.filter(|_| dials))
}

/// The connection the instance's worker dials, at `endpoint`, which a worker started now dials with the ticket its
/// provider was given, and one an earlier hub left dials again by itself.
async fn dialed(
    hub: &Hub,
    provider: &dyn Provider,
    instance_id: &InstanceId,
    endpoint: Endpoint,
    opening: Opening,
) -> Result<Reached, ApiError> {
    let within = match opening {
        Opening::Started => {
            let ticket = hub.worker_ticket(instance_id, endpoint).await?;
            let workers = hub.worker_builds();
            provider.start(instance_id, workers, Some(&ticket)).await.map_err(|e| provider_failed(&e))?;
            provider.starts_or_stops_within()
        }
        Opening::LeftRunning { .. } => {
            hub.dial.listen(endpoint).await?;
            hub.timers.before_serving
        }
    };
    let dialed = match opening {
        Opening::Started => hub.dial.worker(instance_id, within).await,
        Opening::LeftRunning { disconnected_for } => {
            dial_unless_gone(hub, provider, instance_id, within, disconnected_for).await?
        }
    }
    .ok_or_else(|| didnt_dial(within, opening))?;
    Ok(Reached {
        carrier: Carrier::Dialed(dialed.connection),
        reader: Box::new(dialed.recv),
        writer: Box::new(dialed.send),
    })
}

/// How often the hub asks again whether a worker that hasn't dialed is gone. One that has just died can still read as
/// running for a moment, since the system lets go of its claim only as the last of its threads exits.
const ASK_AGAIN_EVERY: Duration = Duration::from_millis(250);
/// The longest between asks of a provider about a worker that has been disconnected for a long while, which an adapter
/// that can be asked costs a request each of.
const ASK_AGAIN_AT_MOST: Duration = Duration::from_secs(30);

/// The connection a worker an earlier hub left dials, if it does within `within`, or an error as soon as its provider
/// says it's gone, since a worker that's gone can't dial, and waiting out `within` for one would hold up every client
/// of the hub that's starting. Anything else the provider answers, or fails to within `within`, is no reason to stop
/// waiting: only proof is. A provider that can't say is asked no more.
async fn dial_unless_gone(
    hub: &Hub,
    provider: &dyn Provider,
    instance_id: &InstanceId,
    within: Duration,
    disconnected_for: Option<Duration>,
) -> Result<Option<Dialed>, ApiError> {
    // The first ask follows at once, the next as the time since the workspace was disconnected grows, up to the longest.
    let pace = disconnected_for.map_or(ASK_AGAIN_EVERY, |gone| (gone / 8).clamp(ASK_AGAIN_EVERY, ASK_AGAIN_AT_MOST));
    let gone = async {
        loop {
            match tokio::time::timeout(within, provider.worker_state(instance_id)).await {
                Ok(Ok(WorkerLiveness::Gone)) => return,
                Ok(Ok(state)) => {
                    debug!(name: "worker.state", %instance_id, ?state, "A worker an earlier hub left isn't gone");
                }
                // An adapter that can't say, which is most of them.
                Ok(Err(ProviderError::Unsupported(_))) => break,
                Ok(Err(ProviderError::Adapter(e))) if e.code == ErrorCode::Unsupported => break,
                Ok(Err(e)) => {
                    debug!(
                        name: "worker.state_unasked",
                        %instance_id,
                        error = error_chain(&e),
                        "Couldn't ask whether a worker an earlier hub left is running"
                    );
                }
                Err(_) => {}
            }
            pause(pace).await;
        }
        std::future::pending::<()>().await;
    };
    tokio::select! {
        dialed = hub.dial.worker(instance_id, within) => Ok(dialed),
        () = gone => Err(ApiError::new(ErrorCode::Worker, "the worker an earlier hub left is gone")),
    }
}

#[expect(clippy::disallowed_methods, reason = "the pace a provider is asked again at, until the worker dials")]
async fn pause(pause: Duration) {
    tokio::time::sleep(pause).await;
}

/// A worker started now that doesn't dial has failed, and one an earlier hub left that doesn't is unavailable, since
/// it may still be running, so asking again later may find it.
fn didnt_dial(within: Duration, opening: Opening) -> ApiError {
    let code = match opening {
        Opening::Started => ErrorCode::Worker,
        Opening::LeftRunning { .. } => ErrorCode::Unavailable,
    };
    ApiError::new(code, format!("the worker didn't dial the hub within {within:?}"))
}

/// Runs the command the local provider gives to reach one of its workers that doesn't dial.
async fn by_command(
    hub: &Hub,
    provider: &dyn Provider,
    instance_id: &InstanceId,
    log: &std::path::Path,
    opening: Opening,
) -> Result<Reached, ApiError> {
    let before_serving = hub.timers.before_serving;
    let command = match opening {
        Opening::Started => {
            provider.start(instance_id, hub.worker_builds(), None).await.map_err(|e| provider_failed(&e))?;
            provider.connect(instance_id).await.map_err(|e| provider_failed(&e))
        }
        Opening::LeftRunning { .. } => {
            match tokio::time::timeout(before_serving, provider.connect(instance_id)).await {
                Ok(command) => command.map_err(|e| provider_failed(&e)),
                Err(_) => Err(ApiError::new(
                    ErrorCode::Worker,
                    format!("the provider didn't say how to reach the worker within {before_serving:?}"),
                )),
            }
        }
    }?;
    let Spawned { process, reader, writer } =
        child_process::spawn(&command.program, &command.args, &command.env, &hub.machine, Stderr::Log(log))
            .map_err(|e| ApiError::new(ErrorCode::Worker, e.to_string()))?;
    Ok(Reached { carrier: Carrier::Command(process), reader, writer })
}

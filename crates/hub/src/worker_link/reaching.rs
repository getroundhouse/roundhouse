//! Reaching a worker: starting its instance, if it's to be started, and then taking the connection its worker dials
//! the hub on, or, for the local provider, running the command it gives.

use std::time::Duration;

use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{InstanceId, ProviderName};
use providers::Provider;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::warn;

use super::carrier::Carrier;
use crate::child_process::{self, Spawned, Stderr};
use crate::error::provider_failed;
use crate::hub::Hub;

/// The worker a link is opened to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opening {
    /// One started on the instance if it's not running already. What it sends goes to its sessions from the start.
    Started,
    /// One an earlier hub left running, as the hub starts and before it serves, so it has only
    /// [`Timers::before_serving`](crate::Timers::before_serving) to dial the hub again, or its provider to say how to
    /// reach it. What it sends is held until [`WorkerLink::release`], so the hub can say first which agent each
    /// session has, and what the worker sends again of what the earlier hub missed is taken for that agent's.
    LeftRunning,
}

/// A worker's stream, and what carries it.
pub(super) struct Reached {
    pub(super) carrier: Carrier,
    pub(super) reader: Box<dyn AsyncRead + Send + Unpin>,
    pub(super) writer: Box<dyn AsyncWrite + Send + Unpin>,
}

/// Whether the provider's workers dial the hub, which every provider's do but the local one's, which the hub reaches
/// by running the command its `connect` gives. *Not built:* the local provider's workers dialing too.
pub(crate) fn dials_its_hub(provider: &dyn Provider) -> bool {
    provider.name() != ProviderName::LOCAL
}

/// Reaches the instance's worker, starting the instance first if it's to be started, with the hub's worker builds for
/// the provider to put there, and a ticket for the worker to dial the hub with. On failure nothing is left running that
/// the hub started. A worker [`Opening::LeftRunning`] has the hub's `before_serving` to dial, or its provider to say how
/// to reach it.
pub(super) async fn reach(
    hub: &Hub,
    provider: &dyn Provider,
    instance_id: &InstanceId,
    log: &std::path::Path,
    opening: Opening,
) -> Result<Reached, ApiError> {
    // Boxed, since every future that can start a worker holds it, and binding an endpoint is a large one.
    let reached = if dials_its_hub(provider) {
        Box::pin(dialed(hub, provider, instance_id, opening)).await
    } else {
        by_command(hub, provider, instance_id, log, opening).await
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

/// The connection the instance's worker dials, which a worker started now dials with the ticket its provider was
/// given, and one an earlier hub left dials again by itself.
async fn dialed(
    hub: &Hub,
    provider: &dyn Provider,
    instance_id: &InstanceId,
    opening: Opening,
) -> Result<Reached, ApiError> {
    let within = match opening {
        Opening::Started => {
            let ticket = hub.worker_ticket(instance_id).await?;
            let workers = hub.worker_builds();
            provider.start(instance_id, workers, Some(&ticket)).await.map_err(|e| provider_failed(&e))?;
            provider.starts_or_stops_within()
        }
        Opening::LeftRunning => {
            hub.dial.listen().await?;
            hub.timers.before_serving
        }
    };
    let dialed = hub.dial.worker(instance_id, within).await.ok_or_else(|| didnt_dial(within))?;
    Ok(Reached {
        carrier: Carrier::Dialed(dialed.connection),
        reader: Box::new(dialed.recv),
        writer: Box::new(dialed.send),
    })
}

fn didnt_dial(within: Duration) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("the worker didn't dial the hub within {within:?}"))
}

/// Runs the command the local provider gives to reach its worker.
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
        Opening::LeftRunning => match tokio::time::timeout(before_serving, provider.connect(instance_id)).await {
            Ok(command) => command.map_err(|e| provider_failed(&e)),
            Err(_) => Err(ApiError::new(
                ErrorCode::Worker,
                format!("the provider didn't say how to reach the worker within {before_serving:?}"),
            )),
        },
    }?;
    // Not one left running, which goes on writing to the log it has.
    if opening == Opening::Started {
        logging::move_aside_if_long(log);
    }
    let Spawned { process, reader, writer } =
        child_process::spawn(&command.program, &command.args, &command.env, &hub.machine, Stderr::Log(log))
            .map_err(|e| ApiError::new(ErrorCode::Worker, e.to_string()))?;
    Ok(Reached { carrier: Carrier::Command(process), reader, writer })
}

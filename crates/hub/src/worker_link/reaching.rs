//! Reaching a worker: starting its instance, if it's to be started, and running the command its provider gives.

use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{InstanceId, ProviderName};
use providers::Provider;
use tracing::warn;

use crate::child_process::{self, Spawned, Stderr};
use crate::error::provider_failed;
use crate::hub::Hub;

/// The worker a link is opened to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opening {
    /// One started on the instance if it's not running already. What it sends goes to its sessions from the start.
    Started,
    /// One an earlier hub left running, as the hub starts and before it serves, so its provider has only
    /// [`Timers::before_serving`](crate::Timers::before_serving) to say how to reach it. What it sends is held until [`WorkerLink::release`], so the
    /// hub can say first which agent each session has, and what the worker sends again of what the earlier hub missed
    /// is taken for that agent's.
    LeftRunning,
}

/// Runs the command that reaches the instance's worker, starting the instance first if it's to be started, with the
/// hub's worker builds for the provider to put there, and a ticket for the worker. On failure nothing is left running
/// that the hub started. The provider has the hub's `before_serving` to say how to reach a worker
/// [`Opening::LeftRunning`].
pub(super) async fn reach(
    hub: &Hub,
    provider: &dyn Provider,
    instance_id: &InstanceId,
    log: &std::path::Path,
    opening: Opening,
) -> Result<Spawned, ApiError> {
    let (before_serving, machine) = (hub.timers.before_serving, &hub.machine);
    let command = match opening {
        Opening::Started => {
            // The local provider's workers don't dial the hub yet.
            let ticket = match provider.name() {
                ProviderName::LOCAL => None,
                _ => Some(hub.worker_ticket(instance_id).await?),
            };
            let workers = hub.worker_builds();
            provider.start(instance_id, workers, ticket.as_ref()).await.map_err(|e| provider_failed(&e))?;
            provider.connect(instance_id).await.map_err(|e| provider_failed(&e))
        }
        Opening::LeftRunning => match tokio::time::timeout(before_serving, provider.connect(instance_id)).await {
            Ok(command) => command.map_err(|e| provider_failed(&e)),
            Err(_) => Err(ApiError::new(
                ErrorCode::Worker,
                format!("the provider didn't say how to reach the worker within {before_serving:?}"),
            )),
        },
    };
    // Not one left running, which goes on writing to the log it has.
    if opening == Opening::Started {
        logging::move_aside_if_long(log);
    }
    let spawned = command.and_then(|command| {
        child_process::spawn(&command.program, &command.args, &command.env, machine, Stderr::Log(log))
            .map_err(|e| ApiError::new(ErrorCode::Worker, e.to_string()))
    });
    if spawned.is_err()
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
    spawned
}

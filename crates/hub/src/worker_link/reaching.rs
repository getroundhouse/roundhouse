//! Reaching a worker: starting its instance, if it's to be started, and running the command its provider gives.

use std::time::Duration;

use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::InstanceId;
use providers::Provider;
use tracing::warn;

use crate::child_process::{self, Spawned, Stderr};
use crate::error::provider_failed;
use crate::machine_environment::MachineEnvironment;

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

/// Runs the command that reaches the instance's worker, in `machine` with the command's own variables, starting the
/// instance first if it's to be started. On failure nothing is left running that the hub started. `before_serving` is
/// how long the provider has to say how to reach a worker [`Opening::LeftRunning`].
pub(super) async fn reach(
    provider: &dyn Provider,
    instance_id: &InstanceId,
    log: &std::path::Path,
    opening: Opening,
    before_serving: Duration,
    machine: &MachineEnvironment,
) -> Result<Spawned, ApiError> {
    let command = match opening {
        Opening::Started => {
            // The local provider runs the hub's own executable, so there are no other builds of the worker to give it.
            provider.start(instance_id, &[]).await.map_err(|e| provider_failed(&e))?;
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
    let spawned = match command {
        Ok(command) => {
            let given = machine.given(&command.env).await;
            child_process::spawn(&command.program, &command.args, &given, Stderr::Log(log))
                .map_err(|e| ApiError::new(ErrorCode::Worker, e.to_string()))
        }
        Err(e) => Err(e),
    };
    if spawned.is_err()
        && opening == Opening::Started
        && let Err(e) = provider.kill_worker(instance_id).await
    {
        warn!(%instance_id, "ending a worker whose stream couldn't be opened: {}", error_chain(&e));
    }
    spawned
}

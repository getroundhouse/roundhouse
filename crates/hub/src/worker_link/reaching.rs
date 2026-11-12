//! Reaching a worker: starting its instance, if it's to be started, and running the command its provider gives.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::InstanceId;
use providers::Provider;

use crate::child_process::{self, Spawned, Stderr};
use crate::error::provider_failed;
use crate::hub::BEFORE_SERVING;

/// The worker a link is opened to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opening {
    /// One started on the instance if it's not running already. What it sends goes to its sessions from the start.
    Started,
    /// One an earlier hub left running, as the hub starts and before it serves, so its provider has only
    /// [`BEFORE_SERVING`] to say how to reach it. What it sends is held until [`WorkerLink::release`], so the
    /// hub can say first which agent each session has, and what the worker sends again of what the earlier hub missed
    /// is taken for that agent's.
    LeftRunning,
}

/// Runs the command that reaches the instance's worker, starting the instance first if it's to be started. On failure
/// nothing is left running that the hub started.
pub(super) async fn reach(
    provider: &dyn Provider,
    instance_id: &InstanceId,
    log: &std::path::Path,
    opening: Opening,
) -> Result<Spawned, ApiError> {
    let command = match opening {
        Opening::Started => {
            // The local provider runs the hub's own executable, so there are no other builds of the worker to give it.
            provider.start(instance_id, &[]).await.map_err(|e| provider_failed(&e))?;
            provider.connect(instance_id).await.map_err(|e| provider_failed(&e))
        }
        Opening::LeftRunning => match tokio::time::timeout(BEFORE_SERVING, provider.connect(instance_id)).await {
            Ok(command) => command.map_err(|e| provider_failed(&e)),
            Err(_) => Err(ApiError::new(
                ErrorCode::Worker,
                format!("the provider didn't say how to reach the worker within {BEFORE_SERVING:?}"),
            )),
        },
    };
    let spawned = command.and_then(|command| {
        child_process::spawn(&command.program, &command.args, &command.env, Stderr::Log(log))
            .map_err(|e| ApiError::new(ErrorCode::Worker, e.to_string()))
    });
    if spawned.is_err() && opening == Opening::Started {
        let _ = provider.kill(instance_id).await;
    }
    spawned
}

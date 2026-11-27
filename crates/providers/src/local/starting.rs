//! Starting one of the local provider's workers that dials its hub.

use std::path::Path;
use std::process::Stdio;

use protocol::dialing::WorkerTicket;
use protocol::model::InstanceId;
use tokio::io::AsyncWriteExt as _;

use super::provider::LocalConfig;
use crate::ProviderError;

/// Runs `worker --detach` with `ticket`, which starts the worker in a session of its own, so it outlives the hub,
/// and returns once it has. The ticket goes on its stdin, as `--ticket -` reads it, and not in its arguments,
/// which any user on the machine can read: on its stdin it reaches only the worker, where the environment would
/// reach everything the worker starts, and a file could outlast it.
pub(super) async fn start_dialing(
    config: &LocalConfig,
    instance_id: &InstanceId,
    state_dir: &Path,
    ticket: &WorkerTicket,
) -> Result<(), ProviderError> {
    let failed = |e: std::io::Error| ProviderError::io("starting the worker", e);
    let log = protocol::paths::worker_log(&config.worker_logs_dir, instance_id);
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ProviderError::io(format!("creating {}", dir.display()), e))?;
    }
    let mut command = tokio::process::Command::new(&config.worker_exe);
    command
        .args(["worker", "--ticket", "-", "--detach", "--log"])
        .arg(&log)
        .arg("--workspaces-root")
        .arg(&config.workspaces_root)
        .arg("--state-dir")
        .arg(state_dir)
        .arg("--clones-dir")
        .arg(&config.clones_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    config.environment.apply(command.as_std_mut());
    let mut child = command.spawn().map_err(failed)?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(ticket.as_str().as_bytes()).await.map_err(failed)?;
    }
    let output = child.wait_with_output().await.map_err(failed)?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    let why = format!("the worker exited with {}: {}", output.status, said.trim());
    Err(failed(std::io::Error::other(why)))
}

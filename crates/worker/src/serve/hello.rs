//! What the worker answers a hub's hello with: the protocol it speaks, and which build it is.

use protocol::Build;
use protocol::client::{ApiError, ErrorCode};
use protocol::versions::Versions;
use protocol::worker::{HelloReply, WORKER_PROTOCOL_VERSION, WorkerResponse};

/// A worker speaks only its own version, and takes a hub whose hello names it, since a hub speaks the one before its
/// own too once a protocol breaks, as `docs/principles/versions.md` says.
pub(super) fn hello(build: Build, newest: u32, oldest: Option<u32>) -> Result<WorkerResponse, ApiError> {
    let (ours, hub) = (Versions::only(WORKER_PROTOCOL_VERSION), Versions::from_hello(newest, oldest));
    let Some(chosen) = ours.newest_shared(hub) else {
        let message = format!("the hub speaks {hub}, and this worker speaks {ours}");
        return Err(ApiError::new(ErrorCode::VersionMismatch, message));
    };
    Ok(WorkerResponse::Hello(HelloReply { protocol_version: chosen, build: Some(build.version().to_string()) }))
}

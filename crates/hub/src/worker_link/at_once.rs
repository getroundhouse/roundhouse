//! Calls a worker answers at once, given only so long, since a worker that takes longer is stuck, and a caller left
//! waiting on it could hold up whatever it holds.

use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::worker::Request;

use super::WorkerLink;

/// How long a worker has to answer a call it answers at once, doing nothing slow before it does, such as telling it to
/// watch, or asking what it holds in memory. A worker that takes this long is stuck, so the caller is better told so
/// than left waiting.
pub(crate) const ANSWERED_WITHIN: Duration = Duration::from_secs(10);

/// That a worker hadn't answered within [`ANSWERED_WITHIN`]. The request has reached it all the same, so it can still
/// do what was asked.
#[derive(Debug)]
pub(crate) struct Late;

impl From<Late> for ApiError {
    /// As a worker that can't do what was asked fails it.
    fn from(Late: Late) -> Self {
        ApiError::new(ErrorCode::Worker, format!("the worker didn't answer within {ANSWERED_WITHIN:?}"))
    }
}

impl WorkerLink {
    /// The worker's reply to `request`, one it answers at once, as [`WorkerLink::call`] gives it, or [`Late`] where it
    /// hasn't answered within [`ANSWERED_WITHIN`].
    pub(crate) async fn call_at_once<R: Request>(&self, request: R) -> Result<Result<R::Reply, ApiError>, Late> {
        answered_at_once(self.call(request)).await
    }
}

/// What `reply`, to a call a worker answers at once, gives, or [`Late`] where it hasn't come within [`ANSWERED_WITHIN`].
pub(crate) async fn answered_at_once<T>(
    reply: impl Future<Output = Result<T, ApiError>>,
) -> Result<Result<T, ApiError>, Late> {
    tokio::time::timeout(ANSWERED_WITHIN, reply).await.map_err(|_| Late)
}

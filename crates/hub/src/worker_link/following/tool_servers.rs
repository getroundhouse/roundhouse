//! The hub's tool servers, as a link follows them.

use protocol::client::{ApiError, ErrorCode};
use protocol::worker::ToolServersSet;
use tokio::sync::watch;
use tracing::info;

use super::super::WorkerLink;
use super::Followed;
use crate::tool_servers::SentToolServers;

/// The tool servers the hub has its workers pass calls on to, or `None` once the link's worker has said it passes on
/// none, as one from before they did says.
pub(in super::super) struct Servers(pub(in super::super) Option<watch::Receiver<SentToolServers>>);

impl Followed for Servers {
    type Set = ToolServersSet;

    const CALLED: &'static str = "tool servers";

    fn set(&mut self) -> ToolServersSet {
        self.0.as_mut().map_or_else(|| ToolServersSet { servers: Vec::new() }, |sent| sent.borrow_and_update().set())
    }

    async fn changed(&mut self) -> bool {
        match &mut self.0 {
            Some(sent) => sent.changed().await.is_ok(),
            None => false,
        }
    }

    /// One that doesn't know the request goes without, and so do its agents, and the hub sends it no more keys. A
    /// worker that passes calls on has nothing to refuse a set for, so any other refusal has failed it.
    fn refused(&mut self, link: &WorkerLink, refused: ApiError) -> Result<(), ApiError> {
        if refused.code != ErrorCode::Unsupported {
            return Err(refused);
        }
        self.0 = None;
        info!(
            name: "worker.tool_servers_unsupported",
            link = %link,
            "A worker is too old to pass calls on to the hub's tool servers, so its agents have none of them"
        );
        Ok(())
    }
}

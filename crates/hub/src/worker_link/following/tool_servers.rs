//! The hub's tool servers, as a link follows them.

use protocol::client::{ApiError, ErrorCode};
use protocol::worker::ToolServersSet;
use tokio::sync::watch;
use tracing::info;

use super::super::WorkerLink;
use super::Followed;
use crate::tool_servers::{SentToolServers, ServersAsSent};

/// The tool servers the hub has its workers pass calls on to.
pub(in super::super) struct Servers {
    /// `None` once the link's worker has said it passes on none, as one from before they did says.
    sent: Option<watch::Receiver<SentToolServers>>,
    /// How they were in the last set.
    last: ServersAsSent,
}

impl Servers {
    pub(in super::super) fn new(sent: watch::Receiver<SentToolServers>) -> Self {
        Self { sent: Some(sent), last: ServersAsSent::default() }
    }
}

impl Followed for Servers {
    type Set = ToolServersSet;

    const CALLED: &'static str = "tool servers";

    fn set(&mut self) -> ToolServersSet {
        let Some(sent) = &mut self.sent else { return ToolServersSet { servers: Vec::new() } };
        let sent = sent.borrow_and_update();
        self.last = sent.as_sent().clone();
        sent.set()
    }

    fn applied(&mut self, link: &WorkerLink) {
        link.has_tool_servers(&self.last);
    }

    async fn changed(&mut self) -> bool {
        match &mut self.sent {
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
        self.sent = None;
        info!(
            name: "worker.tool_servers_unsupported",
            link = %link,
            "A worker is too old to pass calls on to the hub's tool servers, so its agents have none of them"
        );
        Ok(())
    }
}

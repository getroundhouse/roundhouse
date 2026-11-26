use std::sync::Arc;

use protocol::client::tool_server_checks::{NoToolList, ToolServerCheck, ToolServerChecked};
use protocol::client::{ApiError, ErrorCode};
use tokio::sync::{Semaphore, SemaphorePermit};
use tracing::{field, info};

use super::super::{Hub, off_thread};
use crate::tool_servers::{Checking, ToAsk};

/// How many servers can be asked for their tools at once, by checks and saves together. One person adds one server at
/// a time, so a few let others beside them, and keep a client that asks without end from having the hub hold many.
/// *Unconfirmed.*
pub(super) const MOST_CHECKING: usize = 4;

/// The room there is for another server to be asked for its tools.
pub(super) struct Checks(Semaphore);

impl Default for Checks {
    fn default() -> Self {
        Self(Semaphore::new(MOST_CHECKING))
    }
}

impl Checks {
    /// Refused with `Unavailable` when as many are asked as may be.
    pub(super) fn begin(&self) -> Result<SemaphorePermit<'_>, ApiError> {
        let message = "as many MCP servers are being asked for their tools as may be at once; try again in a moment";
        self.0.try_acquire().map_err(|_| ApiError::new(ErrorCode::Unavailable, message))
    }
}

impl Hub {
    /// Checks a tool server for a client from before saves, as `ToolServerCheck` says. It's on its request's own task,
    /// which goes on if its client goes, within the time limit on asking.
    pub(crate) async fn tool_server_check(&self, asked: ToolServerCheck) -> Result<ToolServerChecked, ApiError> {
        let servers = Arc::clone(&self.tool_servers);
        let to_ask = match off_thread(move || servers.checking(asked)).await? {
            Checking::Nothing => return Ok(ToolServerChecked::NothingToAsk),
            Checking::Ask(to_ask) => to_ask,
        };
        let _asking = self.tool_sign_ins.checks.begin()?;
        Ok(match self.ask_for_tools(&to_ask).await {
            Ok(tools) => ToolServerChecked::Listed { tools },
            Err(why) => ToolServerChecked::NotListed { why },
        })
    }

    /// Asks a server for its tools as `to_ask` says, within the time limit on asking, with a place among those asked
    /// taken already, and answers with how many it listed, or why it listed none. It's logged, as what came of it, which
    /// is of kinds and statuses, so nothing of the URL, the key or what the server said is in it.
    pub(super) async fn ask_for_tools(&self, to_ask: &ToAsk) -> Result<u32, NoToolList> {
        let ToAsk { id, url, header } = to_ask;
        let listed = self.tool_sign_ins.client.list_tools(url, header.as_ref()).await;
        let came_of = match listed {
            Ok(tools) => ToolServerChecked::Listed { tools },
            Err(why) => ToolServerChecked::NotListed { why },
        };
        let server_id = id.as_ref().map(field::display);
        info!(name: "tool_server.checked", server_id, came_of = ?came_of, "Asked a tool server for its tools");
        listed
    }
}

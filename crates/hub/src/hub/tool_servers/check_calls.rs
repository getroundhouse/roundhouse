use std::sync::Arc;

use protocol::client::tool_server_checks::{ToolServerCheck, ToolServerChecked};
use protocol::client::{ApiError, ErrorCode};
use tokio::sync::{Semaphore, SemaphorePermit};
use tracing::{field, info};

use super::super::{Hub, off_thread};
use crate::tool_servers::{Checking, ToAsk};

/// How many servers can be asked for their tools at once. One person adds one server at a time, so a few let others
/// beside them, and keep a client that asks without end from having the hub hold many. *Unconfirmed.*
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
    fn begin(&self) -> Result<SemaphorePermit<'_>, ApiError> {
        let message = "as many MCP servers are being asked for their tools as may be at once; try again in a moment";
        self.0.try_acquire().map_err(|_| ApiError::new(ErrorCode::Unavailable, message))
    }
}

impl Hub {
    /// Checks a tool server, as `ToolServerCheck` says. It's on its request's own task, which goes on if its client
    /// goes, within the time limit on asking.
    pub(crate) async fn tool_server_check(&self, asked: ToolServerCheck) -> Result<ToolServerChecked, ApiError> {
        let servers = Arc::clone(&self.tool_servers);
        let ToAsk { id, url, header } = match off_thread(move || servers.checking(asked)).await? {
            Checking::Nothing => return Ok(ToolServerChecked::NothingToAsk),
            Checking::Ask(to_ask) => to_ask,
        };
        let _asking = self.tool_sign_ins.checks.begin()?;
        let checked = match self.tool_sign_ins.client.list_tools(&url, header.as_ref()).await {
            Ok(tools) => ToolServerChecked::Listed { tools },
            Err(why) => ToolServerChecked::NotListed { why },
        };
        // Which is of kinds and statuses, so nothing of the URL, the key or what the server said is in it.
        let server_id = id.as_ref().map(field::display);
        info!(name: "tool_server.checked", server_id, came_of = ?checked, "Asked a tool server for its tools");
        Ok(checked)
    }
}

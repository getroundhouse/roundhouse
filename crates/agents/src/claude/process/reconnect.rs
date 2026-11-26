//! Having the CLI connect again to a tool server, one it failed to connect to or one it's connected to, with
//! `mcp_reconnect`.

use protocol::capability::setup::ServerName;
use serde_json::json;

use super::{ClaudeProcess, write_line};
use crate::{AgentError, AgentOutput};

impl ClaudeProcess {
    /// Asks the CLI to connect to the server `server` again. Whether it did is what it answers, which the reader takes
    /// in order with everything else the CLI says, as
    /// [`ToolServers::answered`](super::super::tool_servers::ToolServers::answered) says, so nothing here waits for it:
    /// the CLI answers once it has shaken hands, which can take it 30 s, and until then it reads nothing else it's sent.
    pub(in super::super) async fn reconnect(&self, server: &ServerName) -> Result<(), AgentError> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let line = json!({"type": "control_request", "request_id": request_id,
            "request": {"subtype": "mcp_reconnect", "serverName": server}});
        // Before it's written, so an answer can't arrive first and be missed, or be overtaken by that it's starting.
        let starting = self.shared.tool_servers.lock().reconnecting(request_id.clone(), server.clone());
        if let Some(starting) = starting {
            self.shared.run.emit(AgentOutput::Report(starting)).await;
        }
        let written = write_line(&self.shared, &line).await;
        if written.is_err() {
            self.shared.tool_servers.lock().unasked(&request_id);
        }
        written
    }
}

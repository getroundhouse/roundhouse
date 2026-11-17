//! What Codex says for the person to read: its warnings, and an MCP server that didn't start.

use protocol::event::{EventBody, NoticeLevel};
use serde_json::Value;

use super::{running::Shared, subagents::Scope};

/// `warning`: a message Codex writes for the person, such as one about its configuration. The one it gives for the
/// under-development feature the adapter turns on is suppressed where Codex is launched.
pub(super) async fn warning(shared: &Shared, params: &Value, scope: &Scope, raw: String) {
    let body = match params["message"].as_str() {
        Some(message) => notice(NoticeLevel::Warning, message.into(), scope),
        None => EventBody::AgentOther { kind: "warning".into(), subagent: scope.parent.clone() },
    };
    shared.event(body, Some(raw)).await;
}

/// `mcpServer/startupStatus/updated`: an MCP server starting, ready, stopped or failed. Only its failing is the
/// person's to know, since its tools are then missing. The rest is state Codex keeps for its own interfaces.
pub(super) async fn mcp_server(shared: &Shared, params: &Value, scope: &Scope, raw: String) {
    let Some(message) = mcp_failure(params) else { return };
    shared.event(notice(NoticeLevel::Warning, message, scope), Some(raw)).await;
}

/// What to say of a failed MCP server, or `None` for one that didn't fail.
pub(super) fn mcp_failure(params: &Value) -> Option<String> {
    if params["status"].as_str() != Some("failed") {
        return None;
    }
    let name = params["name"].as_str().unwrap_or("An MCP server");
    let server = format!("The MCP server `{name}`");
    if params["failureReason"].as_str() == Some("reauthenticationRequired") {
        return Some(format!("{server} needs you to sign in to it again, so its tools aren't available."));
    }
    Some(match params["error"].as_str().map(str::trim).filter(|error| !error.is_empty()) {
        Some(error) => format!("{server} didn't start, so its tools aren't available: {error}"),
        None => format!("{server} didn't start, so its tools aren't available."),
    })
}

fn notice(level: NoticeLevel, message: String, scope: &Scope) -> EventBody {
    EventBody::Notice { level, message, subagent: scope.parent.clone() }
}

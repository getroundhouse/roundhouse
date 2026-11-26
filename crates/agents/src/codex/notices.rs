//! What Codex says for the person to read: its warnings and its errors.

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
    shared.run.event(body, Some(raw)).await;
}

/// `error`: a request of Codex's that failed, as it says.
pub(super) async fn error(shared: &Shared, params: &Value, scope: &Scope, raw: String) {
    let message = params["error"]["message"].as_str().unwrap_or("Codex's request failed");
    shared.run.event(notice(NoticeLevel::Error, message.into(), scope), Some(raw)).await;
}

fn notice(level: NoticeLevel, message: String, scope: &Scope) -> EventBody {
    EventBody::Notice { level, message, subagent: scope.parent.clone() }
}
